//! A decoder of PNG images, following the W3C PNG specification (<https://www.w3.org/TR/png-3/>).
//!
//! [`Image::decode`] reads a whole PNG datastream and either returns the image or an error; it
//! never panics, whatever the bytes. It decodes every colour type and bit depth the
//! specification allows, interlaced (Adam7) or not, and checks:
//!
//! - the signature, and the CRC of every chunk;
//! - the header (sizes, colour type and bit depth, methods) and the order of the critical chunks;
//! - the size of the image against [`Limits`] before anything is allocated, and that the image
//!   data inflates to exactly the size the header implies, so a decompression bomb is an error.
//!
//! Ancillary chunks other than `tRNS` are ignored; an unknown critical chunk is an error, as the
//! specification requires. Colour management (`gAMA`, `iCCP`, ...) is not applied: samples are
//! returned as stored.
//!
//! The parts are usable on their own: [`Crc32`] (the CRC-32 that PNG and gzip use), [`Chunk`]
//! (the chunk layout), [`Filter`] (the scanline filters) and [`Pass`] (the passes of Adam7).

mod adam7;
mod chunk;
mod crc;
mod error;
mod filter;
mod image;

pub use adam7::Pass;
pub use chunk::Chunk;
pub use crc::Crc32;
pub use error::PngError;
pub use filter::Filter;
pub use image::{ColorType, Image, Limits, Transparency};
