//! Elevations from raster terrain tiles in Web Mercator.
//!
//! - [`Source`]: where tiles come from (URL template, zoom range, encoding, the attribution the
//!   provider requires), which tiles cover an area and at which zoom. [`Source::terrarium`] (AWS
//!   Open Data Terrain Tiles, worldwide) and [`Source::gsi`] (the elevation tiles of GSI Japan)
//!   are built in.
//! - [`Encoding`]: how a pixel's red, green and blue hold a height.
//! - [`DemTile`]: one decoded tile, [`Dem`]: a set of tiles of one zoom, sampled with bilinear
//!   interpolation across tile edges. Both are a [`Raster`].
//! - [`MercatorPoint`]: the Web Mercator projection, to and from the pixels of a zoom.
//!
//! Heights are in metres, as the source stores them (above mean sea level for both built-in
//! sources). A missing value is `None`, never a made-up height: no-data pixels, pixels of tiles
//! not loaded and points outside the projection all give `None`.
//!
//! This crate does no networking: fetching tiles is up to the caller. Tiles kept on disk are
//! named by [`Source::file_name`], the contract [`Dem::load_directory`] reads.

mod dem;
mod encoding;
mod error;
mod mercator;
mod raster;
mod source;
mod tile;

pub use dem::Dem;
pub use encoding::Encoding;
pub use error::ElevationError;
pub use mercator::MercatorPoint;
pub use raster::Raster;
pub use source::Source;
pub use tile::DemTile;
