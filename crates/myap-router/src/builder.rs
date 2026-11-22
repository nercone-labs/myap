use crate::{Edge, Graph, Highway, NodeFlag, NodeName, NodeNameKind, Profile, Restriction, WayInfo};
use geo::{Bounds, Coordinate};
use osm::{Element, Filter, MemberKind, Node, Pbf, PbfError, Relation, Tags, Way};
use routing::TravelMode;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::rc::Rc;

#[derive(Debug)]
pub enum BuildError {
    Io(io::Error),
    Pbf(PbfError),
    /// The data holds no road that any mode may use.
    Empty,
    /// More nodes, edges or points than the format can number.
    TooLarge,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Io(e) => write!(f, "{e}"),
            BuildError::Pbf(e) => write!(f, "{e}"),
            BuildError::Empty => write!(f, "no roads in the data"),
            BuildError::TooLarge => write!(f, "too much data for one graph"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<io::Error> for BuildError {
    fn from(e: io::Error) -> Self {
        BuildError::Io(e)
    }
}

impl From<PbfError> for BuildError {
    fn from(e: PbfError) -> Self {
        BuildError::Pbf(e)
    }
}

/// What a build kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildStats {
    pub ways: usize,
    pub nodes: usize,
    pub edges: usize,
    pub restrictions: usize,
}

/// A way kept for the graph.
struct WayRecord {
    info: WayInfo,
    nodes: Vec<i64>,
    /// The modes allowed along the way and against it (`Edge::CAR` | …).
    forward: u16,
    backward: u16,
    /// For a ferry, its duration in seconds, to set its speed once its length is known.
    duration_s: Option<u32>,
}

/// A restriction as OpenStreetMap gives it, before its ways and node are in the graph.
struct RawRestriction {
    from: i64,
    via: i64,
    to: i64,
    only: bool,
}

/// Builds a [`Graph`] from OpenStreetMap elements.
///
/// The ways and relations must be given before the nodes (the order of PBF files), since only
/// the nodes of kept ways are remembered: [`add_way`](Self::add_way) and
/// [`add_relation`](Self::add_relation) first, then [`add_node`](Self::add_node), then
/// [`build`](Self::build). [`from_elements`](Self::from_elements) and [`from_pbf`](Self::from_pbf)
/// do this in that order.
#[derive(Default)]
pub struct GraphBuilder {
    ways: Vec<WayRecord>,
    restrictions: Vec<RawRestriction>,
    strings: Vec<String>,
    string_index: HashMap<String, u32>,
    /// Sorted ids of the nodes of kept ways, and their coordinates once read.
    needed: Vec<i64>,
    coordinates: Vec<Option<[i32; 2]>>,
    /// How many times each needed node is used by ways (a junction when 2 or more).
    uses: Vec<u32>,
    /// What each needed node is (`NodeFlag`), and its names.
    node_flags: Vec<u16>,
    node_names: Vec<(usize, NodeNameKind, u32)>,
}

impl GraphBuilder {
    pub fn new() -> GraphBuilder {
        GraphBuilder::default()
    }

    /// The Overpass QL query for what a graph of `bounds` needs: the roads and ferries, their
    /// nodes, and the turn restrictions.
    pub fn overpass_query(bounds: &Bounds) -> String {
        let bbox = osm::Overpass::bbox(bounds);
        format!(
            "[out:json][timeout:180][maxsize:536870912];(way[\"highway\"]({bbox});way[\"route\"=\"ferry\"]({bbox}););(._;>;);out body qt;relation[\"type\"~\"^restriction\"]({bbox});out body qt;"
        )
    }

    /// Builds a graph from a list of elements in any order (for example a response of Overpass).
    pub fn from_elements(elements: &[Element]) -> Result<(Graph, BuildStats), BuildError> {
        let mut builder = GraphBuilder::new();
        for element in elements {
            match element {
                Element::Way(way) => builder.add_way(way),
                Element::Relation(relation) => builder.add_relation(relation),
                Element::Node(_) => {}
            }
        }
        builder.prepare_nodes();
        for element in elements {
            if let Element::Node(node) = element {
                builder.add_node(node);
            }
        }
        builder.build()
    }

    /// Builds a graph from a PBF file, reading it twice: the ways and relations, then the nodes.
    /// `progress` is told the fraction of the work done (0 to 1).
    pub fn from_pbf(path: &Path, mut progress: impl FnMut(f64)) -> Result<(Graph, BuildStats), BuildError> {
        let size = std::fs::metadata(path)?.len().max(1) as f64;
        let mut builder = GraphBuilder::new();
        // The first pass reads the ways and relations (45 % of the work), the second the nodes.
        for (pass, filter) in
            [Filter { nodes: false, ways: true, relations: true }, Filter { nodes: true, ways: false, relations: false }].into_iter().enumerate()
        {
            let read = Rc::new(Cell::new(0u64));
            let file = Counting { inner: BufReader::new(File::open(path)?), read: Rc::clone(&read) };
            let mut pbf = Pbf::new(file, filter);
            let mut reported = 0.0;
            while let Some(block) = pbf.next_block() {
                for element in block? {
                    match element {
                        Element::Way(way) => builder.add_way(&way),
                        Element::Relation(relation) => builder.add_relation(&relation),
                        Element::Node(node) => builder.add_node(&node),
                    }
                }
                let fraction = 0.45 * pass as f64 + 0.45 * (read.get() as f64 / size).min(1.0);
                if fraction - reported >= 0.01 {
                    reported = fraction;
                    progress(fraction);
                }
            }
            if pass == 0 {
                builder.prepare_nodes();
            }
        }
        let result = builder.build();
        progress(1.0);
        result
    }

    fn intern(&mut self, text: Option<&str>) -> u32 {
        let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Graph::NONE };
        if let Some(&index) = self.string_index.get(text) {
            return index;
        }
        let index = self.strings.len() as u32;
        self.strings.push(text.to_string());
        self.string_index.insert(text.to_string(), index);
        index
    }

    /// The modes allowed along and against a way of `highway` with `tags`.
    fn access(highway: Highway, tags: &Tags) -> (u16, u16) {
        let (mut forward, mut backward) = (0, 0);
        for mode in [TravelMode::Car, TravelMode::Bicycle, TravelMode::Foot] {
            let (f, b) = Profile::new(mode).access(highway, tags);
            if f {
                forward |= Edge::mode_flag(mode);
            }
            if b {
                backward |= Edge::mode_flag(mode);
            }
        }
        (forward, backward)
    }

    pub fn add_way(&mut self, way: &Way) {
        let Some(highway) = Highway::of(&way.tags) else { return };
        if way.nodes.len() < 2 || way.tags.has("area", "yes") {
            return;
        }
        let tags = &way.tags;
        let (forward, backward) = Self::access(highway, tags);
        if forward == 0 && backward == 0 {
            return;
        }
        let mut flags = Profile::way_flags(highway, tags);
        if tags.has("toll", "yes") {
            flags |= WayInfo::TOLL;
        }
        if matches!(tags.get("junction"), Some("roundabout" | "circular")) {
            flags |= WayInfo::ROUNDABOUT;
        }
        // The tags of one direction: `key:forward` and `key:backward`, or `key` itself for the
        // direction a road one-way for motor traffic goes (the wiki defines them so; walkers, and
        // often cyclists, may use such a road both ways).
        let only_forward = backward & Edge::CAR == 0 && forward & Edge::CAR != 0 || backward == 0;
        let only_backward = forward & Edge::CAR == 0 && backward & Edge::CAR != 0 || forward == 0;
        let directed = |key: &str| -> (Option<String>, Option<String>) {
            let along = tags.get(&format!("{key}:forward")).map(str::to_string);
            let against = tags.get(&format!("{key}:backward")).map(str::to_string);
            let plain = tags.get(key).map(str::to_string);
            match (only_forward, only_backward) {
                (true, _) => (along.or(plain), None),
                (_, true) => (None, against.or(plain)),
                _ => (along, against),
            }
        };
        let (turn_forward, turn_backward) = directed("turn:lanes");
        let destination = |direction: &str| -> Option<String> {
            let get = |key: &str| {
                if direction.is_empty() { tags.get(key).map(str::to_string) } else { tags.get(&format!("{key}:{direction}")).map(str::to_string) }
            };
            let parts: Vec<String> = [get("destination:ref"), get("destination")].into_iter().flatten().filter(|p| !p.trim().is_empty()).collect();
            (!parts.is_empty()).then(|| parts.join(";"))
        };
        let (destination_forward, destination_backward) = match (only_forward, only_backward) {
            (true, _) => (destination("forward").or(destination("")), None),
            (_, true) => (None, destination("backward").or(destination(""))),
            _ => (destination("forward"), destination("backward")),
        };
        let lanes_total: Option<u8> = tags.get("lanes").and_then(|l| l.trim().parse().ok());
        let lanes_of = |key: &str| tags.get(key).and_then(|l| l.trim().parse::<u8>().ok());
        let (lanes, lanes_backward) = match (only_forward, only_backward) {
            (true, _) => (lanes_of("lanes:forward").or(lanes_total).unwrap_or(0), 0),
            (_, true) => (0, lanes_of("lanes:backward").or(lanes_total).unwrap_or(0)),
            _ => (
                lanes_of("lanes:forward").or(lanes_total.map(|l| l.div_ceil(2))).unwrap_or(0),
                lanes_of("lanes:backward").or(lanes_total.map(|l| l.div_ceil(2))).unwrap_or(0),
            ),
        };
        let maxspeed = Profile::parse_maxspeed(tags.get("maxspeed"));
        let maxspeed_forward = Profile::parse_maxspeed(tags.get("maxspeed:forward"));
        let maxspeed_backward = Profile::parse_maxspeed(tags.get("maxspeed:backward"));
        let mut info = WayInfo::none(way.id, highway);
        info.maxspeed_kmh = if maxspeed_forward > 0 { maxspeed_forward } else { maxspeed };
        info.maxspeed_backward_kmh = if maxspeed_backward > 0 { maxspeed_backward } else { 0 };
        info.lanes = lanes;
        info.lanes_backward = lanes_backward;
        info.incline_percent = Profile::parse_incline(tags.get("incline")).map_or(0, |p| p.round().clamp(-100.0, 100.0) as i8);
        info.flags = flags;
        info.name = self.intern(tags.get("name"));
        info.road_ref = self.intern(tags.get("ref"));
        info.turn_lanes = self.intern(turn_forward.as_deref());
        info.turn_lanes_backward = self.intern(turn_backward.as_deref());
        info.destination = self.intern(destination_forward.as_deref());
        info.destination_backward = self.intern(destination_backward.as_deref());
        let duration_s = if highway == Highway::Ferry { Profile::parse_duration(tags.get("duration")) } else { None };
        self.ways.push(WayRecord { info, nodes: way.nodes.clone(), forward, backward, duration_s });
    }

    pub fn add_relation(&mut self, relation: &Relation) {
        let tags = &relation.tags;
        if !matches!(tags.get("type"), Some("restriction" | "restriction:motorcar")) {
            return;
        }
        let kind = tags.get("restriction").or(tags.get("restriction:motorcar")).unwrap_or("");
        let only = if kind.starts_with("only_") {
            true
        } else if kind.starts_with("no_") {
            false
        } else {
            return;
        };
        if tags.get("except").is_some_and(|e| e.split(';').any(|x| x.trim() == "motorcar")) {
            return;
        }
        let member = |role: &str, kind: MemberKind| -> Option<i64> {
            let mut matching = relation.members.iter().filter(|m| m.role == role && m.kind == kind);
            let first = matching.next()?;
            // Several `from` or `to` ways (allowed for `no_entry`/`no_exit`) are set aside.
            matching.next().is_none().then_some(first.id)
        };
        // Restrictions through a way (`via` a way) are set aside: see the developer notes.
        let (Some(from), Some(via), Some(to)) = (member("from", MemberKind::Way), member("via", MemberKind::Node), member("to", MemberKind::Way)) else {
            return;
        };
        self.restrictions.push(RawRestriction { from, via, to, only });
    }

    /// Settles which nodes to remember, once every way is in.
    pub fn prepare_nodes(&mut self) {
        let mut all: Vec<i64> = self.ways.iter().flat_map(|w| w.nodes.iter().copied()).collect();
        all.sort_unstable();
        let mut needed = Vec::new();
        let mut uses = Vec::new();
        for id in all {
            if needed.last() == Some(&id) {
                *uses.last_mut().unwrap_or(&mut 0) += 1;
            } else {
                needed.push(id);
                uses.push(1);
            }
        }
        // Ends of ways are junctions, and so are the nodes of restrictions.
        let mark = |id: i64, needed: &[i64], uses: &mut [u32]| {
            if let Ok(i) = needed.binary_search(&id) {
                uses[i] = uses[i].max(2);
            }
        };
        for way in &self.ways {
            mark(way.nodes[0], &needed, &mut uses);
            mark(*way.nodes.last().unwrap_or(&0), &needed, &mut uses);
            // A way that comes back to one of its own nodes passes it twice: counted above.
        }
        for r in &self.restrictions {
            mark(r.via, &needed, &mut uses);
        }
        self.coordinates = vec![None; needed.len()];
        self.node_flags = vec![0; needed.len()];
        self.node_names.clear();
        self.needed = needed;
        self.uses = uses;
    }

    pub fn add_node(&mut self, node: &Node) {
        let Ok(i) = self.needed.binary_search(&node.id) else { return };
        self.coordinates[i] = Some(Graph::fixed(&node.coordinate));
        let flags = Profile::node_flags(&node.tags);
        if flags != 0 {
            self.node_flags[i] = flags;
            // A node of signals, a barrier, a level crossing…: a node of the graph, so that its
            // delay is counted and it can be told.
            self.uses[i] = self.uses[i].max(2);
        }
        let named = flags & (NodeFlag::SIGNALS | NodeFlag::MOTORWAY_JUNCTION) != 0 || node.tags.has("junction", "yes");
        if named {
            let mut has_name = false;
            for (key, kind) in [("name", NodeNameKind::Name), ("junction:name", NodeNameKind::Name), ("ref", NodeNameKind::Ref)] {
                if kind == NodeNameKind::Ref && flags & NodeFlag::MOTORWAY_JUNCTION == 0 {
                    continue;
                }
                if kind == NodeNameKind::Name && has_name {
                    continue;
                }
                let string = self.intern(node.tags.get(key));
                if string != Graph::NONE {
                    has_name |= kind == NodeNameKind::Name;
                    self.node_names.push((i, kind, string));
                    self.uses[i] = self.uses[i].max(2);
                }
            }
        }
    }

    pub fn build(mut self) -> Result<(Graph, BuildStats), BuildError> {
        if self.needed.is_empty() && !self.ways.is_empty() {
            self.prepare_nodes();
        }
        // The graph id of each needed node that is a junction, numbered as first met.
        let mut graph_id = vec![Graph::NONE; self.needed.len()];
        let mut nodes: Vec<[i32; 2]> = Vec::new();
        let mut shapes: Vec<[i32; 2]> = Vec::new();
        let mut directed: Vec<(u32, Edge)> = Vec::new();
        let mut ways: Vec<WayInfo> = Vec::with_capacity(self.ways.len());
        let mut way_index: HashMap<i64, u32> = HashMap::new();
        for record in &self.ways {
            let way = ways.len() as u32;
            let mut info = record.info;
            // Indices of the way's nodes in `needed`, with a break where a node is missing (out of
            // the extract).
            let indices: Vec<Option<usize>> =
                record.nodes.iter().map(|id| self.needed.binary_search(id).ok().filter(|&i| self.coordinates[i].is_some())).collect();
            let mut segments: Vec<(usize, usize, Vec<[i32; 2]>, f64)> = Vec::new();
            let mut current: Vec<[i32; 2]> = Vec::new();
            let mut start: Option<usize> = None;
            let mut length = 0.0;
            for (position, index) in indices.iter().enumerate() {
                let Some(i) = *index else {
                    current.clear();
                    start = None;
                    length = 0.0;
                    continue;
                };
                let point = self.coordinates[i].unwrap_or([0, 0]);
                if let Some(last) = current.last() {
                    length += Graph::coordinate(*last).distance_m(&Graph::coordinate(point));
                }
                current.push(point);
                if start.is_none() {
                    start = Some(i);
                    continue;
                }
                let is_end = position + 1 == indices.len() || indices[position + 1].is_none();
                if self.uses[i] >= 2 || is_end {
                    segments.push((start.unwrap_or(i), i, std::mem::take(&mut current), length));
                    current.push(point);
                    start = Some(i);
                    length = 0.0;
                }
            }
            if segments.is_empty() {
                continue;
            }
            if let Some(duration) = record.duration_s.filter(|d| *d > 0) {
                let total: f64 = segments.iter().map(|s| s.3).sum();
                info.maxspeed_kmh = (total / duration as f64 * 3.6).round().clamp(1.0, 254.0) as u8;
            }
            for (a, b, points, length) in segments {
                if points.len() < 2 || length < 0.05 && a == b {
                    continue;
                }
                let shape_len = u16::try_from(points.len()).map_err(|_| BuildError::TooLarge)?;
                let shape_start = u32::try_from(shapes.len()).map_err(|_| BuildError::TooLarge)?;
                shapes.extend_from_slice(&points);
                let mut id = |i: usize, nodes: &mut Vec<[i32; 2]>| -> u32 {
                    if graph_id[i] == Graph::NONE {
                        graph_id[i] = nodes.len() as u32;
                        nodes.push(self.coordinates[i].unwrap_or([0, 0]));
                    }
                    graph_id[i]
                };
                let (from, to) = (id(a, &mut nodes), id(b, &mut nodes));
                let length_dm = (length * 10.0).round().clamp(1.0, u32::MAX as f64) as u32;
                if record.forward != 0 {
                    directed.push((from, Edge { target: to, way, length_dm, shape_start, shape_len, flags: record.forward }));
                }
                if record.backward != 0 {
                    directed.push((to, Edge { target: from, way, length_dm, shape_start, shape_len, flags: record.backward | Edge::REVERSED }));
                }
            }
            way_index.insert(record.info.osm_id, way);
            ways.push(info);
        }
        if directed.is_empty() {
            return Err(BuildError::Empty);
        }
        u32::try_from(directed.len()).map_err(|_| BuildError::TooLarge)?;
        directed.sort_by_key(|(source, _)| *source);
        let mut first_edge = vec![0u32; nodes.len() + 1];
        for (source, _) in &directed {
            first_edge[*source as usize + 1] += 1;
        }
        for i in 0..nodes.len() {
            first_edge[i + 1] += first_edge[i];
        }
        let edges: Vec<Edge> = directed.into_iter().map(|(_, e)| e).collect();

        let mut restrictions: Vec<Restriction> = self
            .restrictions
            .iter()
            .filter_map(|r| {
                let via = graph_id[self.needed.binary_search(&r.via).ok()?];
                (via != Graph::NONE).then_some(())?;
                Some(Restriction { via, from_way: *way_index.get(&r.from)?, to_way: *way_index.get(&r.to)?, only: r.only })
            })
            .collect();
        restrictions.sort();
        restrictions.dedup();

        let mut string_offsets = Vec::with_capacity(self.strings.len() + 1);
        let mut text = Vec::new();
        string_offsets.push(0u32);
        for s in &self.strings {
            text.extend_from_slice(s.as_bytes());
            string_offsets.push(u32::try_from(text.len()).map_err(|_| BuildError::TooLarge)?);
        }
        let coordinates: Vec<Coordinate> = nodes.iter().map(|p| Graph::coordinate(*p)).collect();
        let bounds = Bounds::of(&coordinates).ok_or(BuildError::Empty)?;
        let mut node_flags = vec![0u16; nodes.len()];
        for (i, flags) in self.node_flags.iter().enumerate() {
            if *flags != 0 && graph_id[i] != Graph::NONE {
                node_flags[graph_id[i] as usize] = *flags;
            }
        }
        let mut node_names: Vec<NodeName> = self
            .node_names
            .iter()
            .filter(|(i, _, _)| graph_id[*i] != Graph::NONE)
            .map(|&(i, kind, string)| NodeName { node: graph_id[i], kind, string })
            .collect();
        node_names.sort();
        node_names.dedup_by(|a, b| (a.node, a.kind) == (b.node, b.kind));
        let stats = BuildStats { ways: ways.len(), nodes: nodes.len(), edges: edges.len(), restrictions: restrictions.len() };
        let mut graph = Graph {
            nodes,
            first_edge,
            edges,
            shapes,
            ways,
            string_offsets,
            text,
            restrictions,
            node_flags,
            node_names,
            elevations: Vec::new(),
            bounds,
            sources: Vec::new(),
            twins: Vec::new(),
        };
        graph.compute_sources();
        debug_assert!(graph.validate().is_ok());
        Ok((graph, stats))
    }
}

/// A reader that counts the bytes read through it, for the progress of a build.
struct Counting<R> {
    inner: R,
    read: Rc<Cell<u64>>,
}

impl<R: io::Read> io::Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.set(self.read.get() + n as u64);
        Ok(n)
    }
}
