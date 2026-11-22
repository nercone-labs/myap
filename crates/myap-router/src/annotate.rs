use crate::{Highway, NodeFlag, NodeNameKind, Router};
use geo::{Coordinate, Line};
use routing::{Alert, AlertKind, ManeuverKind, Route, Span, TravelMode};
use std::collections::HashMap;

/// What a route made by another router gained from a graph ([`Router::annotate`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Annotation {
    /// The fraction of the route's length that lies on roads of the graph.
    pub matched_fraction: f64,
    /// Whether the speed limits were added.
    pub speed_limits: bool,
    /// How many alerts were added (level crossings, speed cameras, toll booths).
    pub alerts: usize,
    /// How many steps were given the name or number of their junction.
    pub junctions: usize,
}

impl Router {
    /// How far a point of a line may lie from a road to be on it, in metres.
    pub const MATCH_M: f64 = 15.0;
    /// How far the direction of the line may differ from the road's, in degrees.
    pub const MATCH_DEGREES: f64 = 40.0;
    /// A road followed (or one leaving its end) this close to the next point is taken without
    /// looking at the others, in metres.
    pub const CLOSE_M: f64 = 5.0;
    /// The points of a line are matched this far apart, in metres.
    pub const MATCH_SAMPLE_M: f64 = 30.0;
    /// The least part of a route on the graph for it to be annotated.
    pub const MIN_MATCHED_FRACTION: f64 = 0.6;
    /// How far a maneuver may be from a named junction to take its name, in metres.
    pub const JUNCTION_M: f64 = 25.0;

    /// The edge of the graph each segment of `points` runs along, for `mode` (`None` where it is
    /// on none): each segment is sampled every [`MATCH_SAMPLE_M`](Self::MATCH_SAMPLE_M), and
    /// each sample goes to the nearest edge within [`MATCH_M`](Self::MATCH_M) whose direction is
    /// within [`MATCH_DEGREES`](Self::MATCH_DEGREES), the edge of the sample before winning a
    /// near tie. Returns, per segment, the edges of its samples in order.
    pub fn match_line(&self, points: &[Coordinate], mode: TravelMode) -> Vec<Vec<u32>> {
        let mut lines: HashMap<u32, Line> = HashMap::new();
        let mut previous: Option<u32> = None;
        let mut out = Vec::with_capacity(points.len().saturating_sub(1));
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = a.distance_m(&b);
            let mut edges: Vec<u32> = Vec::new();
            if length.is_finite() && length >= 0.5 {
                let bearing = a.bearing_to(&b);
                let samples = ((length / Self::MATCH_SAMPLE_M).ceil() as usize).clamp(1, 1_000);
                for k in 0..samples {
                    let point = a.lerp(&b, (k as f64 + 0.5) / samples as f64);
                    let mut best: Option<(f64, u32)> = None;
                    let mut consider = |candidate: u32, best: &mut Option<(f64, u32)>| {
                        if !self.graph.edges[candidate as usize].allows(mode) {
                            return;
                        }
                        let line = lines.entry(candidate).or_insert_with(|| Line::new(self.graph.edge_points(candidate)));
                        let Some(p) = line.project(&point, 0.0, f64::INFINITY) else { return };
                        if p.distance_m > Self::MATCH_M || Coordinate::bearing_difference(p.bearing, bearing).abs() > Self::MATCH_DEGREES {
                            return;
                        }
                        // Staying on the same road wins over a parallel one a little nearer.
                        let score = p.distance_m - if previous == Some(candidate) { 3.0 } else { 0.0 };
                        if best.is_none_or(|(s, _)| score < s) {
                            *best = Some((score, candidate));
                        }
                    };
                    // First the road followed and those leaving its end: a route mostly goes on along
                    // them, and one of them close by is taken without searching the cells round
                    // the point (hundreds of roads in a city).
                    if let Some(e) = previous {
                        consider(e, &mut best);
                        for next in self.graph.out_edges(self.graph.edges[e as usize].target) {
                            consider(next, &mut best);
                        }
                    }
                    if best.is_none_or(|(s, _)| s > Self::CLOSE_M) {
                        for edge in self.index.near(&point, 1) {
                            // The index holds one direction of each road; its twin is the other.
                            for candidate in std::iter::once(edge).chain(self.graph.twin(edge)) {
                                consider(candidate, &mut best);
                            }
                        }
                    }
                    if let Some((_, edge)) = best {
                        if edges.last() != Some(&edge) {
                            edges.push(edge);
                        }
                        previous = Some(edge);
                    }
                }
            }
            out.push(edges);
        }
        out
    }

    /// Adds to `route`, made by another router, what the graph knows along it and the route
    /// lacks: the speed limits, the level crossings, speed cameras and toll booths, and the names
    /// and numbers of the junctions of its steps. What the route has is never replaced. Nothing is
    /// added unless [`MIN_MATCHED_FRACTION`](Self::MIN_MATCHED_FRACTION) of it lies on the graph,
    /// so that a route through roads the graph does not have is not given another road's limits.
    pub fn annotate(&self, route: &mut Route, mode: TravelMode) -> Annotation {
        let points = &route.geometry;
        if points.len() < 2 || mode == TravelMode::Transit || !self.covers(&[points[0], points[points.len() - 1]]) {
            return Annotation::default();
        }
        let matched = self.match_line(points, mode);
        let (mut on, mut total) = (0.0, 0.0);
        for (i, edges) in matched.iter().enumerate() {
            let length = points[i].distance_m(&points[i + 1]);
            total += length;
            if !edges.is_empty() {
                on += length;
            }
        }
        let mut annotation = Annotation { matched_fraction: if total > 0.0 { on / total } else { 0.0 }, ..Annotation::default() };
        if annotation.matched_fraction < Self::MIN_MATCHED_FRACTION {
            return annotation;
        }

        if route.speed_limits.is_empty() {
            let mut spans = Vec::new();
            for (i, edges) in matched.iter().enumerate() {
                // The limit of the road most of the segment is on: that of its middle sample.
                let Some(&edge) = edges.get(edges.len() / 2) else { continue };
                let way = self.graph.way_of(edge);
                let limit = way.maxspeed(self.graph.edges[edge as usize].is_reversed());
                if limit > 0 && way.highway != Highway::Ferry {
                    Span::push(&mut spans, i, i + 1, limit as u16);
                }
            }
            annotation.speed_limits = !spans.is_empty();
            route.speed_limits = spans;
        }

        if route.alerts.is_empty() {
            let line = Line::new(points.clone());
            let mut seen: Vec<u32> = Vec::new();
            let mut alerts = Vec::new();
            let sequence: Vec<(usize, u32)> = matched.iter().enumerate().flat_map(|(i, edges)| edges.iter().map(move |&e| (i, e))).collect();
            for pair in sequence.windows(2) {
                let ((i, from), (j, to)) = (pair[0], pair[1]);
                if from == to {
                    continue;
                }
                // The node between two roads followed one after the other.
                let via = self.graph.edges[from as usize].target;
                if via != self.graph.source(to) || seen.contains(&via) {
                    continue;
                }
                seen.push(via);
                for (flag, kind) in [
                    (NodeFlag::LEVEL_CROSSING, AlertKind::LevelCrossing),
                    (NodeFlag::SPEED_CAMERA, AlertKind::SpeedCamera),
                    (NodeFlag::TOLL_BOOTH, AlertKind::TollBooth),
                ] {
                    if !self.graph.node_has(via, flag) {
                        continue;
                    }
                    let location = self.graph.node_coordinate(via);
                    // The point of the route at or before it, so that it is never told late.
                    let index = line.project(&location, line.along_m(i), line.along_m(j + 1)).map_or(i, |p| p.segment);
                    alerts.push(Alert { kind, index, location, text: None });
                }
            }
            alerts.sort_by_key(|a| a.index);
            annotation.alerts = alerts.len();
            route.alerts = alerts;
        }

        // The junctions of the route, for the steps that have no name of theirs.
        let mut junctions: Vec<(Coordinate, u32)> = Vec::new();
        for pair in matched.iter().flatten().collect::<Vec<_>>().windows(2) {
            let via = self.graph.edges[*pair[0] as usize].target;
            if via == self.graph.source(*pair[1]) && !junctions.iter().any(|(_, n)| *n == via) {
                junctions.push((self.graph.node_coordinate(via), via));
            }
        }
        for step in route.legs.iter_mut().flat_map(|l| l.steps.iter_mut()) {
            if step.junction.is_some() || matches!(step.maneuver.kind, ManeuverKind::Depart | ManeuverKind::Arrive) {
                continue;
            }
            let near = junctions
                .iter()
                .map(|(c, n)| (c.distance_m(&step.maneuver.location), *n))
                .filter(|(d, n)| *d <= Self::JUNCTION_M && self.graph.node_name(*n, NodeNameKind::Name).is_some())
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, node)) = near {
                step.junction = self.graph.node_name(node, NodeNameKind::Name).map(str::to_string);
                if step.exit_number.is_none() && self.graph.node_has(node, NodeFlag::MOTORWAY_JUNCTION) {
                    step.exit_number = self.graph.node_name(node, NodeNameKind::Ref).map(str::to_string);
                }
                annotation.junctions += 1;
            }
        }
        annotation
    }
}
