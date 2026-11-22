//! A reader of the Protocol Buffers wire format
//! (<https://protobuf.dev/programming-guides/encoding/>), without schemas or generated code: a
//! message is read field by field, and the reader of each format decides what a field number
//! means. It borrows the bytes, so reading allocates nothing.
//!
//! Groups (wire types 3 and 4), deprecated since proto2, are rejected.

use std::fmt;

/// A field of a message: its number and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field<'a> {
    pub number: u32,
    pub value: Value<'a>,
}

/// The value of a field, as the wire format carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    /// Wire type 0: `int32`, `int64`, `uint32`, `uint64`, `sint32`, `sint64`, `bool`, `enum`.
    Varint(u64),
    /// Wire type 1: `fixed64`, `sfixed64`, `double`.
    Fixed64(u64),
    /// Wire type 2: `string`, `bytes`, embedded messages and packed repeated fields.
    Bytes(&'a [u8]),
    /// Wire type 5: `fixed32`, `sfixed32`, `float`.
    Fixed32(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    /// The message ended inside a field.
    Truncated,
    /// A varint longer than ten bytes.
    VarintTooLong,
    /// A wire type that is not 0, 1, 2 or 5.
    UnsupportedWireType(u8),
    /// A field of the wrong wire type for what was asked of it.
    UnexpectedType,
    /// A field number of 0.
    InvalidFieldNumber,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Truncated => write!(f, "truncated message"),
            WireError::VarintTooLong => write!(f, "varint longer than ten bytes"),
            WireError::UnsupportedWireType(t) => write!(f, "unsupported wire type {t}"),
            WireError::UnexpectedType => write!(f, "field of an unexpected wire type"),
            WireError::InvalidFieldNumber => write!(f, "field number 0"),
        }
    }
}

impl std::error::Error for WireError {}

impl<'a> Value<'a> {
    pub fn as_u64(&self) -> Result<u64, WireError> {
        match self {
            Value::Varint(v) | Value::Fixed64(v) => Ok(*v),
            Value::Fixed32(v) => Ok(*v as u64),
            Value::Bytes(_) => Err(WireError::UnexpectedType),
        }
    }

    /// An `int64` or `int32` (two's complement in a varint).
    pub fn as_i64(&self) -> Result<i64, WireError> {
        self.as_u64().map(|v| v as i64)
    }

    /// A `sint64` or `sint32` (zigzag in a varint).
    pub fn as_sint(&self) -> Result<i64, WireError> {
        self.as_u64().map(Message::zigzag)
    }

    pub fn as_bool(&self) -> Result<bool, WireError> {
        self.as_u64().map(|v| v != 0)
    }

    pub fn as_bytes(&self) -> Result<&'a [u8], WireError> {
        match self {
            Value::Bytes(b) => Ok(b),
            _ => Err(WireError::UnexpectedType),
        }
    }

    /// A `string`; invalid UTF-8 becomes U+FFFD.
    pub fn as_str(&self) -> Result<std::borrow::Cow<'a, str>, WireError> {
        self.as_bytes().map(String::from_utf8_lossy)
    }

    pub fn as_f64(&self) -> Result<f64, WireError> {
        match self {
            Value::Fixed64(v) => Ok(f64::from_bits(*v)),
            Value::Fixed32(v) => Ok(f32::from_bits(*v) as f64),
            _ => Err(WireError::UnexpectedType),
        }
    }

    /// The varints of a packed repeated field (or of a single unpacked one).
    pub fn packed_varints(&self) -> Packed<'a> {
        match self {
            Value::Bytes(b) => Packed { bytes: b, position: 0, single: None },
            other => Packed { bytes: &[], position: 0, single: other.as_u64().ok() },
        }
    }
}

/// The varints of a packed repeated field.
pub struct Packed<'a> {
    bytes: &'a [u8],
    position: usize,
    single: Option<u64>,
}

impl Iterator for Packed<'_> {
    type Item = Result<u64, WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(v) = self.single.take() {
            return Some(Ok(v));
        }
        if self.position >= self.bytes.len() {
            return None;
        }
        let result = Message::read_varint(self.bytes, &mut self.position);
        if result.is_err() {
            self.position = self.bytes.len();
        }
        Some(result)
    }
}

/// A message: an iterator of its fields. After an error the iterator ends.
#[derive(Debug, Clone)]
pub struct Message<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Message<'a> {
    pub fn new(bytes: &'a [u8]) -> Message<'a> {
        Message { bytes, position: 0 }
    }

    /// Decodes zigzag encoding (`sint32`, `sint64`).
    pub fn zigzag(value: u64) -> i64 {
        ((value >> 1) as i64) ^ -((value & 1) as i64)
    }

    /// Reads a varint at `position`, advancing it.
    pub fn read_varint(bytes: &[u8], position: &mut usize) -> Result<u64, WireError> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *bytes.get(*position).ok_or(WireError::Truncated)?;
            *position += 1;
            if shift == 63 && byte > 1 {
                return Err(WireError::VarintTooLong);
            }
            value |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(WireError::VarintTooLong)
    }

    fn read_field(&mut self) -> Result<Field<'a>, WireError> {
        let key = Self::read_varint(self.bytes, &mut self.position)?;
        let number = u32::try_from(key >> 3).map_err(|_| WireError::InvalidFieldNumber)?;
        if number == 0 {
            return Err(WireError::InvalidFieldNumber);
        }
        let value = match (key & 7) as u8 {
            0 => Value::Varint(Self::read_varint(self.bytes, &mut self.position)?),
            1 => Value::Fixed64(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| WireError::Truncated)?)),
            2 => {
                let length = Self::read_varint(self.bytes, &mut self.position)?;
                let length = usize::try_from(length).map_err(|_| WireError::Truncated)?;
                Value::Bytes(self.take(length)?)
            }
            5 => Value::Fixed32(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| WireError::Truncated)?)),
            other => return Err(WireError::UnsupportedWireType(other)),
        };
        Ok(Field { number, value })
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], WireError> {
        let end = self.position.checked_add(length).ok_or(WireError::Truncated)?;
        let slice = self.bytes.get(self.position..end).ok_or(WireError::Truncated)?;
        self.position = end;
        Ok(slice)
    }
}

impl<'a> Iterator for Message<'a> {
    type Item = Result<Field<'a>, WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.bytes.len() {
            return None;
        }
        let result = self.read_field();
        if result.is_err() {
            self.position = self.bytes.len();
        }
        Some(result)
    }
}
