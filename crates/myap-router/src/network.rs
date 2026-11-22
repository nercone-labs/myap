use crate::{Edge, Graph, Highway, Profile};
use geo::Coordinate;
use routing::TravelMode;

/// The climb along a directed edge, from its elevations (or the incline of its way).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Climb {
    /// The metres climbed and descended along the edge, in decimetres.
    pub up_dm: u16,
    pub down_dm: u16,
    /// The steepest climb and descent over `Network::GRADE_SPAN_M` or more, in percent (both
    /// positive).
    pub max_up_percent: u8,
    pub max_down_percent: u8,
}

impl Climb {
    pub fn up_m(&self) -> f64 {
        self.up_dm as f64 / 10.0
    }

    pub fn down_m(&self) -> f64 {
        self.down_dm as f64 / 10.0
    }
}

/// The class and flags of the way of an edge (`WayInfo::highway`, `WayInfo::flags`), held with
/// the edge so that the search reads them next to its other values instead of from the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeClass {
    pub highway: Highway,
    pub flags: u16,
}

impl EdgeClass {
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }
}

/// What the search derives from a graph once, to spend no time on it per edge searched: the
/// bearings at the ends of each edge, its climb, the class of its way, the place of each node as a
/// point in space, and the time to travel each edge by each mode (computed the first time a mode
/// is routed, since most travellers use one).
pub struct Network {
    /// The bearing leaving the source and arriving at the target of each edge, in tenths of a
    /// degree.
    bearings: Vec<[u16; 2]>,
    classes: Vec<EdgeClass>,
    /// Each node as a point of the unit sphere: the straight line between two is a little
    /// shorter than the way round the Earth, so the estimate of the search needs no trigonometry.
    points: Vec<[f32; 3]>,
    climbs: Vec<Climb>,
    /// The seconds to travel each edge by car, bicycle and walking (infinite where the mode may
    /// not), each empty until first needed.
    times: [Vec<f32>; 3],
    /// The fastest speed of any edge by each mode, in m/s: the estimate of the search must never
    /// be beaten by a ferry or a descent faster than the mode on the flat.
    max_speed: [f32; 3],
}

impl Network {
    /// The shortest stretch a gradient is measured over, in metres: shorter ones are the noise of
    /// elevation data.
    pub const GRADE_SPAN_M: f64 = 30.0;
    /// Changes of elevation smaller than this are not counted as climbs, in metres.
    const CLIMB_THRESHOLD_M: f64 = 1.0;

    pub fn new(graph: &Graph) -> Network {
        let bearings = (0..graph.edges.len() as u32)
            .map(|e| {
                let n = graph.edges[e as usize].shape_len as usize;
                let leave = graph.edge_point(e, 0).bearing_to(&graph.edge_point(e, 1));
                let arrive = graph.edge_point(e, n - 2).bearing_to(&graph.edge_point(e, n - 1));
                [Self::tenths(leave), Self::tenths(arrive)]
            })
            .collect();
        let climbs = (0..graph.edges.len() as u32).map(|e| Self::climb(graph, e)).collect();
        let classes = graph
            .edges
            .iter()
            .map(|e| {
                let way = &graph.ways[e.way as usize];
                EdgeClass { highway: way.highway, flags: way.flags }
            })
            .collect();
        let points = graph
            .nodes
            .iter()
            .map(|p| Self::point_of(&Graph::coordinate(*p)))
            .collect();
        Network { bearings, classes, points, climbs, times: [Vec::new(), Vec::new(), Vec::new()], max_speed: [0.0; 3] }
    }

    /// The point of the unit sphere of `coordinate`.
    pub fn point_of(coordinate: &Coordinate) -> [f32; 3] {
        let (lat, lon) = (coordinate.latitude.to_radians(), coordinate.longitude.to_radians());
        [(lat.cos() * lon.cos()) as f32, (lat.cos() * lon.sin()) as f32, lat.sin() as f32]
    }

    /// A distance in metres from `node` to `point` (`point_of`) never longer than the distance
    /// along the Earth: the chord between them, less the error of `f32` (a metre).
    pub fn chord_m(&self, node: u32, point: &[f32; 3]) -> f32 {
        let p = self.points[node as usize];
        let (dx, dy, dz) = (p[0] - point[0], p[1] - point[1], p[2] - point[2]);
        ((dx * dx + dy * dy + dz * dz).sqrt() * Coordinate::EARTH_RADIUS_M as f32 - 1.0).max(0.0)
    }

    pub fn class_of(&self, edge: u32) -> EdgeClass {
        self.classes[edge as usize]
    }

    /// The angle of the turn from `from` into `to` (positive to the right), in degrees, from the
    /// bearings in tenths: integer arithmetic, without the remainder of floating point.
    pub fn turn_angle(&self, from: u32, to: u32) -> f32 {
        let arrive = self.bearings[from as usize][1] as i32;
        let leave = self.bearings[to as usize][0] as i32;
        let mut d = (leave - arrive).rem_euclid(3600);
        if d > 1800 {
            d -= 3600;
        }
        d as f32 / 10.0
    }

    fn tenths(bearing: f64) -> u16 {
        (Coordinate::normalize_bearing(bearing) * 10.0).round() as u16 % 3600
    }

    fn slot(mode: TravelMode) -> usize {
        match mode {
            TravelMode::Car => 0,
            TravelMode::Bicycle => 1,
            _ => 2,
        }
    }

    /// The bearing leaving the source of `edge`, in degrees.
    pub fn leaving(&self, edge: u32) -> f64 {
        self.bearings[edge as usize][0] as f64 / 10.0
    }

    /// The bearing arriving at the target of `edge`, in degrees.
    pub fn arriving(&self, edge: u32) -> f64 {
        self.bearings[edge as usize][1] as f64 / 10.0
    }

    pub fn climb_of(&self, edge: u32) -> Climb {
        self.climbs[edge as usize]
    }

    /// Computes the times of `mode`, if not yet done.
    pub fn prepare(&mut self, graph: &Graph, mode: TravelMode) {
        let slot = Self::slot(mode);
        if self.times[slot].len() == graph.edges.len() && !graph.edges.is_empty() {
            return;
        }
        let profile = Profile::new(mode);
        let mut fastest = (profile.max_kmh() / 3.6) as f32;
        let times: Vec<f32> = graph
            .edges
            .iter()
            .enumerate()
            .map(|(i, edge)| {
                if !edge.allows(mode) {
                    return f32::INFINITY;
                }
                let way = &graph.ways[edge.way as usize];
                let flat = profile.speed_kmh(way.highway, way.maxspeed(edge.is_reversed()), way.flags);
                let climb = self.climbs[i];
                let time = profile.graded_time_s(edge.length_m(), flat, climb.up_m(), climb.down_m()).max(0.01) as f32;
                fastest = fastest.max(edge.length_m() as f32 / time);
                time
            })
            .collect();
        self.times[slot] = times;
        self.max_speed[slot] = fastest;
    }

    /// The seconds to travel the whole of `edge` by `mode` (infinite when it may not), after
    /// `prepare`.
    pub fn time(&self, edge: u32, mode: TravelMode) -> f32 {
        self.times[Self::slot(mode)].get(edge as usize).copied().unwrap_or(f32::INFINITY)
    }

    /// The fastest speed of `mode` on this graph, in m/s, after `prepare`.
    pub fn max_speed(&self, mode: TravelMode) -> f32 {
        self.max_speed[Self::slot(mode)].max(0.1)
    }

    /// The climb of `edge`, from the elevations of its points, or the incline of its way.
    fn climb(graph: &Graph, edge: u32) -> Climb {
        let e: &Edge = &graph.edges[edge as usize];
        let length = e.length_m();
        if let Some(elevations) = graph.edge_elevations(edge) {
            let points = graph.edge_points(edge);
            return Self::climb_of_profile(&points, &elevations);
        }
        let way = graph.way_of(edge);
        if way.incline_percent == 0 {
            return Climb::default();
        }
        let percent = if e.is_reversed() { -(way.incline_percent as f64) } else { way.incline_percent as f64 };
        let change = (length * percent.abs() / 100.0 * 10.0).round().min(u16::MAX as f64) as u16;
        let steep = percent.abs().round().min(100.0) as u8;
        if percent > 0.0 {
            Climb { up_dm: change, down_dm: 0, max_up_percent: steep, max_down_percent: 0 }
        } else {
            Climb { up_dm: 0, down_dm: change, max_up_percent: 0, max_down_percent: steep }
        }
    }

    /// The climb along points with these elevations (unknown ones skipped).
    pub fn climb_of_profile(points: &[Coordinate], elevations: &[Option<f32>]) -> Climb {
        let known: Vec<(f64, f64)> = {
            let mut along = 0.0;
            let mut out = Vec::with_capacity(points.len());
            for (i, p) in points.iter().enumerate() {
                if i > 0 {
                    along += points[i - 1].distance_m(p);
                }
                if let Some(Some(h)) = elevations.get(i) {
                    out.push((along, *h as f64));
                }
            }
            out
        };
        let Some(&(_, first)) = known.first() else { return Climb::default() };
        // Climb and descent with a threshold, so that the noise of the data does not add up.
        let (mut up, mut down, mut anchor) = (0.0, 0.0, first);
        for &(_, h) in &known[1..] {
            let d = h - anchor;
            if d >= Self::CLIMB_THRESHOLD_M {
                up += d;
                anchor = h;
            } else if d <= -Self::CLIMB_THRESHOLD_M {
                down -= d;
                anchor = h;
            }
        }
        // The steepest gradients over stretches of at least `GRADE_SPAN_M`.
        let (mut max_up, mut max_down) = (0.0f64, 0.0f64);
        let mut start = 0;
        for end in 1..known.len() {
            while start + 1 < end && known[end].0 - known[start + 1].0 >= Self::GRADE_SPAN_M {
                start += 1;
            }
            let span = known[end].0 - known[start].0;
            if span >= Self::GRADE_SPAN_M {
                let grade = (known[end].1 - known[start].1) / span * 100.0;
                max_up = max_up.max(grade);
                max_down = max_down.max(-grade);
            }
        }
        let dm = |m: f64| (m * 10.0).round().clamp(0.0, u16::MAX as f64) as u16;
        let percent = |p: f64| p.round().clamp(0.0, 100.0) as u8;
        Climb { up_dm: dm(up), down_dm: dm(down), max_up_percent: percent(max_up), max_down_percent: percent(max_down) }
    }
}
