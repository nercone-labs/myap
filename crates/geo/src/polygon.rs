use crate::{Bounds, Coordinate};
use serde::{Deserialize, Serialize};

/// An area bounded by a ring of points, without holes: an area to avoid, a zone. The ring is
/// closed implicitly (the last point joins the first) and must not cross itself.
///
/// The edges of the ring are straight in degrees of latitude and longitude, which over the size
/// of a district is within a few metres of a straight line on the ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polygon {
    pub ring: Vec<Coordinate>,
}

impl Polygon {
    /// The most points a ring may hold: more is refused as data, not drawn.
    pub const MAX_POINTS: usize = 1_000;

    pub fn new(ring: Vec<Coordinate>) -> Polygon {
        Polygon { ring }
    }

    /// A regular polygon of `sides` points (at least 3) at `radius_m` around `center`: a circle,
    /// for the formats that take only polygons.
    pub fn circle(center: &Coordinate, radius_m: f64, sides: usize) -> Polygon {
        let sides = sides.max(3);
        let ring = (0..sides).map(|i| center.destination(360.0 * i as f64 / sides as f64, radius_m)).collect();
        Polygon { ring }
    }

    /// Whether the ring can be used: three to `MAX_POINTS` valid points, not crossing the
    /// antimeridian (no edge spans more than 180° of longitude).
    pub fn is_valid(&self) -> bool {
        let n = self.ring.len();
        (3..=Self::MAX_POINTS).contains(&n)
            && self.ring.iter().all(Coordinate::is_valid)
            && (0..n).all(|i| (self.ring[i].longitude - self.ring[(i + 1) % n].longitude).abs() <= 180.0)
    }

    pub fn bounds(&self) -> Option<Bounds> {
        Bounds::of(&self.ring)
    }

    /// Whether `point` is inside the ring (the even-odd rule); a point exactly on an edge may
    /// count either way.
    pub fn contains(&self, point: &Coordinate) -> bool {
        let n = self.ring.len();
        if n < 3 {
            return false;
        }
        let (x, y) = (point.longitude, point.latitude);
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = (self.ring[i].longitude, self.ring[i].latitude);
            let (xj, yj) = (self.ring[j].longitude, self.ring[j].latitude);
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Whether the segment from `a` to `b` touches the area: an end inside, or crossing an edge.
    pub fn touches_segment(&self, a: &Coordinate, b: &Coordinate) -> bool {
        if self.contains(a) || self.contains(b) {
            return true;
        }
        let n = self.ring.len();
        (0..n).any(|i| Self::segments_cross(a, b, &self.ring[i], &self.ring[(i + 1) % n]))
    }

    /// Whether the line through `points` touches the area.
    pub fn touches_line(&self, points: &[Coordinate]) -> bool {
        match points {
            [] => false,
            [p] => self.contains(p),
            _ => points.windows(2).any(|w| self.touches_segment(&w[0], &w[1])),
        }
    }

    /// Whether segments `a`–`b` and `c`–`d` intersect (touching counts), in degrees.
    fn segments_cross(a: &Coordinate, b: &Coordinate, c: &Coordinate, d: &Coordinate) -> bool {
        let orient = |p: &Coordinate, q: &Coordinate, r: &Coordinate| {
            let v = (q.longitude - p.longitude) * (r.latitude - p.latitude) - (q.latitude - p.latitude) * (r.longitude - p.longitude);
            if v > 0.0 {
                1
            } else if v < 0.0 {
                -1
            } else {
                0
            }
        };
        let on = |p: &Coordinate, q: &Coordinate, r: &Coordinate| {
            r.longitude >= p.longitude.min(q.longitude)
                && r.longitude <= p.longitude.max(q.longitude)
                && r.latitude >= p.latitude.min(q.latitude)
                && r.latitude <= p.latitude.max(q.latitude)
        };
        let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
        if o1 != o2 && o3 != o4 {
            return true;
        }
        (o1 == 0 && on(a, b, c)) || (o2 == 0 && on(a, b, d)) || (o3 == 0 && on(c, d, a)) || (o4 == 0 && on(c, d, b))
    }
}
