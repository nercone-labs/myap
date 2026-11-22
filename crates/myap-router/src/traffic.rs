use crate::{Graph, SpatialIndex};
use geo::{Bounds, Coordinate, Line};

/// The speed of now on a line of road, from a source of live traffic: `factor` of its usual
/// speed (1 as usual, 0.5 half as fast), or closed.
#[derive(Debug, Clone, PartialEq)]
pub struct TrafficSpeed {
    pub shape: Vec<Coordinate>,
    pub factor: Option<f64>,
    pub closed: bool,
}

/// The traffic of now on the edges of a graph: the factor of each edge's usual speed, matched
/// from lines of traffic data by place and direction.
///
/// The factors are held one per edge (`UNKNOWN` where there is none), so that the search reads
/// one with an index: 4 bytes an edge while traffic is in use, nothing otherwise.
#[derive(Debug, Clone, Default)]
pub struct TrafficMap {
    /// The factor of each edge (0 closed, `UNKNOWN` unknown).
    factors: Vec<f32>,
    /// How many edges have a factor, and how many of them are closed.
    known: usize,
    closed: usize,
}

impl PartialEq for TrafficMap {
    fn eq(&self, other: &Self) -> bool {
        let n = self.factors.len().max(other.factors.len()) as u32;
        self.known == other.known && (0..n).all(|e| self.factor(e) == other.factor(e))
    }
}

impl TrafficMap {
    /// How far a line of traffic may lie from an edge to be on it, in metres.
    const MATCH_M: f64 = 15.0;
    /// How far its direction may differ from the edge's, in degrees.
    const MATCH_DEGREES: f64 = 35.0;
    /// The points of a line of traffic are tried this far apart, in metres.
    const SAMPLE_M: f64 = 40.0;
    /// The factor of an edge without traffic.
    const UNKNOWN: f32 = -1.0;

    pub fn is_empty(&self) -> bool {
        self.known == 0
    }

    /// How many edges have a factor.
    pub fn len(&self) -> usize {
        self.known
    }

    /// Whether some edge is closed.
    pub fn has_closures(&self) -> bool {
        self.closed > 0
    }

    /// The factor of `edge`'s usual speed now, when known (0 when closed).
    pub fn factor(&self, edge: u32) -> Option<f32> {
        self.factors.get(edge as usize).copied().filter(|f| *f >= 0.0)
    }

    pub fn is_closed(&self, edge: u32) -> bool {
        self.factors.get(edge as usize).is_some_and(|f| *f == 0.0)
    }

    /// Sets the factor of `edge` (closed below 0.01); for traffic built otherwise than by
    /// matching, and for tests.
    pub fn set(&mut self, edge: u32, factor: f64) {
        let f = if factor.is_nan() || factor < 0.01 { 0.0 } else { factor.clamp(0.05, 1.5) as f32 };
        self.put(edge, f);
    }

    /// Sets the factor of `edge` to `factor` (0 to 1.5) unconditionally.
    fn put(&mut self, edge: u32, factor: f32) {
        let i = edge as usize;
        if i >= self.factors.len() {
            self.factors.resize(i + 1, Self::UNKNOWN);
        }
        let old = self.factors[i];
        self.known += usize::from(old < 0.0);
        self.closed -= usize::from(old == 0.0);
        self.closed += usize::from(factor == 0.0);
        self.factors[i] = factor;
    }

    /// Matches `speeds` to the edges of `graph`: each line is sampled along its length, and each
    /// sample is matched to the nearest edge whose direction agrees, within `MATCH_M`. When several
    /// lines fall on one edge, the slowest wins (a closure always).
    pub fn matching(graph: &Graph, index: &SpatialIndex, speeds: &[TrafficSpeed]) -> TrafficMap {
        let mut map = TrafficMap { factors: Vec::with_capacity(graph.edges.len()), known: 0, closed: 0 };
        for speed in speeds {
            let factor = if speed.closed {
                0.0
            } else {
                match speed.factor {
                    Some(f) if f.is_finite() && f > 0.0 => f.clamp(0.05, 1.5),
                    _ => continue,
                }
            };
            if speed.shape.len() < 2 {
                continue;
            }
            let line = Line::new(speed.shape.clone());
            let length = line.length_m();
            if length <= 0.0 {
                continue;
            }
            let samples = ((length / Self::SAMPLE_M).ceil() as usize).clamp(1, 10_000);
            for k in 0..samples {
                let at = (k as f64 + 0.5) * length / samples as f64;
                let (Some(point), Some(bearing)) = (line.point_at(at), line.bearing_at(at)) else { continue };
                let reach = Bounds { south: point.latitude, west: point.longitude, north: point.latitude, east: point.longitude }.expanded_m(Self::MATCH_M);
                let mut best: Option<(f64, u32)> = None;
                for edge in index.near(&point, 1) {
                    if !graph.edge_meets(edge, &reach) {
                        continue;
                    }
                    let edge_line = Line::new(graph.edge_points(edge));
                    let Some(p) = edge_line.project(&point, 0.0, f64::INFINITY) else { continue };
                    if p.distance_m > Self::MATCH_M {
                        continue;
                    }
                    // The index holds one direction of each road; its twin, the other, runs on
                    // the same points the other way.
                    for (candidate, edge_bearing) in std::iter::once((edge, p.bearing)).chain(graph.twin(edge).map(|t| (t, p.bearing + 180.0))) {
                        if Coordinate::bearing_difference(edge_bearing, bearing).abs() > Self::MATCH_DEGREES {
                            continue;
                        }
                        if best.is_none_or(|(d, _)| p.distance_m < d) {
                            best = Some((p.distance_m, candidate));
                        }
                    }
                }
                if let Some((_, edge)) = best {
                    // The slowest line wins (a closure always).
                    let f = factor as f32;
                    if map.factor(edge).is_none_or(|old| f < old) {
                        map.put(edge, f);
                    }
                }
            }
        }
        map
    }
}
