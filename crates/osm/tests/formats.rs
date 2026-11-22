//! The readers against files written here by hand from the specification of each format.

use osm::{Element, Filter, MemberKind, Overpass, Pbf, PbfError};

/// A minimal Protocol Buffers writer, for building test files from the format's definition.
#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn varint(&mut self, mut v: u64) -> &mut Self {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(byte);
                return self;
            }
            self.0.push(byte | 0x80);
        }
    }
    fn key(&mut self, number: u32, wire: u8) -> &mut Self {
        self.varint(((number as u64) << 3) | wire as u64)
    }
    fn uint(&mut self, number: u32, v: u64) -> &mut Self {
        self.key(number, 0).varint(v)
    }
    fn sint(&mut self, number: u32, v: i64) -> &mut Self {
        self.uint(number, ((v << 1) ^ (v >> 63)) as u64)
    }
    fn bytes(&mut self, number: u32, b: &[u8]) -> &mut Self {
        self.key(number, 2).varint(b.len() as u64);
        self.0.extend_from_slice(b);
        self
    }
    fn packed(&mut self, number: u32, values: &[u64]) -> &mut Self {
        let mut inner = Writer::default();
        for &v in values {
            inner.varint(v);
        }
        self.bytes(number, &inner.0)
    }
    fn packed_sint_deltas(&mut self, number: u32, values: &[i64]) -> &mut Self {
        let mut last = 0;
        let zz: Vec<u64> = values
            .iter()
            .map(|&v| {
                let d = v - last;
                last = v;
                ((d << 1) ^ (d >> 63)) as u64
            })
            .collect();
        self.packed(number, &zz)
    }
}

fn blob(kind: &str, data: &[u8], compress: bool) -> Vec<u8> {
    let mut blob = Writer::default();
    if compress {
        blob.uint(2, data.len() as u64);
        blob.bytes(3, &miniz_oxide::deflate::compress_to_vec_zlib(data, 6));
    } else {
        blob.bytes(1, data);
    }
    let mut header = Writer::default();
    header.bytes(1, kind.as_bytes()).uint(3, blob.0.len() as u64);
    let mut out = (header.0.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&header.0);
    out.extend_from_slice(&blob.0);
    out
}

fn header_block(features: &[&str]) -> Vec<u8> {
    let mut h = Writer::default();
    for f in features {
        h.bytes(4, f.as_bytes());
    }
    h.0
}

/// One data block with: dense nodes 1 (tagged highway=traffic_signals) and 2, a plain node 3, a
/// way 10 over them, and a restriction relation 20.
fn data_block() -> Vec<u8> {
    let strings = ["", "highway", "traffic_signals", "residential", "name", "通り", "type", "restriction", "from", "via", "to"];
    let mut table = Writer::default();
    for s in strings {
        table.bytes(1, s.as_bytes());
    }
    // Coordinates in units of the granularity (100 nanodegrees): 35.0 and 139.0, etc.
    let mut dense = Writer::default();
    dense.packed_sint_deltas(1, &[1, 2]);
    dense.packed_sint_deltas(8, &[350_000_000, 350_010_000]);
    dense.packed_sint_deltas(9, &[1_390_000_000, 1_390_010_000]);
    dense.packed(10, &[1, 2, 0, 0]);
    let mut node = Writer::default();
    node.sint(1, 3).sint(8, 350_020_000).sint(9, 1_390_020_000);
    let mut way = Writer::default();
    way.uint(1, 10).packed(2, &[1, 4]).packed(3, &[3, 5]).packed_sint_deltas(8, &[1, 2, 3]);
    let mut relation = Writer::default();
    relation.uint(1, 20).packed(2, &[6]).packed(3, &[7]).packed(8, &[8, 9, 10]).packed_sint_deltas(9, &[10, 2, 11]).packed(10, &[1, 0, 1]);
    let mut group1 = Writer::default();
    group1.bytes(2, &dense.0).bytes(1, &node.0);
    let mut group2 = Writer::default();
    group2.bytes(3, &way.0);
    let mut group3 = Writer::default();
    group3.bytes(4, &relation.0);
    let mut block = Writer::default();
    block.bytes(1, &table.0).bytes(2, &group1.0).bytes(2, &group2.0).bytes(2, &group3.0);
    block.0
}

fn file(compress: bool) -> Vec<u8> {
    let mut file = blob("OSMHeader", &header_block(&["OsmSchema-V0.6", "DenseNodes"]), compress);
    file.extend(blob("OSMData", &data_block(), compress));
    file
}

#[test]
fn reads_every_kind_of_element() {
    for compress in [false, true] {
        let blocks: Vec<Vec<Element>> = Pbf::new(file(compress).as_slice(), Filter::ALL).collect::<Result<_, _>>().unwrap();
        assert_eq!(blocks.len(), 1);
        let elements = &blocks[0];
        assert_eq!(elements.len(), 5);
        let Element::Node(n1) = &elements[0] else { panic!() };
        assert_eq!(n1.id, 1);
        assert!((n1.coordinate.latitude - 35.0).abs() < 1e-9 && (n1.coordinate.longitude - 139.0).abs() < 1e-9);
        assert_eq!(n1.tags.get("highway"), Some("traffic_signals"));
        let Element::Node(n2) = &elements[1] else { panic!() };
        assert_eq!(n2.id, 2);
        assert!(n2.tags.is_empty());
        assert!((n2.coordinate.latitude - 35.001).abs() < 1e-9);
        let Element::Node(n3) = &elements[2] else { panic!() };
        assert_eq!(n3.id, 3);
        assert!((n3.coordinate.longitude - 139.002).abs() < 1e-9);
        let Element::Way(w) = &elements[3] else { panic!() };
        assert_eq!((w.id, w.nodes.as_slice()), (10, &[1, 2, 3][..]));
        assert_eq!(w.tags.get("name"), Some("通り"));
        let Element::Relation(r) = &elements[4] else { panic!() };
        assert_eq!(r.id, 20);
        assert_eq!(r.members.len(), 3);
        assert_eq!((r.members[1].kind, r.members[1].id, r.members[1].role.as_str()), (MemberKind::Node, 2, "via"));
        assert_eq!((r.members[2].kind, r.members[2].id), (MemberKind::Way, 11));
    }
}

#[test]
fn filter_skips_kinds() {
    let filter = Filter { nodes: false, ways: true, relations: false };
    let blocks: Vec<Vec<Element>> = Pbf::new(file(true).as_slice(), filter).collect::<Result<_, _>>().unwrap();
    assert_eq!(blocks[0].len(), 1);
    assert!(matches!(blocks[0][0], Element::Way(_)));
}

#[test]
fn rejects_unsupported_features_and_compressions() {
    let file = blob("OSMHeader", &header_block(&["OsmSchema-V0.6", "HistoricalInformation"]), true);
    assert!(matches!(Pbf::new(file.as_slice(), Filter::ALL).next(), Some(Err(PbfError::UnsupportedFeature(f))) if f == "HistoricalInformation"));

    let mut b = Writer::default();
    b.bytes(7, b"zstd data");
    let mut header = Writer::default();
    header.bytes(1, b"OSMHeader").uint(3, b.0.len() as u64);
    let mut file = (header.0.len() as u32).to_be_bytes().to_vec();
    file.extend_from_slice(&header.0);
    file.extend_from_slice(&b.0);
    assert!(matches!(Pbf::new(file.as_slice(), Filter::ALL).next(), Some(Err(PbfError::UnsupportedCompression("zstd")))));

    // Data before any header.
    let file = blob("OSMData", &data_block(), true);
    assert!(matches!(Pbf::new(file.as_slice(), Filter::ALL).next(), Some(Err(PbfError::Malformed(_)))));

    // A file cut inside a blob.
    let full = self::file(true);
    let cut = &full[..full.len() - 3];
    let results: Vec<_> = Pbf::new(cut, Filter::ALL).collect();
    assert!(matches!(results.last(), Some(Err(PbfError::Io(_)))));
}

#[test]
fn overpass_json() {
    let json = r#"{"version":0.6,"elements":[
        {"type":"node","id":1,"lat":35.0,"lon":139.0},
        {"type":"node","id":2,"lat":95.0,"lon":139.0},
        {"type":"way","id":10,"nodes":[1,3],"tags":{"highway":"primary","name":"国道1号"}},
        {"type":"relation","id":20,"members":[{"type":"way","ref":10,"role":"from"},{"type":"area","ref":1,"role":""}],"tags":{"type":"restriction"}},
        {"type":"area","id":30}
    ]}"#;
    let elements = Overpass::parse(json.as_bytes()).unwrap();
    assert_eq!(elements.len(), 3, "the invalid node and the area are skipped");
    let Element::Way(w) = &elements[1] else { panic!() };
    assert_eq!(w.tags.get("name"), Some("国道1号"));
    let Element::Relation(r) = &elements[2] else { panic!() };
    assert_eq!(r.members.len(), 1);

    let remark = r#"{"elements":[],"remark":"runtime error: Query timed out"}"#;
    assert!(Overpass::parse(remark.as_bytes()).is_err());
    assert_eq!(Overpass::parse(r#"{"elements":[]}"#.as_bytes()).unwrap().len(), 0);
}

#[test]
fn overpass_request() {
    let r = Overpass::request(Overpass::DEFAULT_SERVER, "[out:json];node(1);out;", "Myap/0");
    assert_eq!(r.body.as_deref(), Some("data=%5Bout%3Ajson%5D%3Bnode%281%29%3Bout%3B"));
    assert_eq!(r.header_value("user-agent"), Some("Myap/0"));
    let b = geo::Bounds { south: 35.0, west: 139.0, north: 35.1, east: 139.1 };
    assert_eq!(Overpass::bbox(&b), "35.0000000,139.0000000,35.1000000,139.1000000");
}
