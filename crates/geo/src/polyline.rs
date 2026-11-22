use crate::Coordinate;
use std::fmt;

/// The encoded polyline format of Google: each coordinate is the difference from the previous one,
/// rounded to `10^-precision` degrees, written in 5-bit chunks offset by 63. OSRM and Valhalla use
/// it with a precision of 6, Google with 5.
pub struct Polyline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolylineError {
    /// A character outside the alphabet of the format (63 to 126).
    InvalidCharacter(usize),
    /// The text ended in the middle of a value or of a pair.
    Truncated,
    /// A decoded coordinate is out of range.
    OutOfRange,
}

impl fmt::Display for PolylineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolylineError::InvalidCharacter(at) => write!(f, "invalid character at {at}"),
            PolylineError::Truncated => write!(f, "truncated polyline"),
            PolylineError::OutOfRange => write!(f, "coordinate out of range"),
        }
    }
}

impl std::error::Error for PolylineError {}

impl Polyline {
    pub fn encode(points: &[Coordinate], precision: u32) -> String {
        let factor = 10f64.powi(precision as i32);
        let mut out = String::with_capacity(points.len() * 8);
        let (mut last_lat, mut last_lon) = (0i64, 0i64);
        for point in points {
            let lat = (point.latitude * factor).round() as i64;
            let lon = (point.longitude * factor).round() as i64;
            Self::encode_value(lat - last_lat, &mut out);
            Self::encode_value(lon - last_lon, &mut out);
            (last_lat, last_lon) = (lat, lon);
        }
        out
    }

    pub fn decode(text: &str, precision: u32) -> Result<Vec<Coordinate>, PolylineError> {
        let factor = 10f64.powi(precision as i32);
        let bytes = text.as_bytes();
        let mut points = Vec::with_capacity(bytes.len() / 6);
        let (mut index, mut lat, mut lon) = (0usize, 0i64, 0i64);
        while index < bytes.len() {
            lat += Self::decode_value(bytes, &mut index)?;
            if index >= bytes.len() {
                return Err(PolylineError::Truncated);
            }
            lon += Self::decode_value(bytes, &mut index)?;
            let point = Coordinate::checked(lat as f64 / factor, lon as f64 / factor).ok_or(PolylineError::OutOfRange)?;
            points.push(point);
        }
        Ok(points)
    }

    fn encode_value(value: i64, out: &mut String) {
        let mut v = if value < 0 { !(value << 1) } else { value << 1 } as u64;
        while v >= 0x20 {
            out.push((((v & 0x1f) | 0x20) as u8 + 63) as char);
            v >>= 5;
        }
        out.push((v as u8 + 63) as char);
    }

    fn decode_value(bytes: &[u8], index: &mut usize) -> Result<i64, PolylineError> {
        let (mut result, mut shift) = (0u64, 0u32);
        loop {
            let byte = *bytes.get(*index).ok_or(PolylineError::Truncated)?;
            if !(63..=126).contains(&byte) {
                return Err(PolylineError::InvalidCharacter(*index));
            }
            *index += 1;
            let chunk = (byte - 63) as u64;
            if shift >= 64 {
                return Err(PolylineError::OutOfRange);
            }
            result |= (chunk & 0x1f) << shift;
            shift += 5;
            if chunk < 0x20 {
                break;
            }
        }
        let value = (result >> 1) as i64;
        Ok(if result & 1 == 1 { !value } else { value })
    }
}
