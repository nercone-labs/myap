use crate::{Bounds, Coordinate};
use serde::{Deserialize, Serialize};

/// A tile of the Web Mercator projection (EPSG:3857), numbered as map servers number them:
/// `x` from the west and `y` from the north, `2^zoom` of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tile {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}

impl Tile {
    /// The latitude beyond which Web Mercator draws nothing.
    pub const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

    /// The tile holding `point` at `zoom` (at most 30).
    pub fn at(point: &Coordinate, zoom: u8) -> Tile {
        let zoom = zoom.min(30);
        let n = (1u64 << zoom) as f64;
        let latitude = point.latitude.clamp(-Self::MAX_LATITUDE, Self::MAX_LATITUDE).to_radians();
        let x = ((point.longitude + 180.0) / 360.0 * n).floor();
        let y = ((1.0 - (latitude.tan() + 1.0 / latitude.cos()).ln() / std::f64::consts::PI) / 2.0 * n).floor();
        let max = n - 1.0;
        Tile { zoom, x: x.clamp(0.0, max) as u32, y: y.clamp(0.0, max) as u32 }
    }

    /// The north-west corner of the tile.
    pub fn north_west(&self) -> Coordinate {
        let n = (1u64 << self.zoom) as f64;
        let longitude = self.x as f64 / n * 360.0 - 180.0;
        let latitude = (std::f64::consts::PI * (1.0 - 2.0 * self.y as f64 / n)).sinh().atan().to_degrees();
        Coordinate::new(latitude, longitude)
    }

    pub fn bounds(&self) -> Bounds {
        let nw = self.north_west();
        let se = Tile { zoom: self.zoom, x: self.x + 1, y: self.y + 1 }.north_west();
        Bounds { south: se.latitude, west: nw.longitude, north: nw.latitude, east: se.longitude }
    }

    /// The quadkey of the tile (Bing's numbering), one digit per zoom level.
    pub fn quadkey(&self) -> String {
        (1..=self.zoom)
            .rev()
            .map(|level| {
                let mask = 1u32 << (level - 1);
                let digit = u8::from(self.x & mask != 0) + 2 * u8::from(self.y & mask != 0);
                (b'0' + digit) as char
            })
            .collect()
    }
}
