use crate::guidance::{Assembly, Guidance};
use crate::search::{Costs, Path, Search};
use crate::{EdgeSet, Graph, Highway, Network, NodeFlag, SpatialIndex, TrafficMap, TrafficSpeed, WayInfo};
use geo::{Coordinate, Polygon};
use routing::{Avoid, Route, RouteError, RouteRequest, TravelMode};

/// What the router accepts and how many alternatives it looks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouterOptions {
    /// How far a waypoint may be from the nearest road, in metres.
    pub max_snap_m: f64,
    /// How many alternatives to look for, besides the best route.
    pub alternatives: usize,
    /// An alternative is kept when it takes at most this many times the best route's time…
    pub max_alternative_ratio: f64,
    /// …and shares at most this fraction of its length with every route kept before it.
    pub max_shared_fraction: f64,
    /// The factor that the cost of the roads of the routes found is multiplied by, to find
    /// another.
    pub alternative_penalty: f32,
}

impl Default for RouterOptions {
    fn default() -> RouterOptions {
        RouterOptions { max_snap_m: 1_000.0, alternatives: 2, max_alternative_ratio: 1.4, max_shared_fraction: 0.7, alternative_penalty: 1.6 }
    }
}

/// Routes on a [`Graph`], with the traffic of now when it is given.
pub struct Router {
    pub(crate) graph: Graph,
    pub(crate) index: SpatialIndex,
    network: Network,
    search: Search,
    traffic: Option<TrafficMap>,
    pub options: RouterOptions,
}

impl Router {
    pub fn new(graph: Graph) -> Router {
        let index = SpatialIndex::new(&graph);
        let network = Network::new(&graph);
        let search = Search::new(&graph);
        Router { graph, index, network, search, traffic: None, options: RouterOptions::default() }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Whether every point is within the area of the graph.
    pub fn covers(&self, points: &[Coordinate]) -> bool {
        let area = self.graph.bounds.expanded_m(self.options.max_snap_m);
        points.iter().all(|p| area.contains(p))
    }

    /// Uses the speeds of now for the routes asked from now on (matched to the roads), or the
    /// usual speeds again with `None`. Returns how many edges the speeds fell on.
    pub fn set_traffic(&mut self, speeds: Option<&[TrafficSpeed]>) -> usize {
        self.traffic = speeds.map(|s| TrafficMap::matching(&self.graph, &self.index, s)).filter(|m| !m.is_empty());
        self.traffic.as_ref().map_or(0, TrafficMap::len)
    }

    /// Uses a traffic map made otherwise (for tests, or a source matched elsewhere).
    pub fn set_traffic_map(&mut self, map: Option<TrafficMap>) {
        self.traffic = map;
    }

    pub fn traffic(&self) -> Option<&TrafficMap> {
        self.traffic.as_ref()
    }

    pub fn route(&mut self, request: &RouteRequest) -> Result<Vec<Route>, RouteError> {
        if !request.is_valid() {
            return Err(RouteError::InvalidRequest("two or more valid waypoints are needed".into()));
        }
        if request.mode == TravelMode::Transit {
            return Err(RouteError::UnsupportedMode);
        }
        if !self.covers(&request.waypoints) {
            return Err(RouteError::OutOfCoverage);
        }
        self.network.prepare(&self.graph, request.mode);
        let excluded = self.edges_in(&request.effective_avoid_areas());
        let has_closures = self.traffic.as_ref().is_some_and(TrafficMap::has_closures);
        let routes = match self.route_with(request, &excluded, true) {
            // What cannot be avoided is not: the route without the strict avoidances, said so.
            Err(RouteError::NoRoute) if request.avoid.any() || !excluded.is_empty() || has_closures => self.route_with(request, &excluded, false)?,
            other => other?,
        };
        Ok(routes)
    }

    /// The edges whose roads touch one of `areas`, with their twins.
    fn edges_in(&self, areas: &[&Polygon]) -> EdgeSet {
        let mut out = EdgeSet::default();
        for area in areas {
            let Some(bounds) = area.bounds() else { continue };
            let reach = bounds.expanded_m(50.0);
            let mut tried = EdgeSet::default();
            for edge in self.index.within(&reach) {
                // An edge is in every cell it crosses: it is tested once.
                if out.contains(edge) || !tried.insert(edge) || !self.graph.edge_meets(edge, &reach) {
                    continue;
                }
                if area.touches_line(&self.graph.edge_points(edge)) {
                    out.insert(edge);
                    if let Some(twin) = self.graph.twin(edge) {
                        out.insert(twin);
                    }
                }
            }
        }
        out
    }

    fn costs<'a>(&'a self, request: &RouteRequest, excluded: &'a EdgeSet, penalized: &'a EdgeSet, strict: bool, heading: Option<f64>) -> Costs<'a> {
        Costs {
            mode: request.mode,
            avoid: request.avoid,
            strict,
            optimize: request.optimize,
            driving_side: request.driving_side,
            penalized,
            penalty: self.options.alternative_penalty,
            excluded,
            traffic: self.traffic.as_ref(),
            heading,
        }
    }

    fn route_with(&mut self, request: &RouteRequest, excluded: &EdgeSet, strict: bool) -> Result<Vec<Route>, RouteError> {
        let mode = request.mode;
        let snaps = request
            .waypoints
            .iter()
            .map(|p| {
                let snaps = Search::snaps(&self.graph, &self.index, p, mode, self.options.max_snap_m, Search::CANDIDATES);
                if snaps.is_empty() { Err(RouteError::NoRoad) } else { Ok(snaps) }
            })
            .collect::<Result<Vec<_>, _>>()?;
        // The search is taken out of the router while it runs, so that the costs may borrow the
        // rest of the router.
        let mut search = std::mem::replace(&mut self.search, Search::empty());
        let found = self.paths(&mut search, request, &snaps, excluded, strict);
        self.search = search;
        let paths = found?;
        let mut routes: Vec<Route> = paths.iter().map(|legs| self.assemble(legs, request, excluded)).collect();
        routes.sort_by(|a, b| a.expected_duration_s().total_cmp(&b.expected_duration_s()));
        Ok(routes)
    }

    /// The legs of the best route between the `snaps` of the waypoints, then those of the
    /// alternatives kept (with two waypoints only).
    fn paths(&self, search: &mut Search, request: &RouteRequest, snaps: &[Vec<crate::Snap>], excluded: &EdgeSet, strict: bool) -> Result<Vec<Vec<Path>>, RouteError> {
        let none = EdgeSet::default();
        let mut legs: Vec<Path> = Vec::with_capacity(snaps.len() - 1);
        for (i, pair) in snaps.windows(2).enumerate() {
            let heading = if i == 0 { request.heading } else { None };
            let costs = self.costs(request, excluded, &none, strict, heading);
            legs.push(search.route(&self.graph, &self.network, &pair[0], &pair[1], &costs).ok_or(RouteError::NoRoute)?);
        }
        let mut kept = vec![legs];
        if !request.alternatives || snaps.len() != 2 || self.options.alternatives == 0 {
            return Ok(kept);
        }
        // Every route found, kept or not, is penalised, so that the next search looks elsewhere.
        let real = self.costs(request, excluded, &none, strict, None);
        let best_cost = Search::path_cost(&self.graph, &self.network, &kept[0][0], &real);
        let mut penalized = EdgeSet::with_bound(self.graph.edges.len());
        let mut found: Vec<Path> = vec![kept[0][0].clone()];
        for _ in 0..self.options.alternatives * 2 {
            if kept.len() > self.options.alternatives {
                break;
            }
            if let Some(path) = found.last() {
                for &edge in &path.edges {
                    penalized.insert(edge);
                    if let Some(twin) = self.graph.twin(edge) {
                        penalized.insert(twin);
                    }
                }
            }
            let costs = self.costs(request, excluded, &penalized, strict, request.heading);
            let Some(candidate) = search.route(&self.graph, &self.network, &snaps[0], &snaps[1], &costs) else { break };
            // The real cost, without the penalties, on the same terms as the best route's.
            if Search::path_cost(&self.graph, &self.network, &candidate, &real) > best_cost * self.options.max_alternative_ratio {
                break;
            }
            let shared = kept.iter().map(|p| self.shared_fraction(&candidate, &p[0])).fold(0.0, f64::max);
            if shared <= self.options.max_shared_fraction {
                kept.push(vec![candidate.clone()]);
            }
            found.push(candidate);
        }
        Ok(kept)
    }

    /// The fraction of the length of `a` on roads that `b` also uses.
    fn shared_fraction(&self, a: &Path, b: &Path) -> f64 {
        let used: EdgeSet = b.edges.iter().map(|&e| self.graph.canonical(e)).collect();
        let (mut shared, mut total) = (0.0, 0.0);
        for &e in &a.edges {
            let length = self.graph.edges[e as usize].length_m();
            total += length;
            if used.contains(self.graph.canonical(e)) {
                shared += length;
            }
        }
        if total > 0.0 { shared / total } else { 1.0 }
    }

    fn assemble(&self, legs: &[Path], request: &RouteRequest, excluded: &EdgeSet) -> Route {
        let mode = request.mode;
        let mut assembly = Assembly::default();
        let legs_out: Vec<_> = legs.iter().map(|p| Guidance::leg(&self.graph, &self.network, p, mode, self.traffic.as_ref(), &mut assembly)).collect();
        // The edges travelled along, not those only touched at a waypoint, and the nodes passed.
        let mut all_edges: Vec<u32> = Vec::new();
        let mut nodes: Vec<u32> = Vec::new();
        for p in legs {
            let last = p.edges.len() - 1;
            for (i, &e) in p.edges.iter().enumerate() {
                let start = if i == 0 { p.start_fraction } else { 0.0 };
                let end = if i == last { p.end_fraction } else { 1.0 };
                if self.graph.edges[e as usize].length_m() * (end - start) >= 1.0 {
                    all_edges.push(e);
                }
                if i < last {
                    nodes.push(self.graph.edges[e as usize].target);
                }
            }
        }
        let ways = |test: &dyn Fn(&WayInfo) -> bool| all_edges.iter().any(|&e| test(self.graph.way_of(e)));
        let used = Avoid {
            tolls: ways(&|w| w.is_toll()),
            highways: ways(&|w| w.highway.is_motorway()),
            ferries: ways(&|w| w.highway == Highway::Ferry),
            unpaved: ways(&|w| w.has(WayInfo::UNPAVED)),
            steps: ways(&|w| w.highway == Highway::Steps),
            level_crossings: nodes.iter().any(|&n| self.graph.node_has(n, NodeFlag::LEVEL_CROSSING)),
            // Preferences, not rules: they are not said to be "not avoided".
            narrow: false,
            hills: false,
            crossing_turns: false,
            unlit: false,
        };
        let distance_m = legs_out.iter().map(|l| l.distance_m).sum();
        let duration_s: f64 = legs_out.iter().map(|l| l.duration_s).sum();
        let mut route = Route::new(std::mem::take(&mut assembly.geometry), legs_out, distance_m, duration_s);
        route.summary = Guidance::summary(&self.graph, &all_edges);
        route.has_tolls = used.tolls;
        route.has_ferries = used.ferries;
        route.ignored_avoid = request.avoid.and(&used);
        route.enters_avoided_area = all_edges.iter().any(|&e| excluded.contains(e));
        route.duration_in_traffic_s = self.traffic.as_ref().map(|_| assembly.traffic_s);
        route.speed_limits = assembly.speed_limits;
        route.congestion = assembly.congestion;
        route.alerts = assembly.alerts;
        if self.graph.has_elevations() && assembly.elevations.iter().all(Option::is_some) {
            route.elevations = assembly.elevations.into_iter().flatten().collect();
        }
        route
    }
}
