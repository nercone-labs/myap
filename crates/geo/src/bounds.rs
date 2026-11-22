use crate::Coordinate;
use serde::{Deserialize, Serialize};

/// A rectangle of latitudes and longitudes. It does not cross the antimeridian: `west <= east`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl Bounds {
    /// The smallest bounds holding every point, or `None` without points.
    pub fn of<'a>(points: impl IntoIterator<Item = &'a Coordinate>) -> Option<Bounds> {
        let mut bounds: Option<Bounds> = None;
        for point in points {
            bounds = Some(match bounds {
                None => Bounds { south: point.latitude, west: point.longitude, north: point.latitude, east: point.longitude },
                Some(b) => b.including(point),
            });
        }
        bounds
    }

    /// These bounds grown to hold `point`.
    pub fn including(&self, point: &Coordinate) -> Bounds {
        Bounds {
            south: self.south.min(point.latitude),
            west: self.west.min(point.longitude),
            north: self.north.max(point.latitude),
            east: self.east.max(point.longitude),
        }
    }

    /// The bounds holding both.
    pub fn union(&self, other: &Bounds) -> Bounds {
        Bounds { south: self.south.min(other.south), west: self.west.min(other.west), north: self.north.max(other.north), east: self.east.max(other.east) }
    }

    pub fn contains(&self, point: &Coordinate) -> bool {
        (self.south..=self.north).contains(&point.latitude) && (self.west..=self.east).contains(&point.longitude)
    }

    pub fn intersects(&self, other: &Bounds) -> bool {
        self.south <= other.north && other.south <= self.north && self.west <= other.east && other.west <= self.east
    }

    pub fn center(&self) -> Coordinate {
        Coordinate::new((self.south + self.north) / 2.0, (self.west + self.east) / 2.0)
    }

    /// These bounds grown by `metres` on every side.
    pub fn expanded_m(&self, metres: f64) -> Bounds {
        let dlat = (metres / Coordinate::EARTH_RADIUS_M).to_degrees();
        let latitude = self.south.abs().max(self.north.abs()).min(89.0);
        let dlon = dlat / latitude.to_radians().cos();
        Bounds {
            south: (self.south - dlat).max(-90.0),
            west: (self.west - dlon).max(-180.0),
            north: (self.north + dlat).min(90.0),
            east: (self.east + dlon).min(180.0),
        }
    }

    /// The approximate area in square kilometres.
    pub fn area_km2(&self) -> f64 {
        let height = Coordinate::new(self.south, self.west).distance_m(&Coordinate::new(self.north, self.west)) / 1000.0;
        let middle = (self.south + self.north) / 2.0;
        let width = Coordinate::new(middle, self.west).distance_m(&Coordinate::new(middle, self.east)) / 1000.0;
        height * width
    }
}
