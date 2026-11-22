use crate::{DemTile, ElevationError, Encoding, MercatorPoint, Raster, Source};
use geo::Coordinate;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A digital elevation model: tiles of one zoom and one size, keyed by `(x, y)`, sampled as one
/// grid so that interpolation crosses tile edges. Tiles may be missing; their area has no data.
#[derive(Debug, Clone, Default)]
pub struct Dem {
    zoom: u8,
    tile_size: Option<(u32, u32)>,
    tiles: HashMap<(u32, u32), DemTile>,
}

impl Dem {
    /// The largest tile file [`Dem::load_directory`] reads; larger files are skipped.
    pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

    /// An empty model of tiles at `zoom`.
    pub fn new(zoom: u8) -> Dem {
        Dem { zoom, tile_size: None, tiles: HashMap::new() }
    }

    pub fn zoom(&self) -> u8 {
        self.zoom
    }

    /// The width and height of the tiles, set by the first tile inserted.
    pub fn tile_size(&self) -> Option<(u32, u32)> {
        self.tile_size
    }

    pub fn tiles(&self) -> &HashMap<(u32, u32), DemTile> {
        &self.tiles
    }

    pub fn get(&self, x: u32, y: u32) -> Option<&DemTile> {
        self.tiles.get(&(x, y))
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Adds a tile, returning the one it replaces. The tile must be of the model's zoom and of
    /// the same size as the tiles already there.
    pub fn insert(&mut self, tile: DemTile) -> Result<Option<DemTile>, ElevationError> {
        DemTile::check(&tile.tile)?;
        if tile.tile.zoom != self.zoom {
            return Err(ElevationError::InvalidTile("zoom differs from the model's"));
        }
        if tile.heights.len() as u64 != u64::from(tile.width) * u64::from(tile.height) || tile.width == 0 || tile.height == 0 {
            return Err(ElevationError::InvalidTile("the heights do not match the size"));
        }
        let size = (tile.width, tile.height);
        if self.tile_size.is_some_and(|s| s != size) {
            return Err(ElevationError::InvalidTile("size differs from the other tiles'"));
        }
        self.tile_size = Some(size);
        Ok(self.tiles.insert((tile.tile.x, tile.tile.y), tile))
    }

    /// The height at `point`, interpolated bilinearly between pixel centres (see
    /// [`Raster::interpolate`]), with the pixels of neighbouring tiles where they are loaded.
    /// `None` if the tile holding the point is not loaded or there is no data around it.
    pub fn height(&self, point: &Coordinate) -> Option<f64> {
        let (width, height) = self.tile_size?;
        let (x, y) = MercatorPoint::from_coordinate(point)?.to_pixel(self.zoom, width, height);
        let n = 1u64 << self.zoom.min(DemTile::MAX_ZOOM);
        let (tx, ty) = ((x / f64::from(width)).floor(), (y / f64::from(height)).floor());
        // The east edge of the world (longitude 180) is the west edge of the first column.
        let tx = if tx >= n as f64 { 0.0 } else { tx };
        if !(0.0..n as f64).contains(&ty) || !self.tiles.contains_key(&(tx as u32, ty as u32)) {
            return None;
        }
        self.interpolate(x, y)
    }

    /// Loads every file of `dir` named `{z}-{x}-{y}.png` with `z` equal to `zoom` (the naming of
    /// [`Source::file_name`]), decoded with `encoding`. Other files are ignored. Files that
    /// cannot be read or decoded, or do not fit the model (another size than the first tile
    /// loaded, larger than [`Dem::MAX_FILE_BYTES`]), are skipped: the second value is how many.
    /// Only a directory that cannot be listed is an error.
    pub fn load_directory(dir: &Path, zoom: u8, encoding: Encoding) -> Result<(Dem, usize), ElevationError> {
        let mut dem = Dem::new(zoom);
        let mut skipped = 0;
        for entry in fs::read_dir(dir)? {
            let Ok(entry) = entry else {
                skipped += 1;
                continue;
            };
            let Some(tile) = entry.file_name().to_str().and_then(Source::parse_file_name) else { continue };
            if tile.zoom != zoom {
                continue;
            }
            let path = entry.path();
            let loaded = fs::metadata(&path)
                .ok()
                .filter(|m| m.is_file() && m.len() <= Self::MAX_FILE_BYTES)
                .and_then(|_| fs::read(&path).ok())
                .and_then(|bytes| DemTile::decode(tile, encoding, &bytes).ok())
                .and_then(|tile| dem.insert(tile).ok());
            if loaded.is_none() {
                skipped += 1;
            }
        }
        Ok((dem, skipped))
    }
}

impl Raster for Dem {
    /// The pixel at global pixel indices `(x, y)` of the model's zoom, from whichever tile holds
    /// it. Columns wrap around the antimeridian; rows beyond the poles have no data.
    fn value(&self, x: i64, y: i64) -> Option<f64> {
        let (width, height) = self.tile_size?;
        let n = 1i64 << self.zoom.min(DemTile::MAX_ZOOM);
        let x = x.rem_euclid(n.checked_mul(i64::from(width))?);
        if y < 0 || y >= n.checked_mul(i64::from(height))? {
            return None;
        }
        let (tx, px) = (x / i64::from(width), x % i64::from(width));
        let (ty, py) = (y / i64::from(height), y % i64::from(height));
        self.tiles.get(&(u32::try_from(tx).ok()?, u32::try_from(ty).ok()?))?.pixel(px as u32, py as u32)
    }
}
