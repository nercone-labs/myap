use std::fmt;

/// Why a PNG datastream could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngError {
    /// The data does not start with the PNG signature.
    Signature,
    /// The data ends in the middle of a chunk.
    Truncated,
    /// A chunk length above `2^31 - 1`, which the specification forbids.
    ChunkTooLong(u32),
    /// The CRC stored in a chunk does not match its type and data.
    Crc([u8; 4]),
    /// A critical chunk this decoder does not know, which it must not ignore.
    UnknownCriticalChunk([u8; 4]),
    /// A chunk the image requires is absent (`IHDR`, `PLTE`, `IDAT` or `IEND`).
    MissingChunk(&'static str),
    /// Chunks out of the order the specification sets, or repeated where it forbids it.
    Order(&'static str),
    /// The header holds a value the specification does not allow.
    InvalidHeader(&'static str),
    /// A chunk whose contents are invalid (its length, or values out of range).
    InvalidChunk(&'static str),
    /// The image is larger than the [`Limits`](crate::Limits) of the decoder.
    TooLarge { width: u32, height: u32 },
    /// The image data is not a valid zlib stream.
    Inflate,
    /// The image data inflates to fewer bytes than the header implies.
    MissingData,
    /// The image data inflates to more bytes than the header implies.
    ExtraData,
    /// A scanline starts with a filter type other than 0 to 4.
    InvalidFilter(u8),
}

impl PngError {
    fn name(kind: &[u8; 4]) -> String {
        String::from_utf8_lossy(kind).into_owned()
    }
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PngError::Signature => write!(f, "not a PNG file (bad signature)"),
            PngError::Truncated => write!(f, "the PNG data ends unexpectedly"),
            PngError::ChunkTooLong(n) => write!(f, "chunk length {n} exceeds 2^31 - 1"),
            PngError::Crc(kind) => write!(f, "CRC mismatch in chunk {}", Self::name(kind)),
            PngError::UnknownCriticalChunk(kind) => write!(f, "unknown critical chunk {}", Self::name(kind)),
            PngError::MissingChunk(name) => write!(f, "missing {name} chunk"),
            PngError::Order(what) => write!(f, "chunk order: {what}"),
            PngError::InvalidHeader(what) => write!(f, "invalid IHDR: {what}"),
            PngError::InvalidChunk(what) => write!(f, "invalid chunk: {what}"),
            PngError::TooLarge { width, height } => write!(f, "image of {width} x {height} is larger than the limits"),
            PngError::Inflate => write!(f, "the image data is not a valid zlib stream"),
            PngError::MissingData => write!(f, "the image data is shorter than the image"),
            PngError::ExtraData => write!(f, "the image data is longer than the image"),
            PngError::InvalidFilter(t) => write!(f, "invalid filter type {t}"),
        }
    }
}

impl std::error::Error for PngError {}
