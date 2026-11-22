use crate::{Crc32, PngError};

/// One chunk of a PNG datastream: a type and its data, borrowed from the datastream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub kind: [u8; 4],
    pub data: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// The longest chunk data the specification allows.
    pub const MAX_LENGTH: u32 = 0x7FFF_FFFF;

    /// Reads the chunk at the start of `bytes` (length, type, data, CRC), checks its length and
    /// CRC, and returns it with the bytes after it.
    pub fn parse(bytes: &'a [u8]) -> Result<(Chunk<'a>, &'a [u8]), PngError> {
        let (length, rest) = bytes.split_first_chunk::<4>().ok_or(PngError::Truncated)?;
        let length = u32::from_be_bytes(*length);
        if length > Self::MAX_LENGTH {
            return Err(PngError::ChunkTooLong(length));
        }
        let body_length = (length as usize).checked_add(4).ok_or(PngError::Truncated)?;
        if rest.len() < body_length {
            return Err(PngError::Truncated);
        }
        let (body, rest) = rest.split_at(body_length);
        let (crc, rest) = rest.split_first_chunk::<4>().ok_or(PngError::Truncated)?;
        let (kind, data) = body.split_first_chunk::<4>().ok_or(PngError::Truncated)?;
        if Crc32::of(body) != u32::from_be_bytes(*crc) {
            return Err(PngError::Crc(*kind));
        }
        Ok((Chunk { kind: *kind, data }, rest))
    }

    /// Whether the chunk is critical: bit 5 of its first byte is 0 (an upper-case letter).
    pub fn is_critical(&self) -> bool {
        self.kind[0] & 0x20 == 0
    }
}
