use crate::{Bounds, Coordinate};

/// A line of points measured along its length: a road or a route.
///
/// The distance along the line to each point is computed once, so that the closest point to a
/// location and the point at a distance cost a search, not a measurement of the whole line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    points: Vec<Coordinate>,
    /// `along[i]`: the distance in metres from the first point to point `i`.
    along: Vec<f64>,
}

/// Where a location falls on a [`Line`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// The closest point of the line.
    pub point: Coordinate,
    /// The index of the segment (from point `segment` to point `segment + 1`).
    pub segment: usize,
    /// The distance in metres along the line to `point`.
    pub along_m: f64,
    /// The distance in metres from the location to `point`.
    pub distance_m: f64,
    /// The bearing of the segment, in degrees.
    pub bearing: f64,
}

impl Line {
    pub fn new(points: Vec<Coordinate>) -> Line {
        let mut along = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, point) in points.iter().enumerate() {
            if i > 0 {
                total += points[i - 1].distance_m(point);
            }
            along.push(total);
        }
        Line { points, along }
    }

    pub fn points(&self) -> &[Coordinate] {
        &self.points
    }

    /// The distance in metres from the first point to point `index`.
    pub fn along_m(&self, index: usize) -> f64 {
        self.along.get(index).copied().unwrap_or_else(|| self.length_m())
    }

    pub fn length_m(&self) -> f64 {
        self.along.last().copied().unwrap_or(0.0)
    }

    pub fn bounds(&self) -> Option<Bounds> {
        Bounds::of(&self.points)
    }

    /// The closest point of the line to `location`, among the segments from `from_m` to `to_m`
    /// metres along it (the whole line for `0.0..f64::INFINITY`). `None` for a line without
    /// points.
    ///
    /// Each segment is short enough to be treated as flat, in a local equirectangular projection
    /// around the location.
    pub fn project(&self, location: &Coordinate, from_m: f64, to_m: f64) -> Option<Projection> {
        match self.points.len() {
            0 => return None,
            1 => {
                let point = self.points[0];
                return Some(Projection { point, segment: 0, along_m: 0.0, distance_m: location.distance_m(&point), bearing: 0.0 });
            }
            _ => {}
        }
        let scale = location.latitude.to_radians().cos();
        let flat = |c: &Coordinate| ((c.longitude - location.longitude) * scale, c.latitude - location.latitude);
        let first = self.segment_at(from_m);
        let last = self.segment_at(to_m);
        // The segments are compared by their squared distance in the flat projection, and the
        // distance on the sphere is measured once, for the closest: a measurement per segment
        // costs several trigonometric functions.
        let mut best: Option<(f64, usize, f64)> = None;
        for segment in first..=last {
            let (ax, ay) = flat(&self.points[segment]);
            let (bx, by) = flat(&self.points[segment + 1]);
            let (dx, dy) = (bx - ax, by - ay);
            let length2 = dx * dx + dy * dy;
            let t = if length2 > 0.0 { (-(ax * dx + ay * dy) / length2).clamp(0.0, 1.0) } else { 0.0 };
            let (px, py) = (ax + dx * t, ay + dy * t);
            let distance2 = px * px + py * py;
            if best.is_none_or(|(d, _, _)| distance2 < d) {
                best = Some((distance2, segment, t));
            }
        }
        let (_, segment, t) = best?;
        let (a, b) = (self.points[segment], self.points[segment + 1]);
        let point = a.lerp(&b, t);
        let along_m = self.along[segment] + (self.along[segment + 1] - self.along[segment]) * t;
        Some(Projection { point, segment, along_m, distance_m: location.distance_m(&point), bearing: a.bearing_to(&b) })
    }

    /// The index of the segment holding the point `distance_m` along the line.
    pub fn segment_at(&self, distance_m: f64) -> usize {
        if self.points.len() < 2 {
            return 0;
        }
        let index = self.along.partition_point(|&a| a <= distance_m);
        index.saturating_sub(1).min(self.points.len() - 2)
    }

    /// The point `distance_m` along the line, clamped to its ends.
    pub fn point_at(&self, distance_m: f64) -> Option<Coordinate> {
        match self.points.len() {
            0 => None,
            1 => Some(self.points[0]),
            _ => {
                let segment = self.segment_at(distance_m);
                let (start, end) = (self.along[segment], self.along[segment + 1]);
                let t = if end > start { ((distance_m - start) / (end - start)).clamp(0.0, 1.0) } else { 0.0 };
                Some(self.points[segment].lerp(&self.points[segment + 1], t))
            }
        }
    }

    /// The bearing of the line at `distance_m` along it.
    pub fn bearing_at(&self, distance_m: f64) -> Option<f64> {
        if self.points.len() < 2 {
            return None;
        }
        let segment = self.segment_at(distance_m);
        Some(self.points[segment].bearing_to(&self.points[segment + 1]))
    }

    /// The part of the line from `from_m` to `to_m` metres along it.
    pub fn slice(&self, from_m: f64, to_m: f64) -> Vec<Coordinate> {
        let (Some(start), Some(end)) = (self.point_at(from_m), self.point_at(to_m)) else { return Vec::new() };
        let mut points = vec![start];
        let first = self.segment_at(from_m) + 1;
        let last = self.segment_at(to_m);
        if first <= last {
            points.extend_from_slice(&self.points[first..=last]);
        }
        points.push(end);
        points
    }
}
