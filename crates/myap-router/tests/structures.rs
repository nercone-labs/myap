//! The parts the search is built of, checked against what they are defined to be.

use geo::{Bounds, Coordinate, Line};
use myap_router::{EdgeSet, Graph, GraphBuilder, Network, SpatialIndex, TrafficMap};
use osm::{Element, Node, Tags, Way};

fn tags(pairs: &[(&str, &str)]) -> Tags {
    Tags(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

/// A grid of `n`×`n` junctions 0.001° apart, two-way residential streets, every third row
/// one-way.
fn grid(n: i64) -> Graph {
    let id = |r: i64, c: i64| r * 1000 + c + 1;
    let mut elements = Vec::new();
    for r in 0..n {
        for c in 0..n {
            elements.push(Element::Node(Node {
                id: id(r, c),
                coordinate: Coordinate::new(35.0 + r as f64 * 0.001, 139.0 + c as f64 * 0.001),
                tags: Tags::default(),
            }));
        }
    }
    for line in 0..n {
        let row: Vec<i64> = (0..n).map(|c| id(line, c)).collect();
        let column: Vec<i64> = (0..n).map(|r| id(r, line)).collect();
        let row_tags: &[(&str, &str)] = if line % 3 == 1 { &[("highway", "residential"), ("oneway", "yes")] } else { &[("highway", "residential")] };
        elements.push(Element::Way(Way { id: 10_000 + line, nodes: row, tags: tags(row_tags) }));
        elements.push(Element::Way(Way { id: 20_000 + line, nodes: column, tags: tags(&[("highway", "residential")]) }));
    }
    GraphBuilder::from_elements(&elements).unwrap().0
}

#[test]
fn edge_set_is_a_set() {
    let mut set = EdgeSet::with_bound(10);
    assert!(set.is_empty());
    assert!(set.insert(3));
    assert!(!set.insert(3));
    assert!(set.insert(200)); // beyond the bound: the set grows
    assert!(set.insert(63));
    assert!(set.insert(64));
    assert_eq!(set.len(), 4);
    assert!(set.contains(3) && set.contains(63) && set.contains(64) && set.contains(200));
    assert!(!set.contains(4) && !set.contains(199) && !set.contains(u32::MAX));
    assert_eq!(set.iter().collect::<Vec<_>>(), vec![3, 63, 64, 200]);
    let collected: EdgeSet = [5, 1, 5].into_iter().collect();
    assert_eq!(collected.iter().collect::<Vec<_>>(), vec![1, 5]);
}

#[test]
fn twins_are_the_same_road_the_other_way() {
    let graph = grid(5);
    let mut with_twin = 0;
    for e in 0..graph.edges.len() as u32 {
        let edge = &graph.edges[e as usize];
        if let Some(t) = graph.twin(e) {
            with_twin += 1;
            let twin = &graph.edges[t as usize];
            assert_eq!(graph.twin(t), Some(e));
            assert_eq!(twin.target, graph.source(e));
            assert_eq!(graph.source(t), edge.target);
            assert_eq!(twin.way, edge.way);
            assert_ne!(twin.is_reversed(), edge.is_reversed());
            assert_eq!(graph.canonical(e), graph.canonical(t));
            let mut back = graph.edge_points(t);
            back.reverse();
            assert_eq!(back, graph.edge_points(e));
        } else {
            // Only the one-way rows have no twin.
            assert!(graph.ways[edge.way as usize].osm_id >= 10_000 && graph.ways[edge.way as usize].osm_id < 20_000);
            assert_eq!(graph.canonical(e), e);
        }
    }
    assert!(with_twin > 0);
}

#[test]
fn chord_never_exceeds_the_distance_on_the_earth() {
    // The estimate of the search must be a lower bound of every real distance.
    let graph = grid(4);
    let network = Network::new(&graph);
    let targets = [
        Coordinate::new(35.0, 139.0),
        Coordinate::new(35.0015, 139.0025),
        Coordinate::new(-33.9, 151.2),
        Coordinate::new(35.003, 139.003),
        Coordinate::new(89.0, -179.0),
    ];
    for node in 0..graph.nodes.len() as u32 {
        let c = graph.node_coordinate(node);
        for t in &targets {
            let chord = network.chord_m(node, &Network::point_of(t)) as f64;
            let real = c.distance_m(t);
            assert!(chord <= real, "chord {chord} > distance {real}");
            // And close to it at the distances of a region (within the metre taken off and the
            // error of f32).
            if real < 50_000.0 {
                assert!(real - chord < 5.0, "chord {chord} far below {real}");
            }
        }
    }
}

#[test]
fn turn_angles_agree_with_bearings() {
    let graph = grid(4);
    let network = Network::new(&graph);
    for from in 0..graph.edges.len() as u32 {
        for to in graph.out_edges(graph.edges[from as usize].target) {
            let expected = Coordinate::bearing_difference(network.arriving(from), network.leaving(to));
            let angle = network.turn_angle(from, to) as f64;
            assert!((angle - expected).abs() < 0.05 || (angle.abs() - 180.0).abs() < 0.05 && (expected.abs() - 180.0).abs() < 0.05, "{angle} vs {expected}");
        }
    }
}

#[test]
fn spatial_index_finds_every_edge_near_a_point() {
    let graph = grid(6);
    let index = SpatialIndex::new(&graph);
    let point = Coordinate::new(35.0025, 139.0025);
    let near: EdgeSet = index.near(&point, 1).collect();
    let within: EdgeSet = index.within(&Bounds { south: 35.001, west: 139.001, north: 35.004, east: 139.004 }).collect();
    for e in 0..graph.edges.len() as u32 {
        // The index holds one direction of each road.
        let e = graph.canonical(e);
        let line = Line::new(graph.edge_points(e));
        if line.project(&point, 0.0, f64::INFINITY).unwrap().distance_m < 300.0 {
            assert!(near.contains(e) || graph.twin(e).is_some_and(|t| near.contains(t)), "edge {e} near the point is missing");
        }
        if graph.edge_points(e).iter().any(|p| p.latitude > 35.0011 && p.latitude < 35.0039 && p.longitude > 139.0011 && p.longitude < 139.0039) {
            assert!(within.contains(e) || graph.twin(e).is_some_and(|t| within.contains(t)), "edge {e} within the bounds is missing");
        }
    }
    assert_eq!(index.within(&Bounds { south: 10.0, west: 10.0, north: 10.01, east: 10.01 }).count(), 0);
}

#[test]
fn edge_meets_is_the_box_of_its_points() {
    let graph = grid(3);
    for e in 0..graph.edges.len() as u32 {
        let b = Bounds::of(&graph.edge_points(e)).unwrap();
        assert!(graph.edge_meets(e, &b));
        assert!(graph.edge_meets(e, &Bounds { south: b.north, west: b.east, north: b.north + 1.0, east: b.east + 1.0 }));
        assert!(!graph.edge_meets(e, &Bounds { south: b.north + 0.0001, west: b.west, north: b.north + 1.0, east: b.east }));
        assert!(!graph.edge_meets(e, &Bounds { south: b.south, west: b.east + 0.0001, north: b.north, east: b.east + 1.0 }));
    }
}

#[test]
fn traffic_map_counts_and_closures() {
    let mut map = TrafficMap::default();
    assert!(map.is_empty() && !map.has_closures());
    assert_eq!(map.factor(7), None);
    map.set(7, 0.5);
    map.set(2, 0.0);
    assert_eq!(map.len(), 2);
    assert_eq!(map.factor(7), Some(0.5));
    assert!(map.is_closed(2) && !map.is_closed(7) && !map.is_closed(100));
    assert!(map.has_closures());
    map.set(2, 1.0);
    assert!(!map.has_closures());
    assert_eq!(map.len(), 2);
    map.set(7, 9.0);
    assert_eq!(map.factor(7), Some(1.5));
    map.set(7, f64::NAN);
    assert!(map.is_closed(7));
    let mut other = TrafficMap::default();
    other.set(2, 1.0);
    other.set(7, 0.0);
    assert_eq!(map, other);
}

#[test]
fn hostile_counts_are_refused_without_allocating_them() {
    let graph = grid(3);
    let mut bytes = Vec::new();
    graph.write(&mut bytes).unwrap();
    // The counts follow the magic (8), the version (4) and the bounds (32). Each set to the
    // largest value: an error (never a crash, nor an allocation of that size).
    for count in 0..9 {
        let mut b = bytes.clone();
        let at = 44 + count * 4;
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Graph::read(&mut b.as_slice()).is_err(), "count {count}");
    }
    // A byte too many is refused too.
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(Graph::read(&mut longer.as_slice()).is_err());
    assert_eq!(Graph::read(&mut bytes.as_slice()).unwrap(), graph);
}
