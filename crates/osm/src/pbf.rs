use crate::{Element, Member, MemberKind, Node, Relation, Tags, Way};
use geo::Coordinate;
use protowire::{Message, Value, WireError};
use std::fmt;
use std::io::{self, Read};

/// Which kinds of element to decode. Blocks are still decompressed, but the groups of the other
/// kinds are skipped without building their elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    pub nodes: bool,
    pub ways: bool,
    pub relations: bool,
}

impl Filter {
    pub const ALL: Filter = Filter { nodes: true, ways: true, relations: true };
}

#[derive(Debug)]
pub enum PbfError {
    Io(io::Error),
    Wire(WireError),
    /// A blob header or blob too large to be valid (the format limits them to 64 KiB and 32 MiB).
    TooLarge(usize),
    /// A compression other than zlib, or none at all.
    UnsupportedCompression(&'static str),
    /// The zlib data could not be inflated.
    Inflate,
    /// The header requires a feature this reader does not implement.
    UnsupportedFeature(String),
    /// Something the format requires is missing.
    Malformed(&'static str),
}

impl fmt::Display for PbfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PbfError::Io(e) => write!(f, "{e}"),
            PbfError::Wire(e) => write!(f, "{e}"),
            PbfError::TooLarge(n) => write!(f, "block of {n} bytes is larger than the format allows"),
            PbfError::UnsupportedCompression(c) => write!(f, "unsupported compression: {c}"),
            PbfError::Inflate => write!(f, "the zlib data could not be inflated"),
            PbfError::UnsupportedFeature(name) => write!(f, "unsupported required feature: {name}"),
            PbfError::Malformed(what) => write!(f, "malformed file: {what}"),
        }
    }
}

impl std::error::Error for PbfError {}

impl From<io::Error> for PbfError {
    fn from(e: io::Error) -> Self {
        PbfError::Io(e)
    }
}

impl From<WireError> for PbfError {
    fn from(e: WireError) -> Self {
        PbfError::Wire(e)
    }
}

/// A reader of the OpenStreetMap PBF format, block by block, so that a file of any size is read
/// in the memory of one block (at most 32 MiB inflated).
pub struct Pbf<R: Read> {
    reader: R,
    filter: Filter,
    header_seen: bool,
}

/// The state of a primitive block that its elements need: the strings and the coordinates' scale.
struct Block<'a> {
    strings: Vec<&'a [u8]>,
    granularity: i64,
    lat_offset: i64,
    lon_offset: i64,
}

impl<R: Read> Pbf<R> {
    /// The largest blob header the format allows.
    pub const MAX_HEADER: usize = 64 * 1024;
    /// The largest blob, compressed or not, the format allows.
    pub const MAX_BLOB: usize = 32 * 1024 * 1024;
    /// The features of the header this reader implements.
    pub const SUPPORTED_FEATURES: [&'static str; 2] = ["OsmSchema-V0.6", "DenseNodes"];

    pub fn new(reader: R, filter: Filter) -> Pbf<R> {
        Pbf { reader, filter, header_seen: false }
    }

    /// The elements of the next data block, or `None` at the end of the file. The header block is
    /// checked and skipped.
    pub fn next_block(&mut self) -> Option<Result<Vec<Element>, PbfError>> {
        loop {
            match self.read_blob() {
                Ok(None) => return None,
                Ok(Some((kind, data))) => match kind.as_str() {
                    "OSMHeader" => {
                        if let Err(e) = Self::check_header(&data) {
                            return Some(Err(e));
                        }
                        self.header_seen = true;
                    }
                    "OSMData" => {
                        if !self.header_seen {
                            return Some(Err(PbfError::Malformed("data before the header")));
                        }
                        return Some(self.read_data(&data));
                    }
                    // Unknown blob types are to be skipped (the format's rule).
                    _ => {}
                },
                Err(e) => return Some(Err(e)),
            }
        }
    }

    fn read_blob(&mut self) -> Result<Option<(String, Vec<u8>)>, PbfError> {
        let mut length = [0u8; 4];
        match self.reader.read_exact(&mut length) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let length = u32::from_be_bytes(length) as usize;
        if length > Self::MAX_HEADER {
            return Err(PbfError::TooLarge(length));
        }
        let mut header = vec![0u8; length];
        self.reader.read_exact(&mut header)?;
        let (mut kind, mut size) = (None, None);
        for field in Message::new(&header) {
            let field = field?;
            match field.number {
                1 => kind = Some(field.value.as_str()?.into_owned()),
                3 => size = Some(field.value.as_u64()? as usize),
                _ => {}
            }
        }
        let kind = kind.ok_or(PbfError::Malformed("blob header without a type"))?;
        let size = size.ok_or(PbfError::Malformed("blob header without a size"))?;
        if size > Self::MAX_BLOB {
            return Err(PbfError::TooLarge(size));
        }
        let mut blob = vec![0u8; size];
        self.reader.read_exact(&mut blob)?;
        Ok(Some((kind, Self::inflate_blob(&blob)?)))
    }

    fn inflate_blob(blob: &[u8]) -> Result<Vec<u8>, PbfError> {
        let mut raw_size = None;
        for field in Message::new(blob) {
            let field = field?;
            match field.number {
                1 => return Ok(field.value.as_bytes()?.to_vec()),
                2 => raw_size = Some(field.value.as_u64()? as usize),
                3 => {
                    let limit = raw_size.unwrap_or(Self::MAX_BLOB).min(Self::MAX_BLOB);
                    return miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(field.value.as_bytes()?, limit).map_err(|_| PbfError::Inflate);
                }
                4 => return Err(PbfError::UnsupportedCompression("lzma")),
                5 => return Err(PbfError::UnsupportedCompression("bzip2")),
                6 => return Err(PbfError::UnsupportedCompression("lz4")),
                7 => return Err(PbfError::UnsupportedCompression("zstd")),
                _ => {}
            }
        }
        Err(PbfError::UnsupportedCompression("none"))
    }

    fn check_header(data: &[u8]) -> Result<(), PbfError> {
        for field in Message::new(data) {
            let field = field?;
            if field.number == 4 {
                let feature = field.value.as_str()?;
                if !Self::SUPPORTED_FEATURES.contains(&feature.as_ref()) {
                    return Err(PbfError::UnsupportedFeature(feature.into_owned()));
                }
            }
        }
        Ok(())
    }

    fn read_data(&self, data: &[u8]) -> Result<Vec<Element>, PbfError> {
        let mut block = Block { strings: Vec::new(), granularity: 100, lat_offset: 0, lon_offset: 0 };
        let mut groups = Vec::new();
        for field in Message::new(data) {
            let field = field?;
            match field.number {
                1 => {
                    for s in Message::new(field.value.as_bytes()?) {
                        let s = s?;
                        if s.number == 1 {
                            block.strings.push(s.value.as_bytes()?);
                        }
                    }
                }
                2 => groups.push(field.value.as_bytes()?),
                17 => block.granularity = field.value.as_i64()?,
                19 => block.lat_offset = field.value.as_i64()?,
                20 => block.lon_offset = field.value.as_i64()?,
                _ => {}
            }
        }
        let mut elements = Vec::new();
        for group in groups {
            for field in Message::new(group) {
                let field = field?;
                let bytes = field.value.as_bytes()?;
                match field.number {
                    1 if self.filter.nodes => elements.push(Element::Node(block.node(bytes)?)),
                    2 if self.filter.nodes => block.dense_nodes(bytes, &mut elements)?,
                    3 if self.filter.ways => elements.push(Element::Way(block.way(bytes)?)),
                    4 if self.filter.relations => elements.push(Element::Relation(block.relation(bytes)?)),
                    _ => {}
                }
            }
        }
        Ok(elements)
    }
}

impl<R: Read> Iterator for Pbf<R> {
    type Item = Result<Vec<Element>, PbfError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_block()
    }
}

impl<'a> Block<'a> {
    fn string(&self, index: u64) -> Result<String, PbfError> {
        let bytes = self.strings.get(index as usize).ok_or(PbfError::Malformed("string index out of the table"))?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn coordinate(&self, lat: i64, lon: i64) -> Result<Coordinate, PbfError> {
        let latitude = 1e-9 * (self.lat_offset + self.granularity * lat) as f64;
        let longitude = 1e-9 * (self.lon_offset + self.granularity * lon) as f64;
        Coordinate::checked(latitude, longitude).ok_or(PbfError::Malformed("coordinate out of range"))
    }

    fn tags(&self, keys: &[u64], values: &[u64]) -> Result<Tags, PbfError> {
        if keys.len() != values.len() {
            return Err(PbfError::Malformed("keys and values of different lengths"));
        }
        keys.iter().zip(values).map(|(&k, &v)| Ok((self.string(k)?, self.string(v)?))).collect::<Result<_, _>>().map(Tags)
    }

    fn varints(value: &Value) -> Result<Vec<u64>, PbfError> {
        Ok(value.packed_varints().collect::<Result<_, _>>()?)
    }

    /// Delta-coded `sint64`s.
    fn deltas(value: &Value) -> Result<Vec<i64>, PbfError> {
        let mut last = 0i64;
        value
            .packed_varints()
            .map(|v| {
                last = last.wrapping_add(Message::zigzag(v?));
                Ok(last)
            })
            .collect()
    }

    fn node(&self, bytes: &[u8]) -> Result<Node, PbfError> {
        let (mut id, mut lat, mut lon) = (None, None, None);
        let (mut keys, mut values) = (Vec::new(), Vec::new());
        for field in Message::new(bytes) {
            let field = field?;
            match field.number {
                1 => id = Some(field.value.as_sint()?),
                2 => keys = Self::varints(&field.value)?,
                3 => values = Self::varints(&field.value)?,
                8 => lat = Some(field.value.as_sint()?),
                9 => lon = Some(field.value.as_sint()?),
                _ => {}
            }
        }
        let (id, lat, lon) = (id.ok_or(PbfError::Malformed("node without id"))?, lat.unwrap_or(0), lon.unwrap_or(0));
        Ok(Node { id, coordinate: self.coordinate(lat, lon)?, tags: self.tags(&keys, &values)? })
    }

    fn dense_nodes(&self, bytes: &[u8], out: &mut Vec<Element>) -> Result<(), PbfError> {
        let (mut ids, mut lats, mut lons, mut keys_vals) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for field in Message::new(bytes) {
            let field = field?;
            match field.number {
                1 => ids = Self::deltas(&field.value)?,
                8 => lats = Self::deltas(&field.value)?,
                9 => lons = Self::deltas(&field.value)?,
                10 => keys_vals = Self::varints(&field.value)?,
                _ => {}
            }
        }
        if ids.len() != lats.len() || ids.len() != lons.len() {
            return Err(PbfError::Malformed("dense nodes of different lengths"));
        }
        let mut kv = keys_vals.iter();
        for i in 0..ids.len() {
            let mut tags = Vec::new();
            // Each node's pairs end with a 0; a block without tags has no `keys_vals` at all.
            if !keys_vals.is_empty() {
                loop {
                    match kv.next() {
                        None | Some(0) => break,
                        Some(&k) => {
                            let v = *kv.next().ok_or(PbfError::Malformed("dense key without a value"))?;
                            tags.push((self.string(k)?, self.string(v)?));
                        }
                    }
                }
            }
            out.push(Element::Node(Node { id: ids[i], coordinate: self.coordinate(lats[i], lons[i])?, tags: Tags(tags) }));
        }
        Ok(())
    }

    fn way(&self, bytes: &[u8]) -> Result<Way, PbfError> {
        let (mut id, mut refs) = (None, Vec::new());
        let (mut keys, mut values) = (Vec::new(), Vec::new());
        for field in Message::new(bytes) {
            let field = field?;
            match field.number {
                1 => id = Some(field.value.as_i64()?),
                2 => keys = Self::varints(&field.value)?,
                3 => values = Self::varints(&field.value)?,
                8 => refs = Self::deltas(&field.value)?,
                _ => {}
            }
        }
        Ok(Way { id: id.ok_or(PbfError::Malformed("way without id"))?, nodes: refs, tags: self.tags(&keys, &values)? })
    }

    fn relation(&self, bytes: &[u8]) -> Result<Relation, PbfError> {
        let (mut id, mut roles, mut ids, mut types) = (None, Vec::new(), Vec::new(), Vec::new());
        let (mut keys, mut values) = (Vec::new(), Vec::new());
        for field in Message::new(bytes) {
            let field = field?;
            match field.number {
                1 => id = Some(field.value.as_i64()?),
                2 => keys = Self::varints(&field.value)?,
                3 => values = Self::varints(&field.value)?,
                8 => roles = Self::varints(&field.value)?,
                9 => ids = Self::deltas(&field.value)?,
                10 => types = Self::varints(&field.value)?,
                _ => {}
            }
        }
        if roles.len() != ids.len() || ids.len() != types.len() {
            return Err(PbfError::Malformed("relation members of different lengths"));
        }
        let members = (0..ids.len())
            .map(|i| {
                let kind = match types[i] {
                    0 => MemberKind::Node,
                    1 => MemberKind::Way,
                    2 => MemberKind::Relation,
                    _ => return Err(PbfError::Malformed("unknown member type")),
                };
                Ok(Member { kind, id: ids[i], role: self.string(roles[i] as u32 as u64)? })
            })
            .collect::<Result<_, _>>()?;
        Ok(Relation { id: id.ok_or(PbfError::Malformed("relation without id"))?, members, tags: self.tags(&keys, &values)? })
    }
}
