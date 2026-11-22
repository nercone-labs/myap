use crate::Highway;
use geo::{Bounds, Coordinate};
use routing::TravelMode;
use std::fmt;
use std::io::{self, BufRead, Read, Write};

/// What a way is: one record per way of OpenStreetMap kept in the graph. The fields of a
/// direction (`*_backward`) apply to the edges that go against the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WayInfo {
    pub osm_id: i64,
    pub highway: Highway,
    /// The speed limit in km/h along the way (0 unknown); for a ferry, its speed over the crossing.
    pub maxspeed_kmh: u8,
    /// The speed limit against the way, when it differs (0: the same as along it).
    pub maxspeed_backward_kmh: u8,
    /// The number of lanes along and against the way (0 unknown).
    pub lanes: u8,
    pub lanes_backward: u8,
    /// The incline of the way along it, in percent, from its tags (0 unknown or flat): used where
    /// the graph has no elevations.
    pub incline_percent: i8,
    pub flags: u16,
    /// Indices in the strings of the graph, or `Graph::NONE`: the name, the number (`ref`), the
    /// lanes (`turn:lanes`) and the destinations of the signs, along and against the way.
    pub name: u32,
    pub road_ref: u32,
    pub turn_lanes: u32,
    pub turn_lanes_backward: u32,
    pub destination: u32,
    pub destination_backward: u32,
}

impl WayInfo {
    pub const TOLL: u16 = 1;
    pub const ROUNDABOUT: u16 = 2;
    /// Not paved (gravel, dirt, grass…).
    pub const UNPAVED: u16 = 4;
    /// Narrower than two cars can pass, or a single-lane side street.
    pub const NARROW: u16 = 8;
    /// Lit at night (`lit=yes`).
    pub const LIT: u16 = 16;
    /// Known to be unlit (`lit=no`).
    pub const UNLIT: u16 = 32;
    pub const BRIDGE: u16 = 64;
    pub const TUNNEL: u16 = 128;
    /// A lane or track for bicycles along it (`cycleway=lane|track`…).
    pub const CYCLE_LANE: u16 = 256;
    /// A rough surface (`smoothness` bad or worse).
    pub const ROUGH: u16 = 512;

    pub fn none(osm_id: i64, highway: Highway) -> WayInfo {
        WayInfo {
            osm_id,
            highway,
            maxspeed_kmh: 0,
            maxspeed_backward_kmh: 0,
            lanes: 0,
            lanes_backward: 0,
            incline_percent: 0,
            flags: 0,
            name: Graph::NONE,
            road_ref: Graph::NONE,
            turn_lanes: Graph::NONE,
            turn_lanes_backward: Graph::NONE,
            destination: Graph::NONE,
            destination_backward: Graph::NONE,
        }
    }

    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    pub fn is_toll(&self) -> bool {
        self.has(Self::TOLL)
    }

    pub fn is_roundabout(&self) -> bool {
        self.has(Self::ROUNDABOUT)
    }

    /// The speed limit in the direction of travel (`backward`: against the way), 0 unknown.
    pub fn maxspeed(&self, backward: bool) -> u8 {
        if backward && self.maxspeed_backward_kmh > 0 { self.maxspeed_backward_kmh } else { self.maxspeed_kmh }
    }
}

/// A directed edge: the road from its source to `target`, along a shape of points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub target: u32,
    pub way: u32,
    /// The length in decimetres.
    pub length_dm: u32,
    /// The points of the shape, from its source to its target (or the other way when
    /// `REVERSED`): `shapes[shape_start..shape_start + shape_len]`, both ends included.
    pub shape_start: u32,
    pub shape_len: u16,
    pub flags: u16,
}

impl Edge {
    pub const CAR: u16 = 1;
    pub const BICYCLE: u16 = 2;
    pub const FOOT: u16 = 4;
    /// The shape is stored in the other direction (this edge goes against its way).
    pub const REVERSED: u16 = 8;

    pub fn mode_flag(mode: TravelMode) -> u16 {
        match mode {
            TravelMode::Car => Self::CAR,
            TravelMode::Bicycle => Self::BICYCLE,
            _ => Self::FOOT,
        }
    }

    pub fn allows(&self, mode: TravelMode) -> bool {
        self.flags & Self::mode_flag(mode) != 0
    }

    pub fn is_reversed(&self) -> bool {
        self.flags & Self::REVERSED != 0
    }

    pub fn length_m(&self) -> f64 {
        self.length_dm as f64 / 10.0
    }
}

/// A turn restriction at a junction (`via`), from one way to another: `only` the turn to `to_way`
/// is allowed, or the turn to `to_way` is not. Restrictions apply to cars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Restriction {
    pub via: u32,
    pub from_way: u32,
    pub to_way: u32,
    pub only: bool,
}

/// What a node of the graph is, besides a junction: flags of `Graph::node_flags`.
pub struct NodeFlag;

impl NodeFlag {
    pub const SIGNALS: u16 = 1;
    pub const STOP: u16 = 2;
    pub const GIVE_WAY: u16 = 4;
    /// A railway level crossing (`railway=level_crossing|crossing`).
    pub const LEVEL_CROSSING: u16 = 8;
    pub const SPEED_CAMERA: u16 = 16;
    /// A crossing for pedestrians.
    pub const CROSSING: u16 = 32;
    pub const TOLL_BOOTH: u16 = 64;
    /// A junction of a motorway (`highway=motorway_junction`), whose name and number are those of
    /// the exit.
    pub const MOTORWAY_JUNCTION: u16 = 128;
    /// A barrier that cars, bicycles or walkers cannot pass.
    pub const BLOCKS_CAR: u16 = 256;
    pub const BLOCKS_BICYCLE: u16 = 512;
    pub const BLOCKS_FOOT: u16 = 1024;

    pub fn blocks(mode: TravelMode) -> u16 {
        match mode {
            TravelMode::Car => Self::BLOCKS_CAR,
            TravelMode::Bicycle => Self::BLOCKS_BICYCLE,
            _ => Self::BLOCKS_FOOT,
        }
    }
}

/// A name of a node: the name of a junction (交差点名) or the number of an exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeName {
    pub node: u32,
    pub kind: NodeNameKind,
    pub string: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum NodeNameKind {
    Name = 0,
    Ref = 1,
}

#[derive(Debug)]
pub enum GraphError {
    Io(io::Error),
    /// Not a graph of Myap, or of a version this one cannot read.
    Format(String),
    /// A graph whose parts do not agree: a damaged file.
    Invalid(&'static str),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::Io(e) => write!(f, "{e}"),
            GraphError::Format(s) => write!(f, "{s}"),
            GraphError::Invalid(s) => write!(f, "damaged graph: {s}"),
        }
    }
}

impl std::error::Error for GraphError {}

impl From<io::Error> for GraphError {
    fn from(e: io::Error) -> Self {
        GraphError::Io(e)
    }
}

/// The roads of an area, between their junctions, as a graph in compressed rows: the edges
/// leaving node `n` are `edges[first_edge[n]..first_edge[n + 1]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    /// Latitude and longitude of each node, in units of 10⁻⁷ degree.
    pub nodes: Vec<[i32; 2]>,
    pub first_edge: Vec<u32>,
    pub edges: Vec<Edge>,
    pub shapes: Vec<[i32; 2]>,
    pub ways: Vec<WayInfo>,
    /// The strings: string `i` is `text[string_offsets[i]..string_offsets[i + 1]]`.
    pub string_offsets: Vec<u32>,
    pub text: Vec<u8>,
    /// Sorted by `via`.
    pub restrictions: Vec<Restriction>,
    /// What each node is (`NodeFlag`), one per node.
    pub node_flags: Vec<u16>,
    /// The names of nodes, sorted.
    pub node_names: Vec<NodeName>,
    /// The elevation of each point of `shapes` in metres (`Graph::NO_ELEVATION` where unknown), or
    /// empty when the graph has none.
    pub elevations: Vec<i16>,
    pub bounds: Bounds,
    /// The source node of each edge (computed, not stored in the file).
    pub sources: Vec<u32>,
    /// The edge along the same road in the other direction of each edge, or `Graph::NONE`
    /// (computed, not stored in the file).
    pub twins: Vec<u32>,
}

impl Graph {
    pub const NONE: u32 = u32::MAX;
    pub const NO_ELEVATION: i16 = i16::MIN;
    pub const MAGIC: &'static [u8; 8] = b"MYAPGRPH";
    /// Version 2 added the lanes, signs, surfaces, widths and lights of ways, the barriers, names
    /// and signs of nodes, and elevations.
    pub const VERSION: u32 = 2;
    /// The bytes of each record in the file.
    const NODE_BYTES: u64 = 8;
    const EDGE_BYTES: u64 = 20;
    const WAY_BYTES: u64 = 40;
    const RESTRICTION_BYTES: u64 = 13;
    const NODE_NAME_BYTES: u64 = 9;

    pub fn coordinate(point: [i32; 2]) -> Coordinate {
        Coordinate::new(point[0] as f64 / 1e7, point[1] as f64 / 1e7)
    }

    pub fn fixed(coordinate: &Coordinate) -> [i32; 2] {
        [(coordinate.latitude * 1e7).round() as i32, (coordinate.longitude * 1e7).round() as i32]
    }

    pub fn node_coordinate(&self, node: u32) -> Coordinate {
        Self::coordinate(self.nodes[node as usize])
    }

    pub fn out_edges(&self, node: u32) -> std::ops::Range<u32> {
        self.first_edge[node as usize]..self.first_edge[node as usize + 1]
    }

    pub fn source(&self, edge: u32) -> u32 {
        self.sources[edge as usize]
    }

    pub fn way_of(&self, edge: u32) -> &WayInfo {
        &self.ways[self.edges[edge as usize].way as usize]
    }

    pub fn string(&self, index: u32) -> Option<&str> {
        if index == Self::NONE {
            return None;
        }
        let i = index as usize;
        let bytes = self.text.get(*self.string_offsets.get(i)? as usize..*self.string_offsets.get(i + 1)? as usize)?;
        std::str::from_utf8(bytes).ok().filter(|s| !s.is_empty())
    }

    /// The range of `edge`'s points in `shapes`.
    fn shape_range(&self, edge: u32) -> std::ops::Range<usize> {
        let e = &self.edges[edge as usize];
        e.shape_start as usize..e.shape_start as usize + e.shape_len as usize
    }

    /// The points of `edge`, from its source to its target.
    pub fn edge_points(&self, edge: u32) -> Vec<Coordinate> {
        let mut points: Vec<Coordinate> = self.shapes[self.shape_range(edge)].iter().map(|p| Self::coordinate(*p)).collect();
        if self.edges[edge as usize].is_reversed() {
            points.reverse();
        }
        points
    }

    /// The point `index` of `edge`'s points from its source (0) to its target, without
    /// allocating.
    pub fn edge_point(&self, edge: u32, index: usize) -> Coordinate {
        let range = self.shape_range(edge);
        let n = range.len();
        let i = if self.edges[edge as usize].is_reversed() { n - 1 - index.min(n - 1) } else { index.min(n - 1) };
        Self::coordinate(self.shapes[range.start + i])
    }

    pub fn has_elevations(&self) -> bool {
        !self.elevations.is_empty()
    }

    /// The elevations of `edge`'s points from its source, in metres (`None` where unknown), or
    /// `None` when the graph has none.
    pub fn edge_elevations(&self, edge: u32) -> Option<Vec<Option<f32>>> {
        if !self.has_elevations() {
            return None;
        }
        let mut values: Vec<Option<f32>> = self.elevations[self.shape_range(edge)].iter().map(|&e| (e != Self::NO_ELEVATION).then_some(e as f32)).collect();
        if self.edges[edge as usize].is_reversed() {
            values.reverse();
        }
        Some(values)
    }

    /// The edge that runs along the same road as `edge` in the other direction, if any.
    pub fn twin(&self, edge: u32) -> Option<u32> {
        self.twins.get(edge as usize).copied().filter(|&t| t != Self::NONE)
    }

    /// One of `edge` and its twin, the same for both: to count a road once.
    pub fn canonical(&self, edge: u32) -> u32 {
        self.twin(edge).map_or(edge, |t| t.min(edge))
    }

    /// Whether the box of `edge`'s points meets `bounds`: a test without allocation, that an
    /// edge outside `bounds` fails, before its points are measured.
    pub fn edge_meets(&self, edge: u32, bounds: &Bounds) -> bool {
        let (south, west) = (bounds.south * 1e7, bounds.west * 1e7);
        let (north, east) = (bounds.north * 1e7, bounds.east * 1e7);
        let mut lat = (i32::MAX, i32::MIN);
        let mut lon = (i32::MAX, i32::MIN);
        for p in &self.shapes[self.shape_range(edge)] {
            lat = (lat.0.min(p[0]), lat.1.max(p[0]));
            lon = (lon.0.min(p[1]), lon.1.max(p[1]));
        }
        lat.0 as f64 <= north && lat.1 as f64 >= south && lon.0 as f64 <= east && lon.1 as f64 >= west
    }

    pub fn node_has(&self, node: u32, flag: u16) -> bool {
        self.node_flags.get(node as usize).is_some_and(|f| f & flag != 0)
    }

    pub fn has_signals(&self, node: u32) -> bool {
        self.node_has(node, NodeFlag::SIGNALS)
    }

    /// The name of `node` of `kind`, when it has one.
    pub fn node_name(&self, node: u32, kind: NodeNameKind) -> Option<&str> {
        let i = self.node_names.partition_point(|n| (n.node, n.kind) < (node, kind));
        self.node_names.get(i).filter(|n| n.node == node && n.kind == kind).and_then(|n| self.string(n.string))
    }

    /// The restrictions at `node`.
    pub fn restrictions_at(&self, node: u32) -> &[Restriction] {
        let start = self.restrictions.partition_point(|r| r.via < node);
        let end = self.restrictions.partition_point(|r| r.via <= node);
        &self.restrictions[start..end]
    }

    /// Fills what is computed from the rows and not stored: the source and the twin of each
    /// edge. The graph must be valid (`validate`).
    pub fn compute_sources(&mut self) {
        let mut sources = vec![0u32; self.edges.len()];
        for node in 0..self.nodes.len() {
            for edge in self.first_edge[node]..self.first_edge[node + 1] {
                sources[edge as usize] = node as u32;
            }
        }
        self.sources = sources;
        let twins = (0..self.edges.len() as u32)
            .map(|edge| {
                let e = &self.edges[edge as usize];
                let source = self.sources[edge as usize];
                self.out_edges(e.target)
                    .find(|&f| {
                        let other = &self.edges[f as usize];
                        other.target == source && other.shape_start == e.shape_start && other.way == e.way && other.is_reversed() != e.is_reversed()
                    })
                    .unwrap_or(Self::NONE)
            })
            .collect();
        self.twins = twins;
    }

    /// Sets the elevation of every point of the shapes from `height` (metres), or removes them
    /// all when no point has one. A point without a height takes the nearest known one along its
    /// way's shape; one with none at all stays unknown.
    pub fn set_elevations(&mut self, height: impl Fn(&Coordinate) -> Option<f64>) -> usize {
        let mut values: Vec<i16> = self
            .shapes
            .iter()
            .map(|p| height(&Self::coordinate(*p)).filter(|h| h.is_finite()).map_or(Self::NO_ELEVATION, |h| h.round().clamp(-11_000.0, 9_000.0) as i16))
            .collect();
        let known = values.iter().filter(|v| **v != Self::NO_ELEVATION).count();
        if known == 0 {
            self.elevations.clear();
            return 0;
        }
        // Fill gaps within each edge's shape from its known points.
        let mut ranges: Vec<(usize, usize)> = self.edges.iter().map(|e| (e.shape_start as usize, e.shape_start as usize + e.shape_len as usize)).collect();
        ranges.sort_unstable();
        ranges.dedup();
        for (start, end) in ranges {
            let slice = &mut values[start..end];
            let Some(first) = slice.iter().position(|v| *v != Self::NO_ELEVATION) else { continue };
            let mut last = slice[first];
            for v in slice.iter_mut() {
                if *v == Self::NO_ELEVATION {
                    *v = last;
                } else {
                    last = *v;
                }
            }
        }
        self.elevations = values;
        known
    }

    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let mut w = io::BufWriter::new(out);
        w.write_all(Self::MAGIC)?;
        w.write_all(&Self::VERSION.to_le_bytes())?;
        for v in [self.bounds.south, self.bounds.west, self.bounds.north, self.bounds.east] {
            w.write_all(&v.to_le_bytes())?;
        }
        let counts = [
            self.nodes.len(),
            self.edges.len(),
            self.shapes.len(),
            self.ways.len(),
            self.string_offsets.len(),
            self.text.len(),
            self.restrictions.len(),
            self.node_names.len(),
            self.elevations.len(),
        ];
        for c in counts {
            let c = u32::try_from(c).map_err(|_| io::Error::other("graph too large"))?;
            w.write_all(&c.to_le_bytes())?;
        }
        for [a, b] in &self.nodes {
            w.write_all(&a.to_le_bytes())?;
            w.write_all(&b.to_le_bytes())?;
        }
        for v in &self.first_edge {
            w.write_all(&v.to_le_bytes())?;
        }
        for e in &self.edges {
            w.write_all(&e.target.to_le_bytes())?;
            w.write_all(&e.way.to_le_bytes())?;
            w.write_all(&e.length_dm.to_le_bytes())?;
            w.write_all(&e.shape_start.to_le_bytes())?;
            w.write_all(&e.shape_len.to_le_bytes())?;
            w.write_all(&e.flags.to_le_bytes())?;
        }
        for [a, b] in &self.shapes {
            w.write_all(&a.to_le_bytes())?;
            w.write_all(&b.to_le_bytes())?;
        }
        for way in &self.ways {
            w.write_all(&way.osm_id.to_le_bytes())?;
            w.write_all(&[way.highway as u8, way.maxspeed_kmh, way.maxspeed_backward_kmh, way.lanes, way.lanes_backward, way.incline_percent as u8])?;
            w.write_all(&way.flags.to_le_bytes())?;
            for s in [way.name, way.road_ref, way.turn_lanes, way.turn_lanes_backward, way.destination, way.destination_backward] {
                w.write_all(&s.to_le_bytes())?;
            }
        }
        for v in &self.string_offsets {
            w.write_all(&v.to_le_bytes())?;
        }
        w.write_all(&self.text)?;
        for r in &self.restrictions {
            w.write_all(&r.via.to_le_bytes())?;
            w.write_all(&r.from_way.to_le_bytes())?;
            w.write_all(&r.to_way.to_le_bytes())?;
            w.write_all(&[u8::from(r.only)])?;
        }
        for v in &self.node_flags {
            w.write_all(&v.to_le_bytes())?;
        }
        for n in &self.node_names {
            w.write_all(&n.node.to_le_bytes())?;
            w.write_all(&n.string.to_le_bytes())?;
            w.write_all(&[n.kind as u8])?;
        }
        for v in &self.elevations {
            w.write_all(&v.to_le_bytes())?;
        }
        w.flush()
    }

    /// Reads a graph, and checks every index in it: a damaged or hostile file is an error.
    ///
    /// The file is read as a stream, record by record, so that the memory of a read is that of
    /// the graph, not of the file and the graph. A count larger than the file cannot make a large
    /// allocation: the space of a part grows with the records actually read (`Reader::records`).
    pub fn read(input: &mut impl Read) -> Result<Graph, GraphError> {
        let mut r = Reader { inner: io::BufReader::with_capacity(1 << 16, input) };
        if &r.array::<8>()? != Self::MAGIC {
            return Err(GraphError::Format("not a graph of Myap".into()));
        }
        let version = r.u32()?;
        if version != Self::VERSION {
            return Err(GraphError::Format(format!("graph version {version}; this version of Myap reads version {}", Self::VERSION)));
        }
        let bounds = Bounds { south: r.f64()?, west: r.f64()?, north: r.f64()?, east: r.f64()? };
        let mut counts = [0u32; 9];
        for c in &mut counts {
            *c = r.u32()?;
        }
        let [nodes, edges, shapes, ways, strings, text, restrictions, node_names, elevations] = counts.map(|c| c as usize);
        let point = |b: &[u8]| -> Result<[i32; 2], GraphError> { Ok([Le::i32(&b[0..4]), Le::i32(&b[4..8])]) };
        let nodes_v = r.records(nodes, Self::NODE_BYTES as usize, point)?;
        let first_edge = r.records(nodes + 1, 4, |b| Ok(Le::u32(b)))?;
        let edges_v = r.records(edges, Self::EDGE_BYTES as usize, |b| {
            Ok(Edge {
                target: Le::u32(&b[0..4]),
                way: Le::u32(&b[4..8]),
                length_dm: Le::u32(&b[8..12]),
                shape_start: Le::u32(&b[12..16]),
                shape_len: Le::u16(&b[16..18]),
                flags: Le::u16(&b[18..20]),
            })
        })?;
        let shapes_v = r.records(shapes, 8, point)?;
        let ways_v = r.records(ways, Self::WAY_BYTES as usize, |b| {
            let highway = Highway::from_u8(b[8]).ok_or(GraphError::Invalid("unknown road class"))?;
            let string = |i: usize| Le::u32(&b[16 + 4 * i..20 + 4 * i]);
            Ok(WayInfo {
                osm_id: i64::from_le_bytes(b[0..8].try_into().map_err(|_| GraphError::Invalid("truncated"))?),
                highway,
                maxspeed_kmh: b[9],
                maxspeed_backward_kmh: b[10],
                lanes: b[11],
                lanes_backward: b[12],
                incline_percent: b[13] as i8,
                flags: Le::u16(&b[14..16]),
                name: string(0),
                road_ref: string(1),
                turn_lanes: string(2),
                turn_lanes_backward: string(3),
                destination: string(4),
                destination_backward: string(5),
            })
        })?;
        let string_offsets = r.records(strings, 4, |b| Ok(Le::u32(b)))?;
        let text_v = r.records(text, 1, |b| Ok(b[0]))?;
        let restrictions_v = r.records(restrictions, Self::RESTRICTION_BYTES as usize, |b| {
            Ok(Restriction { via: Le::u32(&b[0..4]), from_way: Le::u32(&b[4..8]), to_way: Le::u32(&b[8..12]), only: b[12] != 0 })
        })?;
        let node_flags = r.records(nodes, 2, |b| Ok(Le::u16(b)))?;
        let node_names_v = r.records(node_names, Self::NODE_NAME_BYTES as usize, |b| {
            let kind = match b[8] {
                0 => NodeNameKind::Name,
                1 => NodeNameKind::Ref,
                _ => return Err(GraphError::Invalid("unknown kind of node name")),
            };
            Ok(NodeName { node: Le::u32(&b[0..4]), kind, string: Le::u32(&b[4..8]) })
        })?;
        let elevations_v = r.records(elevations, 2, |b| Ok(i16::from_le_bytes([b[0], b[1]])))?;
        if !r.at_end()? {
            return Err(GraphError::Invalid("sizes do not match the file"));
        }
        let mut graph = Graph {
            nodes: nodes_v,
            first_edge,
            edges: edges_v,
            shapes: shapes_v,
            ways: ways_v,
            string_offsets,
            text: text_v,
            restrictions: restrictions_v,
            node_flags,
            node_names: node_names_v,
            elevations: elevations_v,
            bounds,
            sources: Vec::new(),
            twins: Vec::new(),
        };
        graph.validate()?;
        graph.compute_sources();
        Ok(graph)
    }

    /// Checks that every part of the graph refers only to parts that exist.
    pub fn validate(&self) -> Result<(), GraphError> {
        let invalid = GraphError::Invalid;
        let (n, e, s, w) = (self.nodes.len(), self.edges.len(), self.shapes.len(), self.ways.len());
        let strings = self.string_offsets.len().saturating_sub(1);
        if self.first_edge.len() != n + 1 || self.first_edge.first() != Some(&0) || self.first_edge.last().map(|&l| l as usize) != Some(e) {
            return Err(invalid("rows of edges"));
        }
        if self.first_edge.windows(2).any(|p| p[0] > p[1]) {
            return Err(invalid("rows of edges out of order"));
        }
        let valid_point = |p: &[i32; 2]| (-900_000_000..=900_000_000).contains(&p[0]) && (-1_800_000_000..=1_800_000_000).contains(&p[1]);
        if !self.nodes.iter().all(valid_point) || !self.shapes.iter().all(valid_point) {
            return Err(invalid("coordinate out of range"));
        }
        for edge in &self.edges {
            // In 64 bits: on 32-bit systems the sum could wrap past a check in `usize`.
            if edge.target as usize >= n || edge.way as usize >= w || edge.shape_len < 2 || edge.shape_start as u64 + edge.shape_len as u64 > s as u64 {
                return Err(invalid("edge"));
            }
        }
        let string_ok = |index: u32| index == Self::NONE || (index as usize) < strings;
        for way in &self.ways {
            if ![way.name, way.road_ref, way.turn_lanes, way.turn_lanes_backward, way.destination, way.destination_backward].into_iter().all(string_ok) {
                return Err(invalid("string index"));
            }
        }
        if !self.string_offsets.is_empty()
            && (self.string_offsets[0] != 0
                || self.string_offsets.windows(2).any(|p| p[0] > p[1])
                || *self.string_offsets.last().unwrap_or(&0) as usize != self.text.len())
        {
            return Err(invalid("strings"));
        }
        if self.string_offsets.is_empty() && !self.text.is_empty() {
            return Err(invalid("strings"));
        }
        for r in &self.restrictions {
            if r.via as usize >= n || r.from_way as usize >= w || r.to_way as usize >= w {
                return Err(invalid("restriction"));
            }
        }
        if self.restrictions.windows(2).any(|p| p[0].via > p[1].via) {
            return Err(invalid("restrictions out of order"));
        }
        if self.node_flags.len() != n {
            return Err(invalid("node flags"));
        }
        if self.node_names.iter().any(|x| x.node as usize >= n || x.string == Self::NONE || !string_ok(x.string))
            || self.node_names.windows(2).any(|p| (p[0].node, p[0].kind) >= (p[1].node, p[1].kind))
        {
            return Err(invalid("node names"));
        }
        if !self.elevations.is_empty() && self.elevations.len() != s {
            return Err(invalid("elevations"));
        }
        if ![self.bounds.south, self.bounds.west, self.bounds.north, self.bounds.east].iter().all(|v| v.is_finite()) {
            return Err(invalid("bounds"));
        }
        Ok(())
    }
}

/// Little-endian numbers from the bytes of a record (always long enough: `Reader::records`
/// gives each record its full size).
struct Le;

impl Le {
    fn u16(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[0], b[1]])
    }

    fn u32(b: &[u8]) -> u32 {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    fn i32(b: &[u8]) -> i32 {
        i32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}

/// A reader of the records of a graph file, as a stream.
struct Reader<R: Read> {
    inner: io::BufReader<R>,
}

impl<R: Read> Reader<R> {
    /// The bytes read at once for the records of a part.
    const CHUNK_BYTES: usize = 1 << 16;

    fn fill(&mut self, buf: &mut [u8]) -> Result<(), GraphError> {
        self.inner.read_exact(buf).map_err(|e| if e.kind() == io::ErrorKind::UnexpectedEof { GraphError::Invalid("truncated") } else { GraphError::Io(e) })
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], GraphError> {
        let mut b = [0u8; N];
        self.fill(&mut b)?;
        Ok(b)
    }

    fn u32(&mut self) -> Result<u32, GraphError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn f64(&mut self) -> Result<f64, GraphError> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    /// `count` records of `size` bytes, each read by `parse`. The vector grows with the records
    /// read, at most doubling, never beyond `count`: a count larger than the file ends with
    /// "truncated" after using no more memory than twice the file.
    fn records<T>(&mut self, count: usize, size: usize, parse: impl Fn(&[u8]) -> Result<T, GraphError>) -> Result<Vec<T>, GraphError> {
        let per_chunk = (Self::CHUNK_BYTES / size).max(1);
        let mut out: Vec<T> = Vec::new();
        let mut buf = vec![0u8; per_chunk.min(count) * size];
        let mut left = count;
        while left > 0 {
            let n = left.min(per_chunk);
            let bytes = &mut buf[..n * size];
            self.fill(bytes)?;
            if out.capacity() - out.len() < n {
                out.reserve_exact(out.len().max(n).min(count - out.len()));
            }
            for record in bytes.chunks_exact(size) {
                out.push(parse(record)?);
            }
            left -= n;
        }
        Ok(out)
    }

    /// Whether nothing is left to read.
    fn at_end(&mut self) -> Result<bool, GraphError> {
        Ok(self.inner.fill_buf()?.is_empty())
    }
}
