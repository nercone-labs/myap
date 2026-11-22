use geo::{Coordinate, Tile};
use std::f64::consts::PI;

/// A point of the Web Mercator projection (EPSG:3857) as fractions of the world: `x` from the
/// antimeridian eastwards and `y` from the north edge southwards, both from 0 to 1.
///
/// At zoom `z`, with tiles of `w` by `h` pixels, the world is `2^z * w` pixels wide and
/// `2^z * h` high, and pixel `(px, py)` of tile `(x, y)` covers the square from global pixel
/// coordinate `(x * w + px, y * h + py)` to one more in each direction: its centre is at
/// `+ 0.5`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MercatorPoint {
    pub x: f64,
    pub y: f64,
}

impl MercatorPoint {
    /// The projection of `point`, or `None` if it is invalid or beyond the latitudes Web
    /// Mercator draws ([`Tile::MAX_LATITUDE`]).
    pub fn from_coordinate(point: &Coordinate) -> Option<MercatorPoint> {
        if !point.is_valid() || point.latitude.abs() > Tile::MAX_LATITUDE {
            return None;
        }
        let latitude = point.latitude.to_radians();
        Some(MercatorPoint { x: (point.longitude + 180.0) / 360.0, y: (1.0 - latitude.tan().asinh() / PI) / 2.0 })
    }

    /// The coordinate of this point.
    pub fn to_coordinate(&self) -> Coordinate {
        let latitude = (PI * (1.0 - 2.0 * self.y)).sinh().atan().to_degrees();
        Coordinate::new(latitude, self.x * 360.0 - 180.0)
    }

    /// The global pixel coordinates of this point at `zoom` for tiles of `tile_width` by
    /// `tile_height` pixels.
    pub fn to_pixel(&self, zoom: u8, tile_width: u32, tile_height: u32) -> (f64, f64) {
        let n = 2f64.powi(i32::from(zoom));
        (self.x * n * f64::from(tile_width), self.y * n * f64::from(tile_height))
    }

    /// The point at global pixel coordinates `(x, y)`, the inverse of [`MercatorPoint::to_pixel`].
    pub fn from_pixel(x: f64, y: f64, zoom: u8, tile_width: u32, tile_height: u32) -> MercatorPoint {
        let n = 2f64.powi(i32::from(zoom));
        MercatorPoint { x: x / (n * f64::from(tile_width)), y: y / (n * f64::from(tile_height)) }
    }
}
