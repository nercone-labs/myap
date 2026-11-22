use crate::{ElevationError, Encoding, MercatorPoint, Raster};
use geo::{Coordinate, Tile};
use png::{Image, Limits};

/// The heights of one terrain tile, in metres, row by row from the north-west pixel.
/// No-data pixels are NaN.
#[derive(Debug, Clone, PartialEq)]
pub struct DemTile {
    pub tile: Tile,
    pub width: u32,
    pub height: u32,
    pub heights: Vec<f32>,
}

impl DemTile {
    /// The highest zoom a tile can have, so that pixel indices stay exact.
    pub const MAX_ZOOM: u8 = 30;
    /// The largest tile image accepted: terrain tiles are 256 to 516 pixels wide.
    pub const LIMITS: Limits = Limits { max_width: 4096, max_height: 4096, max_bytes: 128 * 1024 * 1024 };

    /// A tile of `heights` (`width * height` of them, row by row), checked for consistency.
    pub fn new(tile: Tile, width: u32, height: u32, heights: Vec<f32>) -> Result<DemTile, ElevationError> {
        Self::check(&tile)?;
        if width == 0 || height == 0 || heights.len() as u64 != u64::from(width) * u64::from(height) {
            return Err(ElevationError::InvalidTile("the heights do not match the size"));
        }
        Ok(DemTile { tile, width, height, heights })
    }

    /// Checks that `tile` exists: a zoom of at most [`DemTile::MAX_ZOOM`] and `x`, `y` below
    /// `2^zoom`.
    pub fn check(tile: &Tile) -> Result<(), ElevationError> {
        if tile.zoom > Self::MAX_ZOOM {
            return Err(ElevationError::InvalidTile("zoom above 30"));
        }
        let n = 1u64 << tile.zoom;
        if u64::from(tile.x) >= n || u64::from(tile.y) >= n {
            return Err(ElevationError::InvalidTile("coordinates outside the zoom"));
        }
        Ok(())
    }

    /// Decodes the PNG image of `tile`, whose colours hold heights in `encoding`. A fully
    /// transparent pixel is no data, whatever its colour.
    pub fn decode(tile: Tile, encoding: Encoding, png_bytes: &[u8]) -> Result<DemTile, ElevationError> {
        Self::check(&tile)?;
        let image = Image::decode_with(png_bytes, &Self::LIMITS)?;
        let mut heights = Vec::with_capacity(image.width as usize * image.height as usize);
        for y in 0..image.height {
            for x in 0..image.width {
                let height = match image.rgba8(x, y) {
                    Some([r, g, b, a]) if a > 0 => encoding.height([r, g, b]).map_or(f32::NAN, |h| h as f32),
                    _ => f32::NAN,
                };
                heights.push(height);
            }
        }
        Self::new(tile, image.width, image.height, heights)
    }

    /// The height of pixel `(px, py)` of the tile, or `None` outside it or for no data.
    pub fn pixel(&self, px: u32, py: u32) -> Option<f64> {
        if px >= self.width || py >= self.height {
            return None;
        }
        let index = py as usize * self.width as usize + px as usize;
        self.heights.get(index).map(|&h| f64::from(h)).filter(|h| h.is_finite())
    }

    /// The global pixel coordinates of `point` in the pixels of this tile's zoom and size, or
    /// `None` outside the projection.
    pub fn to_pixel(&self, point: &Coordinate) -> Option<(f64, f64)> {
        Some(MercatorPoint::from_coordinate(point)?.to_pixel(self.tile.zoom, self.width, self.height))
    }

    /// Whether the global pixel coordinates `(x, y)` fall in this tile (west and north edges
    /// included, east and south edges excluded).
    pub fn contains_pixel(&self, x: f64, y: f64) -> bool {
        let (x0, y0) = (f64::from(self.tile.x) * f64::from(self.width), f64::from(self.tile.y) * f64::from(self.height));
        (x0..x0 + f64::from(self.width)).contains(&x) && (y0..y0 + f64::from(self.height)).contains(&y)
    }

    /// The height at `point`, interpolated bilinearly between pixel centres (see
    /// [`Raster::interpolate`]); near the edges, pixels outside this tile count as no data.
    /// `None` if the point is outside the tile or there is no data around it.
    pub fn height_at(&self, point: &Coordinate) -> Option<f64> {
        let (x, y) = self.to_pixel(point)?;
        if !self.contains_pixel(x, y) {
            return None;
        }
        self.interpolate(x, y)
    }
}

impl Raster for DemTile {
    /// The pixel at global pixel indices `(x, y)` of this tile's zoom, if it is in the tile.
    fn value(&self, x: i64, y: i64) -> Option<f64> {
        let px = x.checked_sub(i64::from(self.tile.x).checked_mul(i64::from(self.width))?)?;
        let py = y.checked_sub(i64::from(self.tile.y).checked_mul(i64::from(self.height))?)?;
        self.pixel(u32::try_from(px).ok()?, u32::try_from(py).ok()?)
    }
}
