use crate::search::{Path, Search};
use crate::{Graph, Highway, Network, NodeFlag, NodeNameKind, TrafficMap};
use geo::{Coordinate, Line};
use routing::{Alert, AlertKind, Congestion, Lane, Leg, Maneuver, ManeuverKind, Modifier, Span, Step, TravelMode};
use std::collections::HashMap;

/// The part of a path on one edge: where it starts in the geometry of the leg, its length and
/// the time to travel it (as usual, and with the traffic of now).
struct Piece {
    edge: u32,
    start: usize,
    distance_m: f64,
    duration_s: f64,
    traffic_s: f64,
}

/// A maneuver found at the start of a piece.
struct Found {
    piece: usize,
    kind: ManeuverKind,
    modifier: Option<Modifier>,
    exit: Option<u32>,
    angle: f64,
}

/// What the legs of a route add up to as they are made: the geometry, and what is known along
/// it (elevations, speed limits, congestion, alerts) at the same indices.
#[derive(Debug, Default)]
pub struct Assembly {
    pub geometry: Vec<Coordinate>,
    /// One per point of `geometry` (`None` where unknown).
    pub elevations: Vec<Option<f32>>,
    pub speed_limits: Vec<Span<u16>>,
    pub congestion: Vec<Span<Congestion>>,
    pub alerts: Vec<Alert>,
    /// The time with the traffic of now, in seconds.
    pub traffic_s: f64,
}

/// Turn-by-turn guidance from a path on the graph: where the traveller has to do something,
/// and what.
pub struct Guidance;

impl Guidance {
    /// Below this angle a road goes straight on.
    const STRAIGHT_DEGREES: f64 = 25.0;
    /// Two ways within this angle of each other are a fork.
    const FORK_DEGREES: f64 = 40.0;

    /// The elevations of the points `line.slice(from, to)` gives, from `elevations` of the line's
    /// points.
    fn slice_elevations(line: &Line, elevations: &[Option<f32>], from: f64, to: f64) -> Vec<Option<f32>> {
        let at = |d: f64| -> Option<f32> {
            let n = line.points().len();
            if n < 2 {
                return elevations.first().copied().flatten();
            }
            let s = line.segment_at(d);
            let (a, b) = (elevations.get(s).copied().flatten(), elevations.get(s + 1).copied().flatten());
            let (start, end) = (line.along_m(s), line.along_m(s + 1));
            let t = if end > start { ((d - start) / (end - start)).clamp(0.0, 1.0) as f32 } else { 0.0 };
            match (a, b) {
                (Some(a), Some(b)) => Some(a + (b - a) * t),
                (a, b) => a.or(b),
            }
        };
        let mut out = vec![at(from)];
        let first = line.segment_at(from) + 1;
        let last = line.segment_at(to);
        if first <= last {
            out.extend((first..=last).map(|i| elevations.get(i).copied().flatten()));
        }
        out.push(at(to));
        out
    }

    /// The leg of `path`, added to `assembly`: the steps index into its geometry, which begins
    /// with the points of earlier legs.
    pub fn leg(graph: &Graph, network: &Network, path: &Path, mode: TravelMode, traffic: Option<&TrafficMap>, assembly: &mut Assembly) -> Leg {
        let offset = assembly.geometry.len().saturating_sub(usize::from(!assembly.geometry.is_empty()));
        let mut points: Vec<Coordinate> = Vec::new();
        let mut heights: Vec<Option<f32>> = Vec::new();
        let mut pieces = Vec::with_capacity(path.edges.len());
        let last = path.edges.len() - 1;
        for (i, &edge) in path.edges.iter().enumerate() {
            let line = Line::new(graph.edge_points(edge));
            let length = line.length_m();
            let from = if i == 0 { path.start_fraction } else { 0.0 } * length;
            let to = if i == last { path.end_fraction * length } else { length }.max(from);
            let part = line.slice(from, to);
            let part_heights = match graph.edge_elevations(edge) {
                Some(e) => Self::slice_elevations(&line, &e, from, to),
                None => vec![None; part.len()],
            };
            let skip = usize::from(!points.is_empty());
            let start = points.len().saturating_sub(skip);
            points.extend(part.into_iter().skip(skip));
            heights.extend(part_heights.into_iter().skip(skip));
            let share = if length > 0.0 { (to - from).max(0.0) / length } else { 0.0 };
            let full = network.time(edge, mode);
            let full = if full.is_finite() { full as f64 } else { 0.0 };
            let now = Search::travel_time(graph, network, edge, mode, traffic).map_or(full, |t| t as f64);
            pieces.push(Piece { edge, start, distance_m: (to - from).max(0.0), duration_s: full * share, traffic_s: now * share });
        }
        if points.len() < 2 {
            // A path of no length: the points coincide.
            let p = points.first().copied().unwrap_or_else(|| graph.node_coordinate(graph.source(path.edges[0])));
            let h = heights.first().copied().flatten();
            points = vec![p, p];
            heights = vec![h, h];
        }
        let point_count = points.len();
        let piece_end = |i: usize| pieces.get(i + 1).map_or(point_count - 1, |p| p.start.min(point_count - 1)).max(pieces[i].start.min(point_count - 1));

        // What is known along the leg.
        for (i, piece) in pieces.iter().enumerate() {
            let e = &graph.edges[piece.edge as usize];
            let way = graph.way_of(piece.edge);
            let (start, end) = (offset + piece.start.min(point_count - 1), offset + piece_end(i));
            let limit = way.maxspeed(e.is_reversed());
            if limit > 0 && way.highway != Highway::Ferry {
                Span::push(&mut assembly.speed_limits, start, end, limit as u16);
            }
            if let Some(t) = traffic
                && let Some(factor) = t.factor(piece.edge)
            {
                let congestion = if factor <= 0.0 { Congestion::Closed } else { Congestion::of_speed_factor(factor as f64) };
                Span::push(&mut assembly.congestion, start, end, congestion);
            }
            if i > 0 {
                let via = graph.source(piece.edge);
                let location = points[piece.start.min(point_count - 1)];
                for (flag, kind) in [
                    (NodeFlag::LEVEL_CROSSING, AlertKind::LevelCrossing),
                    (NodeFlag::SPEED_CAMERA, AlertKind::SpeedCamera),
                    (NodeFlag::TOLL_BOOTH, AlertKind::TollBooth),
                ] {
                    if graph.node_has(via, flag) {
                        assembly.alerts.push(Alert { kind, index: start, location, text: None });
                    }
                }
            }
            let steep = network.climb_of(piece.edge).max_up_percent > Search::steep_percent(mode);
            let previous_steep = i > 0 && network.climb_of(pieces[i - 1].edge).max_up_percent > Search::steep_percent(mode);
            if steep && !previous_steep && piece.distance_m >= Network::GRADE_SPAN_M {
                let location = points[piece.start.min(point_count - 1)];
                let text = Some(format!("{}%", network.climb_of(piece.edge).max_up_percent));
                assembly.alerts.push(Alert { kind: AlertKind::Steep, index: start, location, text });
            }
        }

        let found = Self::maneuvers(graph, &pieces, mode);
        let mut steps = Vec::with_capacity(found.len() + 1);
        for (n, f) in found.iter().enumerate() {
            let next_piece = found.get(n + 1).map_or(pieces.len(), |g| g.piece);
            let start = pieces[f.piece].start.min(point_count - 1);
            let end = found.get(n + 1).map_or(point_count - 1, |g| pieces[g.piece].start.min(point_count - 1)).max(start);
            let (distance, duration) = pieces[f.piece..next_piece].iter().fold((0.0, 0.0), |(d, t), p| (d + p.distance_m, t + p.duration_s));
            // The road of the step: the one taken after the maneuver (after a roundabout, the one
            // it is left by).
            let road_piece = if f.kind == ManeuverKind::Roundabout {
                (f.piece..next_piece).find(|&i| !Self::roundabout(graph, pieces[i].edge)).unwrap_or(f.piece)
            } else {
                f.piece
            };
            let way = graph.way_of(pieces[road_piece].edge);
            let mut maneuver = Maneuver::new(f.kind, f.modifier, points[start]);
            maneuver.exit = f.exit;
            maneuver.bearing_after = points.get(start + 1).map(|next| points[start].bearing_to(next));
            if start > 0 {
                maneuver.bearing_before = Some(points[start - 1].bearing_to(&points[start]));
            }
            let mut step = Step::new(maneuver, distance, duration, offset + start, offset + end);
            step.name = graph.string(way.name).map(str::to_string);
            step.road_ref = graph.string(way.road_ref).map(str::to_string);
            if f.piece > 0 && f.kind != ManeuverKind::Depart {
                let arriving = pieces[f.piece - 1].edge;
                let via = graph.edges[arriving as usize].target;
                step.junction = graph.node_name(via, NodeNameKind::Name).map(str::to_string);
                if f.kind == ManeuverKind::OffRamp || graph.node_has(via, NodeFlag::MOTORWAY_JUNCTION) {
                    step.exit_number = graph.node_name(via, NodeNameKind::Ref).map(str::to_string);
                }
                step.lanes = Self::lanes(graph, arriving, f.angle, f.kind);
            }
            step.destinations = Self::destinations(graph, &pieces[f.piece..next_piece]);
            steps.push(step);
        }
        let end = point_count - 1;
        let mut arrive = Maneuver::new(ManeuverKind::Arrive, None, points[end]);
        arrive.bearing_before = Some(points[end - 1].bearing_to(&points[end]));
        steps.push(Step::new(arrive, 0.0, 0.0, offset + end, offset + end));
        let turns = Self::turn_time(graph, network, &pieces, mode);
        let distance_m = pieces.iter().map(|p| p.distance_m).sum();
        let duration_s = pieces.iter().map(|p| p.duration_s).sum::<f64>() + turns;
        assembly.traffic_s += pieces.iter().map(|p| p.traffic_s).sum::<f64>() + turns;
        let skip = usize::from(!assembly.geometry.is_empty());
        assembly.geometry.extend(points.into_iter().skip(skip));
        assembly.elevations.extend(heights.into_iter().skip(skip));
        Leg { distance_m, duration_s, steps }
    }

    /// The lanes of the road arriving at a maneuver, those leading where the route goes marked.
    fn lanes(graph: &Graph, arriving: u32, angle: f64, kind: ManeuverKind) -> Vec<Lane> {
        if matches!(kind, ManeuverKind::Roundabout | ManeuverKind::Ferry | ManeuverKind::Arrive | ManeuverKind::Depart) {
            return Vec::new();
        }
        let e = &graph.edges[arriving as usize];
        let way = graph.way_of(arriving);
        let text = graph.string(if e.is_reversed() { way.turn_lanes_backward } else { way.turn_lanes });
        let Some(mut lanes) = text.and_then(Lane::parse_osm) else { return Vec::new() };
        if lanes.len() > 32 || !Lane::mark(&mut lanes, angle) {
            return Vec::new();
        }
        lanes
    }

    /// The destinations of the signs of the road taken (its first pieces, through its ramps).
    fn destinations(graph: &Graph, pieces: &[Piece]) -> Option<String> {
        for piece in pieces.iter().take(4) {
            let e = &graph.edges[piece.edge as usize];
            let way = graph.way_of(piece.edge);
            let text = graph.string(if e.is_reversed() { way.destination_backward } else { way.destination });
            if let Some(text) = text {
                let parts: Vec<&str> = text.split(';').map(str::trim).filter(|p| !p.is_empty()).collect();
                if !parts.is_empty() {
                    return Some(parts.join(", "));
                }
            }
            if !way.highway.is_link() {
                break;
            }
        }
        None
    }

    /// The time lost turning and waiting along the path: the delays the search counted, without
    /// what it added to steer the choice.
    fn turn_time(graph: &Graph, network: &Network, pieces: &[Piece], mode: TravelMode) -> f64 {
        pieces
            .windows(2)
            .map(|w| {
                let angle = Coordinate::bearing_difference(network.arriving(w[0].edge), network.leaving(w[1].edge)).abs();
                Search::node_delay(graph, graph.edges[w[0].edge as usize].target, mode) as f64
                    + match mode {
                        TravelMode::Car if angle >= 100.0 => 12.0,
                        TravelMode::Car if angle >= 30.0 => 6.0,
                        TravelMode::Bicycle if angle >= 45.0 => 3.0,
                        _ => 0.0,
                    }
            })
            .sum()
    }

    fn roundabout(graph: &Graph, edge: u32) -> bool {
        graph.ways[graph.edges[edge as usize].way as usize].is_roundabout()
    }

    fn highway(graph: &Graph, edge: u32) -> Highway {
        graph.way_of(edge).highway
    }

    /// The angle of the turn from `from` into `to`, positive to the right.
    fn angle(graph: &Graph, from: u32, to: u32) -> f64 {
        let a = graph.edge_points(from);
        let b = graph.edge_points(to);
        // Over the last and first 30 m, so that a kink at the junction itself does not count.
        let before = Line::new(a.clone());
        let after = Line::new(b);
        let end = before.length_m();
        let p0 = before.point_at((end - 30.0).max(0.0)).unwrap_or(a[0]);
        let p1 = a[a.len() - 1];
        let p2 = after.point_at(30.0_f64.min(after.length_m())).unwrap_or(p1);
        if p0.distance_m(&p1) < 0.5 || p1.distance_m(&p2) < 0.5 {
            return 0.0;
        }
        Coordinate::bearing_difference(p0.bearing_to(&p1), p1.bearing_to(&p2))
    }

    /// The name and the number of the road of `edge`.
    fn road(graph: &Graph, edge: u32) -> (Option<&str>, Option<&str>) {
        let way = &graph.ways[graph.edges[edge as usize].way as usize];
        (graph.string(way.name), graph.string(way.road_ref))
    }

    /// Whether `b` is another road than `a`, and one with a name or number to tell: the same name
    /// or the same number is the same road.
    fn renamed(a: (Option<&str>, Option<&str>), b: (Option<&str>, Option<&str>)) -> bool {
        let same = (a.0.is_some() && a.0 == b.0) || (a.1.is_some() && a.1 == b.1);
        !same && (b.0.is_some() || b.1.is_some())
    }

    fn maneuvers(graph: &Graph, pieces: &[Piece], mode: TravelMode) -> Vec<Found> {
        let mut found = vec![Found { piece: 0, kind: ManeuverKind::Depart, modifier: None, exit: None, angle: 0.0 }];
        let mut i = 1;
        while i < pieces.len() {
            let (e, f) = (pieces[i - 1].edge, pieces[i].edge);
            let via = graph.edges[e as usize].target;
            let angle = Self::angle(graph, e, f);
            let (he, hf) = (Self::highway(graph, e), Self::highway(graph, f));
            // The other ways the traveller could take here.
            let twin = graph.twin(e);
            let others: Vec<f64> =
                graph.out_edges(via).filter(|&g| g != f && Some(g) != twin && graph.edges[g as usize].allows(mode)).map(|g| Self::angle(graph, e, g)).collect();

            if Self::roundabout(graph, f) && !Self::roundabout(graph, e) {
                // Count the exits passed until the roundabout is left.
                let mut exit = 0u32;
                let mut j = i;
                while j < pieces.len() && Self::roundabout(graph, pieces[j].edge) {
                    let node = graph.edges[pieces[j].edge as usize].target;
                    let next = pieces.get(j + 1).map(|p| p.edge);
                    let leaves = graph
                        .out_edges(node)
                        .any(|g| !Self::roundabout(graph, g) && graph.edges[g as usize].allows(mode) && Some(g) != graph.twin(pieces[j].edge));
                    if leaves || next.is_some_and(|n| !Self::roundabout(graph, n)) {
                        exit += 1;
                    }
                    j += 1;
                }
                found.push(Found { piece: i, kind: ManeuverKind::Roundabout, modifier: None, exit: Some(exit.max(1)), angle });
                // Leaving the roundabout is part of this maneuver: the next junction is after it.
                i = (j + 1).max(i + 1);
                continue;
            }

            let kind_modifier: Option<(ManeuverKind, Option<Modifier>)> = if hf == Highway::Ferry && he != Highway::Ferry {
                Some((ManeuverKind::Ferry, None))
            } else if hf.is_link() && !he.is_link() && matches!(he, Highway::Motorway | Highway::Trunk) {
                Some((ManeuverKind::OffRamp, Some(Self::side(angle))))
            } else if hf.is_link() && !he.is_link() {
                Some((ManeuverKind::OnRamp, Some(Modifier::from_angle(angle))))
            } else if he.is_link() && !hf.is_link() && matches!(hf, Highway::Motorway | Highway::Trunk) {
                Some((ManeuverKind::Merge, Some(Self::side(angle))))
            } else if others.is_empty() {
                // No choice: a bend of the road is not told; a change of road is, as a turn when
                // the road turns.
                let renamed = Self::renamed(Self::road(graph, e), Self::road(graph, f));
                match (renamed, angle.abs() >= 45.0) {
                    (true, true) => Some((ManeuverKind::Turn, Some(Modifier::from_angle(angle)))),
                    (true, false) => Some((ManeuverKind::NewName, None)),
                    _ => None,
                }
            } else if angle.abs() < Self::FORK_DEGREES && others.iter().any(|o| o.abs() < Self::FORK_DEGREES && (o - angle).abs() > 5.0) && angle.abs() >= 5.0 {
                // Two ways ahead: keep to the side of ours.
                let other = others
                    .iter()
                    .copied()
                    .filter(|o| o.abs() < Self::FORK_DEGREES)
                    .fold(f64::NAN, |a, b| if a.is_nan() || (b - angle).abs() < (a - angle).abs() { b } else { a });
                Some((ManeuverKind::Fork, Some(if angle < other { Modifier::SlightLeft } else { Modifier::SlightRight })))
            } else if angle.abs() < Self::STRAIGHT_DEGREES {
                Self::renamed(Self::road(graph, e), Self::road(graph, f)).then_some((ManeuverKind::Continue, Some(Modifier::Straight)))
            } else {
                let straight_possible = others.iter().any(|o| o.abs() < Self::STRAIGHT_DEGREES);
                let kind = if !straight_possible && angle.abs() > 60.0 && others.iter().all(|o| o.signum() != angle.signum()) && others.len() == 1 {
                    ManeuverKind::EndOfRoad
                } else {
                    ManeuverKind::Turn
                };
                Some((kind, Some(Modifier::from_angle(angle))))
            };
            if let Some((kind, modifier)) = kind_modifier {
                found.push(Found { piece: i, kind, modifier, exit: None, angle });
            }
            i += 1;
        }
        found
    }

    fn side(angle: f64) -> Modifier {
        if angle < 0.0 { Modifier::SlightLeft } else { Modifier::SlightRight }
    }

    /// The roads that carry most of `edges`, for telling routes apart.
    pub fn summary(graph: &Graph, edges: &[u32]) -> String {
        let mut lengths: HashMap<&str, f64> = HashMap::new();
        for &edge in edges {
            let (name, road_ref) = Self::road(graph, edge);
            if let Some(n) = road_ref.or(name) {
                *lengths.entry(n).or_default() += graph.edges[edge as usize].length_m();
            }
        }
        let mut roads: Vec<(&str, f64)> = lengths.into_iter().collect();
        roads.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        roads.iter().take(2).map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
    }
}
