use crate::{EdgeSet, Graph, Highway, Network, NodeFlag, Profile, SpatialIndex, TrafficMap, WayInfo};
use geo::{Bounds, Coordinate, Line};
use routing::{Avoid, DrivingSide, Optimize, TravelMode};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Where a point falls on the roads: on `edge`, a `fraction` of its length from its source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    pub edge: u32,
    pub fraction: f64,
    pub point: Coordinate,
    pub distance_m: f64,
}

/// A path found by the search: the edges in order, entered a fraction `start_fraction` along
/// the first and left `end_fraction` along the last.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub edges: Vec<u32>,
    pub start_fraction: f64,
    pub end_fraction: f64,
    pub cost_s: f64,
}

/// What the search may use, what it avoids and what it minimises.
///
/// Avoidances are of two kinds:
/// - *strict* (tolls, motorways, ferries, unpaved roads, steps, level crossings, the areas and
///   closures in `excluded` and the closures of `traffic`): such roads are not used at all while
///   `strict` is set; the router searches again without them when nothing else leads there;
/// - *soft* (narrow and unlit roads, hills, turns across traffic): such roads and turns cost more,
///   so that they are taken only when they save much.
#[derive(Debug, Clone)]
pub struct Costs<'a> {
    pub mode: TravelMode,
    pub avoid: Avoid,
    pub strict: bool,
    pub optimize: Optimize,
    pub driving_side: DrivingSide,
    /// Edges whose cost is multiplied by `penalty`, to find alternatives.
    pub penalized: &'a EdgeSet,
    pub penalty: f32,
    /// Edges not to use while `strict` (in an area to avoid).
    pub excluded: &'a EdgeSet,
    /// The speeds of now, when known.
    pub traffic: Option<&'a TrafficMap>,
    /// The heading at the start, to prefer leaving in that direction.
    pub heading: Option<f64>,
}

impl Costs<'_> {
    /// The speed of a route that minimises distance, to turn metres into seconds of cost.
    const SHORTEST_MPS: f32 = 10.0;

    /// The least cost of a metre: the estimate of the search must never exceed a real cost.
    fn least_per_metre(&self, network: &Network) -> f32 {
        match self.optimize {
            Optimize::Shortest => 1.0 / Self::SHORTEST_MPS,
            _ => {
                // The smallest preference of a class (cycleways for bicycles), squared for `Simple`.
                let least_preference = match (self.mode, self.optimize) {
                    (TravelMode::Bicycle, Optimize::Simple) => 0.81,
                    (TravelMode::Bicycle, _) => 0.9,
                    _ => 1.0,
                };
                least_preference / network.max_speed(self.mode)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Item {
    key: f32,
    cost: f32,
    state: u32,
}

impl Eq for Item {}

impl Ord for Item {
    fn cmp(&self, other: &Self) -> Ordering {
        // The smallest key first (the heap is a max-heap).
        other.key.total_cmp(&self.key).then_with(|| other.state.cmp(&self.state))
    }
}

impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The working memory of searches on one graph, kept between searches: the costs are marked by
/// a generation, so that a new search needs no clearing.
pub struct Search {
    cost: Vec<f32>,
    pred: Vec<u32>,
    stamp: Vec<u32>,
    generation: u32,
    heap: BinaryHeap<Item>,
}

impl Search {
    const FINISH: u32 = u32::MAX;
    const START: u32 = u32::MAX;
    /// How much further than the nearest road another road may be to be a candidate, in metres.
    pub const CANDIDATE_MARGIN_M: f64 = 60.0;
    /// How many roads a waypoint is tried on.
    pub const CANDIDATES: usize = 5;
    /// The cost of reaching a road from its waypoint, in seconds a metre. Steep (a fifth of
    /// walking speed), so that a further road is taken only when the nearest leads nowhere, never
    /// as a shortcut through buildings or round a turn restriction.
    const APPROACH_S_PER_M: f32 = 3.6;
    /// A gradient steeper than this is steep for the mode, in percent: avoided with the hills.
    pub fn steep_percent(mode: TravelMode) -> u8 {
        match mode {
            TravelMode::Car => 12,
            TravelMode::Bicycle => 6,
            _ => 8,
        }
    }

    /// A search with no memory, to hold the place of one taken out for a while.
    pub fn empty() -> Search {
        Search { cost: Vec::new(), pred: Vec::new(), stamp: Vec::new(), generation: 0, heap: BinaryHeap::new() }
    }

    pub fn new(graph: &Graph) -> Search {
        let n = graph.edges.len();
        Search { cost: vec![f32::INFINITY; n], pred: vec![Self::START; n], stamp: vec![0; n], generation: 0, heap: BinaryHeap::new() }
    }

    /// The nearest road to `point` that `mode` may use, within `max_distance_m`.
    pub fn snap(graph: &Graph, index: &SpatialIndex, point: &Coordinate, mode: TravelMode, max_distance_m: f64) -> Option<Snap> {
        Self::snaps(graph, index, point, mode, max_distance_m, 1).into_iter().next()
    }

    /// The roads nearest to `point` that `mode` may use, nearest first: at most `count`, each
    /// within `max_distance_m` and within `Self::CANDIDATE_MARGIN_M` of the nearest. The nearest
    /// road may lead nowhere (a car park, a private yard), so a search starts from several.
    pub fn snaps(graph: &Graph, index: &SpatialIndex, point: &Coordinate, mode: TravelMode, max_distance_m: f64, count: usize) -> Vec<Snap> {
        let usable = |e: u32| graph.edges[e as usize].allows(mode) || graph.twin(e).is_some_and(|t| graph.edges[t as usize].allows(mode));
        let reach = Bounds { south: point.latitude, west: point.longitude, north: point.latitude, east: point.longitude }.expanded_m(max_distance_m);
        let mut found: Vec<Snap> = Vec::new();
        // The nearest cells first; further rings only while nothing near enough is found.
        let max_rings = ((max_distance_m / 500.0).ceil() as i32).clamp(1, 6);
        for rings in 1..=max_rings {
            found.clear();
            for edge in index.near(point, rings) {
                if !usable(edge) || !graph.edge_meets(edge, &reach) {
                    continue;
                }
                let line = Line::new(graph.edge_points(edge));
                let Some(p) = line.project(point, 0.0, f64::INFINITY) else { continue };
                if p.distance_m > max_distance_m {
                    continue;
                }
                let fraction = if line.length_m() > 0.0 { p.along_m / line.length_m() } else { 0.0 };
                let snap = Snap { edge, fraction, point: p.point, distance_m: p.distance_m };
                match found.iter_mut().find(|s| graph.canonical(s.edge) == graph.canonical(edge)) {
                    Some(existing) if snap.distance_m < existing.distance_m => *existing = snap,
                    Some(_) => {}
                    None => found.push(snap),
                }
            }
            let covered = rings as f64 * SpatialIndex::CELL_DEGREES * 111_000.0 * 0.5;
            if found.iter().any(|s| s.distance_m <= covered) {
                break;
            }
        }
        found.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        let nearest = found.first().map_or(0.0, |s| s.distance_m);
        found.retain(|s| s.distance_m <= nearest + Self::CANDIDATE_MARGIN_M);
        found.truncate(count);
        found
    }

    /// Whether `edge` is avoided strictly by `costs` (whatever is to be avoided and allowed).
    pub fn strictly_avoided(network: &Network, edge: u32, costs: &Costs) -> bool {
        let class = network.class_of(edge);
        let a = &costs.avoid;
        (a.tolls && class.has(WayInfo::TOLL))
            || (a.ferries && class.highway == Highway::Ferry)
            || (a.highways && class.highway.is_motorway())
            || (a.unpaved && class.has(WayInfo::UNPAVED))
            || (a.steps && class.highway == Highway::Steps)
            || costs.excluded.contains(edge)
            || costs.traffic.is_some_and(|t| t.is_closed(edge))
    }

    /// The cost of travelling the whole of `edge`, or `None` when it may not be used (by the mode,
    /// or strictly avoided).
    pub fn edge_cost(graph: &Graph, network: &Network, edge: u32, costs: &Costs) -> Option<f32> {
        if costs.strict && Self::strictly_avoided(network, edge, costs) {
            return None;
        }
        Self::travel_cost(graph, network, edge, costs)
    }

    /// The time in seconds to travel the whole of `edge` now: with the traffic when known.
    /// `None` when the mode may not use it.
    pub fn travel_time(graph: &Graph, network: &Network, edge: u32, mode: TravelMode, traffic: Option<&TrafficMap>) -> Option<f32> {
        let time = network.time(edge, mode);
        if !time.is_finite() {
            return None;
        }
        match traffic.and_then(|t| t.factor(edge)) {
            // Slower or faster than usual, but never faster than the fastest edge of the mode, so
            // that the estimate of the search stays below every real cost.
            Some(factor) if factor > 0.0 => {
                let floor = graph.edges[edge as usize].length_m() as f32 / network.max_speed(mode);
                Some((time / factor).max(floor))
            }
            _ => Some(time),
        }
    }

    /// The cost of travelling the whole of `edge`, whatever is avoided strictly (the road a
    /// waypoint is on is used, avoided or not). `None` when the mode may not use it.
    pub fn travel_cost(graph: &Graph, network: &Network, edge: u32, costs: &Costs) -> Option<f32> {
        let mode = costs.mode;
        let seconds = Self::travel_time(graph, network, edge, mode, costs.traffic)?;
        let e = &graph.edges[edge as usize];
        let way = network.class_of(edge);
        let profile = Profile::new(mode);
        let climb = network.climb_of(edge);
        let mut cost = match costs.optimize {
            Optimize::Fastest => seconds * profile.preference(way.highway),
            Optimize::Shortest => e.length_m() as f32 / Costs::SHORTEST_MPS,
            Optimize::Simple => seconds * profile.preference(way.highway).powi(2),
            Optimize::Eco => {
                // Fuel and effort grow with speed beyond about 80 km/h, and with every climb.
                let speed_kmh = e.length_m() as f32 / seconds.max(0.01) * 3.6;
                let fast = if mode == TravelMode::Car { 1.0 + (speed_kmh - 80.0).max(0.0) / 100.0 } else { 1.0 };
                let climb_s = climb.up_m() as f32 * if mode == TravelMode::Car { 1.5 } else { 4.0 };
                seconds * profile.preference(way.highway) * fast + climb_s
            }
        };
        // A road avoided strictly but used all the same (no other way, or a waypoint is on it)
        // still costs more; a closed one much more.
        if costs.traffic.is_some_and(|t| t.is_closed(edge)) {
            cost *= 20.0;
        } else if !costs.strict && Self::strictly_avoided(network, edge, costs) {
            cost *= 4.0;
        }
        let a = &costs.avoid;
        if a.narrow && way.has(WayInfo::NARROW) {
            cost *= match mode {
                TravelMode::Car => 3.0,
                TravelMode::Bicycle => 1.3,
                _ => 1.0,
            };
        }
        if a.unlit && mode != TravelMode::Car && way.has(WayInfo::UNLIT) {
            cost *= 3.0;
        }
        if a.unlit && mode != TravelMode::Car && !way.has(WayInfo::LIT) && !way.has(WayInfo::UNLIT) {
            // Unknown: perhaps unlit.
            cost *= 1.3;
        }
        if a.hills {
            // Each metre climbed is worth this many seconds, and steep climbs much more.
            let per_metre = match mode {
                TravelMode::Car => 1.0,
                TravelMode::Bicycle => 12.0,
                _ => 8.0,
            };
            cost += climb.up_m() as f32 * per_metre;
            if climb.max_up_percent > Self::steep_percent(mode) {
                cost *= 3.0;
            }
        }
        if costs.penalized.contains(edge) {
            cost *= costs.penalty;
        }
        Some(cost)
    }

    /// The cost of turning from `from` into `to` at their junction, or `None` when the turn is
    /// not allowed (a restriction, a U-turn where another way exists, a barrier, a level crossing
    /// avoided strictly).
    pub fn turn_cost(graph: &Graph, network: &Network, from: u32, to: u32, costs: &Costs) -> Option<f32> {
        let mode = costs.mode;
        let e = &graph.edges[from as usize];
        let f = &graph.edges[to as usize];
        let via = e.target;
        debug_assert_eq!(graph.source(to), via);
        if graph.node_has(via, NodeFlag::blocks(mode)) {
            return None;
        }
        if costs.strict && costs.avoid.level_crossings && graph.node_has(via, NodeFlag::LEVEL_CROSSING) {
            return None;
        }
        if graph.twin(from) == Some(to) {
            // Only at a dead end, where there is no other way on.
            let other = graph.out_edges(via).any(|g| g != to && graph.edges[g as usize].allows(mode));
            if other {
                return None;
            }
            return Some(match mode {
                TravelMode::Car => 60.0,
                TravelMode::Bicycle => 10.0,
                _ => 2.0,
            });
        }
        if mode == TravelMode::Car {
            for r in graph.restrictions_at(via) {
                // A restriction back onto its own way (`no_u_turn`) is what the U-turn rule above
                // already enforces; read as a turn, it would forbid going straight on.
                if r.from_way == r.to_way && !r.only {
                    continue;
                }
                if r.from_way == e.way {
                    if r.only && r.to_way != f.way {
                        return None;
                    }
                    if !r.only && r.to_way == f.way {
                        return None;
                    }
                }
            }
        }
        let angle = network.turn_angle(from, to);
        let sharp = angle.abs();
        let mut turn = match mode {
            TravelMode::Car => match sharp {
                a if a < 30.0 => 0.0,
                a if a < 100.0 => 6.0,
                _ => 12.0,
            },
            TravelMode::Bicycle => match sharp {
                a if a < 45.0 => 0.0,
                _ => 3.0,
            },
            _ => 0.0,
        };
        if costs.avoid.crossing_turns && sharp >= 30.0 && costs.driving_side.crosses(angle > 0.0) {
            turn += match mode {
                TravelMode::Car => 45.0,
                TravelMode::Bicycle => 15.0,
                _ => 0.0,
            };
        }
        let delay = Self::node_delay(graph, via, mode);
        Some(match costs.optimize {
            Optimize::Fastest => turn + delay,
            Optimize::Shortest => (turn + delay) * 0.2,
            Optimize::Simple => turn * 3.0 + delay,
            Optimize::Eco => turn * 1.5 + delay * 1.5,
        })
    }

    /// The average wait at `node`: about half of a red phase at signals, a stop at a stop sign or
    /// a level crossing, the opening of a gate.
    pub fn node_delay(graph: &Graph, node: u32, mode: TravelMode) -> f32 {
        let Some(&flags) = graph.node_flags.get(node as usize) else { return 0.0 };
        if flags == 0 {
            return 0.0;
        }
        let mut delay = 0.0;
        let has = |f: u16| flags & f != 0;
        if has(NodeFlag::SIGNALS) {
            delay += match mode {
                TravelMode::Car => 15.0,
                TravelMode::Bicycle => 12.0,
                _ => 10.0,
            };
        }
        if has(NodeFlag::LEVEL_CROSSING) {
            delay += match mode {
                TravelMode::Car => 15.0,
                TravelMode::Bicycle => 10.0,
                _ => 8.0,
            };
        }
        if has(NodeFlag::STOP) {
            delay += if mode == TravelMode::Foot { 0.0 } else { 5.0 };
        }
        if has(NodeFlag::GIVE_WAY) {
            delay += if mode == TravelMode::Foot { 0.0 } else { 2.0 };
        }
        if has(NodeFlag::TOLL_BOOTH) && mode == TravelMode::Car {
            delay += 10.0;
        }
        delay
    }

    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.generation = 1;
        }
        self.heap.clear();
    }

    fn cost_of(&self, edge: u32) -> f32 {
        if self.stamp[edge as usize] == self.generation { self.cost[edge as usize] } else { f32::INFINITY }
    }

    fn set(&mut self, edge: u32, cost: f32, pred: u32) {
        let i = edge as usize;
        self.stamp[i] = self.generation;
        self.cost[i] = cost;
        self.pred[i] = pred;
    }

    /// The directions of the roads of snaps: each snapped edge and its twin, with the fraction
    /// of the point along it and the cost of reaching it.
    fn directions(graph: &Graph, snaps: &[Snap]) -> Vec<(u32, f64, f32)> {
        let mut out = Vec::with_capacity(snaps.len() * 2);
        for snap in snaps {
            let approach = snap.distance_m as f32 * Self::APPROACH_S_PER_M;
            out.push((snap.edge, snap.fraction, approach));
            if let Some(twin) = graph.twin(snap.edge) {
                out.push((twin, 1.0 - snap.fraction, approach));
            }
        }
        out
    }

    /// The cheapest path from one of the roads `from` to one of the roads `to`, or `None` when
    /// there is none. `network` must be prepared for the mode.
    pub fn route(&mut self, graph: &Graph, network: &Network, from: &[Snap], to: &[Snap], costs: &Costs) -> Option<Path> {
        self.reset();
        let goal = Network::point_of(&to.first()?.point);
        let per_metre = costs.least_per_metre(network);
        // The estimate is to the nearest road of the destination. Another of its roads may be
        // nearer to a node, by at most the distances of both roads from the destination; but its
        // approach costs `APPROACH_S_PER_M` a metre, more than twice the cost of a metre at any
        // speed: the estimate stays below the real cost to every road.
        let heuristic = |node: u32| per_metre * network.chord_m(node, &goal);
        let targets = Self::directions(graph, to);
        // The best way found to the destination: its cost, the edge before the last, the last
        // edge and how far along it.
        let mut best: Option<(f32, u32, u32, f64)> = None;

        // A road is used even if it is to be avoided when a waypoint is on it, part way along;
        // not when the waypoint is at its end, where another road can be taken.
        const AT_END: f64 = 0.01;
        let starts = Self::directions(graph, from);
        for &(edge, fraction, approach) in &starts {
            let full = if fraction > AT_END { Self::travel_cost(graph, network, edge, costs) } else { Self::edge_cost(graph, network, edge, costs) };
            let Some(full) = full else { continue };
            let mut cost = approach + (1.0 - fraction as f32) * full;
            if let Some(heading) = costs.heading {
                let line = Line::new(graph.edge_points(edge));
                if let Some(bearing) = line.bearing_at(fraction * line.length_m())
                    && Coordinate::bearing_difference(heading, bearing).abs() > 90.0
                {
                    // Turning round costs time and is often not possible at once.
                    cost += 90.0;
                }
            }
            // Both points on this edge, the destination ahead.
            for &(target, target_fraction, target_approach) in &targets {
                if target == edge && target_fraction >= fraction {
                    let direct = cost - (1.0 - target_fraction as f32) * full + target_approach;
                    if best.is_none_or(|b| direct < b.0) {
                        best = Some((direct, Self::START, edge, target_fraction));
                        self.heap.push(Item { key: direct, cost: direct, state: Self::FINISH });
                    }
                }
            }
            if cost < self.cost_of(edge) {
                self.set(edge, cost, Self::START);
                self.heap.push(Item { key: cost + heuristic(graph.edges[edge as usize].target), cost, state: edge });
            }
        }

        while let Some(item) = self.heap.pop() {
            if best.is_some_and(|b| item.key >= b.0) {
                break;
            }
            if item.state == Self::FINISH {
                continue;
            }
            let edge = item.state;
            if item.cost > self.cost_of(edge) {
                continue;
            }
            let via = graph.edges[edge as usize].target;
            for next in graph.out_edges(via) {
                // Each edge's cost is computed once here: `edge_cost` is `travel_cost` with the
                // test of strict avoidance before it.
                let Some(travel) = Self::travel_cost(graph, network, next, costs) else { continue };
                let Some(turn) = Self::turn_cost(graph, network, edge, next, costs) else { continue };
                let entered = item.cost + turn;
                let usable = !(costs.strict && Self::strictly_avoided(network, next, costs));
                for &(target, target_fraction, target_approach) in &targets {
                    if target == next && (target_fraction < 1.0 - AT_END || usable) {
                        let finish = entered + target_fraction as f32 * travel + target_approach;
                        if best.is_none_or(|b| finish < b.0) {
                            best = Some((finish, edge, next, target_fraction));
                            self.heap.push(Item { key: finish, cost: finish, state: Self::FINISH });
                        }
                    }
                }
                if !usable {
                    continue;
                }
                let cost = entered + travel;
                if cost < self.cost_of(next) {
                    self.set(next, cost, edge);
                    let key = cost + heuristic(graph.edges[next as usize].target);
                    // A state no better than the best way found would only be popped to stop.
                    if best.is_none_or(|b| key < b.0) {
                        self.heap.push(Item { key, cost, state: next });
                    }
                }
            }
        }

        let (cost, before, last, end_fraction) = best?;
        let mut edges = vec![last];
        let mut current = before;
        // Each edge is set once per search with a predecessor of lower cost: no cycle.
        while current != Self::START {
            edges.push(current);
            current = self.pred[current as usize];
            if edges.len() > graph.edges.len() + 1 {
                return None;
            }
        }
        edges.reverse();
        let first = edges[0];
        let start_fraction = starts.into_iter().find(|(e, _, _)| *e == first).map_or(0.0, |(_, f, _)| f);
        Some(Path { edges, start_fraction, end_fraction, cost_s: cost as f64 })
    }

    /// The cost of `path` by `costs`, without the approach to its ends: the part of its first
    /// and last edges travelled, the whole of the others, and its turns. To compare paths found
    /// with other costs (the penalties of alternatives) on the same terms.
    pub fn path_cost(graph: &Graph, network: &Network, path: &Path, costs: &Costs) -> f64 {
        let last = path.edges.len().saturating_sub(1);
        let mut total = 0.0;
        for (i, &edge) in path.edges.iter().enumerate() {
            let from = if i == 0 { path.start_fraction } else { 0.0 };
            let to = if i == last { path.end_fraction } else { 1.0 };
            total += Self::travel_cost(graph, network, edge, costs).unwrap_or(0.0) as f64 * (to - from).max(0.0);
            if i > 0 {
                total += Self::turn_cost(graph, network, path.edges[i - 1], edge, costs).unwrap_or(0.0) as f64;
            }
        }
        total
    }

    /// The points of `path`, from the start point to the end point.
    pub fn path_points(graph: &Graph, path: &Path) -> Vec<Coordinate> {
        let mut points: Vec<Coordinate> = Vec::new();
        let last = path.edges.len() - 1;
        for (i, &edge) in path.edges.iter().enumerate() {
            let line = Line::new(graph.edge_points(edge));
            let length = line.length_m();
            let from = if i == 0 { path.start_fraction * length } else { 0.0 };
            let to = if i == last { path.end_fraction * length } else { length };
            let part = line.slice(from, to);
            let skip = usize::from(!points.is_empty());
            points.extend(part.into_iter().skip(skip));
        }
        points.dedup_by(|a, b| a.distance_m(b) < 0.01);
        points
    }
}
