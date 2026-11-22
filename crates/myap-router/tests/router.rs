//! The router on small networks built here, where the right answer is known.

use geo::Coordinate;
use myap_router::{Graph, GraphBuilder, Router};
use osm::{Element, Member, MemberKind, Node, Relation, Tags, Way};
use routing::{ManeuverKind, Modifier, RouteError, RouteRequest, TravelMode};

fn tags(pairs: &[(&str, &str)]) -> Tags {
    Tags(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

/// Node `id` at grid position (row, column), rows going north, 0.001° apart.
fn node(id: i64, row: i32, column: i32) -> Element {
    Element::Node(Node { id, coordinate: Coordinate::new(35.0 + row as f64 * 0.001, 139.0 + column as f64 * 0.001), tags: Tags::default() })
}

fn way(id: i64, nodes: &[i64], pairs: &[(&str, &str)]) -> Element {
    Element::Way(Way { id, nodes: nodes.to_vec(), tags: tags(pairs) })
}

fn restriction(id: i64, from: i64, via: i64, to: i64, kind: &str) -> Element {
    Element::Relation(Relation {
        id,
        members: vec![
            Member { kind: MemberKind::Way, id: from, role: "from".into() },
            Member { kind: MemberKind::Node, id: via, role: "via".into() },
            Member { kind: MemberKind::Way, id: to, role: "to".into() },
        ],
        tags: tags(&[("type", "restriction"), ("restriction", kind)]),
    })
}

/// A 3×3 grid of nodes 1..9 (row-major from the south-west), with residential streets along
/// every row and column.
///
/// ```text
/// 7 - 8 - 9
/// |   |   |
/// 4 - 5 - 6
/// |   |   |
/// 1 - 2 - 3
/// ```
fn grid(extra: Vec<Element>, overrides: &[(i64, &[(&str, &str)])]) -> Router {
    let mut elements = Vec::new();
    for r in 0..3 {
        for c in 0..3 {
            elements.push(node((r * 3 + c + 1) as i64, r, c));
        }
    }
    let lines: [(i64, [i64; 3]); 6] = [(101, [1, 2, 3]), (102, [4, 5, 6]), (103, [7, 8, 9]), (201, [1, 4, 7]), (202, [2, 5, 8]), (203, [3, 6, 9])];
    for (id, nodes) in lines {
        let base: Vec<(&str, &str)> = vec![("highway", "residential")];
        let pairs = overrides.iter().find(|(w, _)| *w == id).map_or(base, |(_, p)| p.to_vec());
        let name = format!("Street {id}");
        let mut pairs: Vec<(&str, &str)> = pairs;
        pairs.push(("name", &name));
        elements.push(way(id, &nodes, &pairs));
    }
    elements.extend(extra);
    let (graph, _) = GraphBuilder::from_elements(&elements).unwrap();
    Router::new(graph)
}

fn at(row: f64, column: f64) -> Coordinate {
    Coordinate::new(35.0 + row * 0.001, 139.0 + column * 0.001)
}

fn route(router: &mut Router, from: Coordinate, to: Coordinate, mode: TravelMode) -> Result<routing::Route, RouteError> {
    router.route(&RouteRequest::new(vec![from, to], mode)).map(|mut r| r.remove(0))
}

#[test]
fn shortest_path_on_a_grid() {
    let mut router = grid(vec![], &[]);
    let r = route(&mut router, at(0.0, 0.0), at(2.0, 2.0), TravelMode::Car).unwrap();
    let manhattan = at(0.0, 0.0).distance_m(&at(2.0, 0.0)) + at(2.0, 0.0).distance_m(&at(2.0, 2.0));
    assert!((r.distance_m - manhattan).abs() < 5.0, "{} vs {manhattan}", r.distance_m);
    assert!(r.validate().is_ok());
    let steps: Vec<_> = r.steps().collect();
    assert_eq!(steps.first().unwrap().maneuver.kind, ManeuverKind::Depart);
    assert_eq!(steps.last().unwrap().maneuver.kind, ManeuverKind::Arrive);
    // Turn costs make one turn better than several.
    let turns = steps.iter().filter(|s| s.maneuver.kind == ManeuverKind::Turn).count();
    assert_eq!(turns, 1);
    assert!(r.geometry.first().unwrap().distance_m(&at(0.0, 0.0)) < 1.0);
    assert!(r.geometry.last().unwrap().distance_m(&at(2.0, 2.0)) < 1.0);
}

#[test]
fn starts_and_ends_in_the_middle_of_roads() {
    let mut router = grid(vec![], &[]);
    // Along the bottom street, both points between nodes 1 and 2.
    let r = route(&mut router, at(0.0, 0.2), at(0.0, 0.8), TravelMode::Foot).unwrap();
    let expected = at(0.0, 0.2).distance_m(&at(0.0, 0.8));
    assert!((r.distance_m - expected).abs() < 1.0, "{}", r.distance_m);
    // A point a little off the road is snapped onto it.
    let r = route(&mut router, at(0.05, 0.5), at(1.0, 1.5), TravelMode::Foot).unwrap();
    assert!(r.geometry[0].distance_m(&at(0.0, 0.5)) < 1.0);
    // Backwards along one road: the walker turns round where they are.
    let r = route(&mut router, at(0.0, 0.8), at(0.0, 0.2), TravelMode::Foot).unwrap();
    assert!((r.distance_m - expected).abs() < 1.0);
}

#[test]
fn oneway_streets() {
    // The bottom street is one-way westwards (against its node order).
    let mut router = grid(vec![], &[(101, &[("highway", "residential"), ("oneway", "-1")])]);
    let car = route(&mut router, at(0.0, 0.0), at(0.0, 2.0), TravelMode::Car).unwrap();
    let direct = at(0.0, 0.0).distance_m(&at(0.0, 2.0));
    assert!(car.distance_m > direct + 100.0, "a car may not drive east on it");
    let back = route(&mut router, at(0.0, 2.0), at(0.0, 0.0), TravelMode::Car).unwrap();
    assert!((back.distance_m - direct).abs() < 5.0);
    let walk = route(&mut router, at(0.0, 0.0), at(0.0, 2.0), TravelMode::Foot).unwrap();
    assert!((walk.distance_m - direct).abs() < 5.0, "one-way streets do not bind walkers");
    // A bicycle may use it both ways when the tags say so.
    let mut router = grid(vec![], &[(101, &[("highway", "residential"), ("oneway", "-1"), ("oneway:bicycle", "no")])]);
    let bike = route(&mut router, at(0.0, 0.0), at(0.0, 2.0), TravelMode::Bicycle).unwrap();
    assert!((bike.distance_m - direct).abs() < 5.0);
}

#[test]
fn access_and_classes() {
    // The middle column is private, the middle row a footway.
    let mut router = grid(vec![], &[(202, &[("highway", "residential"), ("access", "private")]), (102, &[("highway", "footway")])]);
    let car = route(&mut router, at(0.0, 1.0), at(2.0, 1.0), TravelMode::Car).unwrap();
    assert!(car.distance_m > 400.0, "{}", car.distance_m);
    let foot_private = route(&mut router, at(1.0, 0.0), at(1.0, 2.0), TravelMode::Foot).unwrap();
    assert!((foot_private.distance_m - at(1.0, 0.0).distance_m(&at(1.0, 2.0))).abs() < 5.0);
    // Motorways are closed to walkers and cyclists.
    let mut router = grid(vec![], &[(101, &[("highway", "motorway"), ("oneway", "no")])]);
    let walk = route(&mut router, at(0.0, 0.0), at(0.0, 2.0), TravelMode::Foot).unwrap();
    assert!(walk.distance_m > 400.0);
}

#[test]
fn turn_restrictions() {
    // From the bottom street (101) at node 2, no left turn into the middle column (202).
    let no_left = restriction(1, 101, 2, 202, "no_left_turn");
    let mut router = grid(vec![no_left], &[]);
    let start = at(0.0, 0.5);
    let r = route(&mut router, start, at(1.5, 1.0), TravelMode::Car).unwrap();
    // The car may not go 1-2-5; it goes round by 3-6-5 or by 1-4-5.
    let via_2_then_5 =
        r.geometry.windows(2).any(|w| w[0].distance_m(&at(0.0, 1.0)) < 1.0 && w[1].latitude > 35.0001 && (w[1].longitude - 139.001).abs() < 1e-6);
    assert!(!via_2_then_5);
    // Walking is not bound by it.
    let walk = route(&mut router, start, at(1.5, 1.0), TravelMode::Foot).unwrap();
    assert!(walk.distance_m < r.distance_m);

    // Only straight on: from 101 at node 2 a car must keep to 101.
    let only = restriction(2, 101, 2, 101, "only_straight_on");
    let mut router = grid(vec![only], &[]);
    let r = route(&mut router, at(0.0, 0.5), at(2.0, 1.0), TravelMode::Car).unwrap();
    assert!(r.distance_m > at(0.0, 0.5).distance_m(&at(0.0, 1.0)) + at(0.0, 1.0).distance_m(&at(2.0, 1.0)) + 50.0);
}

#[test]
fn avoiding_tolls() {
    // The bottom street is a toll road; going round is longer.
    let mut router = grid(vec![], &[(101, &[("highway", "residential"), ("toll", "yes")])]);
    let mut request = RouteRequest::new(vec![at(0.0, 0.0), at(0.0, 2.0)], TravelMode::Car);
    let fastest = router.route(&request).unwrap().remove(0);
    assert!(fastest.has_tolls);
    request.avoid.tolls = true;
    let avoiding = router.route(&request).unwrap().remove(0);
    assert!(!avoiding.has_tolls);
    assert!(avoiding.distance_m > fastest.distance_m);
    assert!(!avoiding.ignored_avoid.tolls);
    // When every way has tolls, the route takes them and says so.
    let all: Vec<(i64, &[(&str, &str)])> = [101, 102, 103, 201, 202, 203].iter().map(|&w| (w, &[("highway", "residential"), ("toll", "yes")][..])).collect();
    let mut router = grid(vec![], &all);
    let r = router.route(&request).unwrap().remove(0);
    assert!(r.ignored_avoid.tolls);
}

#[test]
fn errors() {
    let mut router = grid(vec![], &[]);
    // Far from any road, but within the area of the graph's coverage margin.
    assert_eq!(route(&mut router, at(0.0, 0.0), at(10.0, 10.0), TravelMode::Car).unwrap_err(), RouteError::NoRoad);
    assert_eq!(route(&mut router, at(0.0, 0.0), Coordinate::new(36.0, 140.0), TravelMode::Car).unwrap_err(), RouteError::OutOfCoverage);
    assert_eq!(route(&mut router, at(0.0, 0.0), at(2.0, 2.0), TravelMode::Transit).unwrap_err(), RouteError::UnsupportedMode);
    // Two separate networks.
    let elements = vec![
        node(1, 0, 0),
        node(2, 0, 1),
        node(3, 5, 0),
        node(4, 5, 1),
        way(1, &[1, 2], &[("highway", "residential")]),
        way(2, &[3, 4], &[("highway", "residential")]),
    ];
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    assert_eq!(route(&mut router, at(0.0, 0.5), at(5.0, 0.5), TravelMode::Car).unwrap_err(), RouteError::NoRoute);
}

#[test]
fn roundabout_exits() {
    // A roundabout of four nodes (10 north, 11 east, 12 south, 13 west), entered from the south
    // (way from node 20) and left to the north (way to node 21); east and west have roads too.
    let mut elements = vec![node(10, 1, 0), node(11, 0, 1), node(12, -1, 0), node(13, 0, -1), node(20, -3, 0), node(21, 3, 0), node(22, 0, 3), node(23, 0, -3)];
    elements.push(way(1, &[12, 11, 10, 13, 12], &[("highway", "primary"), ("junction", "roundabout")]));
    elements.push(way(2, &[20, 12], &[("highway", "primary"), ("name", "South Road")]));
    elements.push(way(3, &[10, 21], &[("highway", "primary"), ("name", "North Road")]));
    elements.push(way(4, &[11, 22], &[("highway", "primary"), ("name", "East Road")]));
    elements.push(way(5, &[13, 23], &[("highway", "primary"), ("name", "West Road")]));
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    let r = route(&mut router, at(-2.5, 0.0), at(2.5, 0.0), TravelMode::Car).unwrap();
    let roundabout = r.steps().find(|s| s.maneuver.kind == ManeuverKind::Roundabout).expect("a roundabout step");
    // Anticlockwise (the order of the way): east is the first exit, north the second.
    assert_eq!(roundabout.maneuver.exit, Some(2));
    assert_eq!(roundabout.name.as_deref(), Some("North Road"));
}

#[test]
fn turn_directions() {
    // From the bottom street eastwards, turn left (north) at node 2.
    let mut router = grid(vec![], &[]);
    let r = route(&mut router, at(0.0, 0.2), at(1.5, 1.0), TravelMode::Car).unwrap();
    let turn = r.steps().find(|s| s.maneuver.kind == ManeuverKind::Turn).unwrap();
    assert_eq!(turn.maneuver.modifier, Some(Modifier::Left));
    assert_eq!(turn.name.as_deref(), Some("Street 202"));
}

#[test]
fn alternatives_differ() {
    let mut router = grid(vec![], &[]);
    let mut request = RouteRequest::new(vec![at(0.0, 0.0), at(2.0, 2.0)], TravelMode::Car);
    request.alternatives = true;
    let routes = router.route(&request).unwrap();
    assert!(routes.len() >= 2);
    assert_ne!(routes[0].geometry, routes[1].geometry);
    assert!(routes[0].duration_s <= routes[1].duration_s);
}

#[test]
fn via_waypoints_make_legs() {
    let mut router = grid(vec![], &[]);
    let request = RouteRequest::new(vec![at(0.0, 0.0), at(0.0, 2.0), at(2.0, 2.0)], TravelMode::Car);
    let r = router.route(&request).unwrap().remove(0);
    assert_eq!(r.legs.len(), 2);
    assert_eq!(r.legs[0].steps.last().unwrap().maneuver.kind, ManeuverKind::Arrive);
    assert!(r.validate().is_ok());
    let total: f64 = r.legs.iter().map(|l| l.distance_m).sum();
    assert!((total - r.distance_m).abs() < 1e-6);
}

#[test]
fn graph_file_round_trip_and_damage() {
    let router = grid(vec![restriction(1, 101, 2, 202, "no_left_turn")], &[]);
    let graph = router.graph().clone();
    let mut bytes = Vec::new();
    graph.write(&mut bytes).unwrap();
    let back = Graph::read(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, graph);
    // Truncated, wrong magic, an edge to a node that does not exist: errors, never panics.
    assert!(Graph::read(&mut &bytes[..bytes.len() - 1]).is_err());
    let mut wrong = bytes.clone();
    wrong[0] = b'X';
    assert!(Graph::read(&mut wrong.as_slice()).is_err());
    let mut bad = graph.clone();
    bad.edges[0].target = 9999;
    let mut bad_bytes = Vec::new();
    bad.write(&mut bad_bytes).unwrap();
    assert!(Graph::read(&mut bad_bytes.as_slice()).is_err());
    // Every single-byte change of the header is either read or refused.
    for i in 0..64.min(bytes.len()) {
        let mut b = bytes.clone();
        b[i] ^= 0xff;
        let _ = Graph::read(&mut b.as_slice());
    }
}

#[test]
fn ferry_speed_from_duration() {
    let elements = vec![
        node(1, 0, 0),
        // About 9.1 km.
        node(2, 0, 100),
        way(1, &[1, 2], &[("route", "ferry"), ("duration", "00:30"), ("motorcar", "yes")]),
    ];
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    let r = route(&mut router, at(0.0, 0.0), at(0.0, 100.0), TravelMode::Car).unwrap();
    // The speed is kept in whole km/h: within 3 % of the half hour.
    assert!((r.duration_s - 1800.0).abs() < 54.0, "{}", r.duration_s);
    assert!(r.has_ferries);
}

/// The search's estimate of the time left must not exceed any real time, ferries included (found
/// by the independent review).
#[test]
fn a_ferry_faster_than_walking_is_found_first() {
    // O south of A; A–B ferry; land detour O–D–B.
    let json = r#"{"elements":[
      {"type":"node","id":1,"lat":34.995,"lon":139.0},
      {"type":"node","id":2,"lat":35.0,"lon":139.0},
      {"type":"node","id":3,"lat":35.0,"lon":139.02},
      {"type":"node","id":4,"lat":34.997,"lon":139.01},
      {"type":"way","id":10,"nodes":[1,2],"tags":{"highway":"footway"}},
      {"type":"way","id":11,"nodes":[2,3],"tags":{"route":"ferry"}},
      {"type":"way","id":12,"nodes":[1,4,3],"tags":{"highway":"footway"}}
    ]}"#;
    let elements = osm::Overpass::parse(json.as_bytes()).unwrap();
    let (graph, _) = GraphBuilder::from_elements(&elements).unwrap();
    let mut router = Router::new(graph);
    let mut request = RouteRequest::new(vec![Coordinate::new(34.995, 139.0), Coordinate::new(35.0, 139.02)], TravelMode::Foot);
    request.alternatives = false;
    let routes = router.route(&request).unwrap();
    assert!(routes[0].has_ferries, "the search (estimating at 5 km/h, with a ferry at 20 km/h) returned the slower land route: {:.0} s", routes[0].duration_s);
}

#[test]
fn a_no_u_turn_restriction_does_not_forbid_going_straight_on() {
    // From the bottom street at node 2, no U-turn: going on east along the same street is allowed.
    let mut router = grid(vec![restriction(1, 101, 2, 101, "no_u_turn")], &[]);
    let r = route(&mut router, at(0.0, 0.0), at(0.0, 2.0), TravelMode::Car).unwrap();
    assert!((r.distance_m - at(0.0, 0.0).distance_m(&at(0.0, 2.0))).abs() < 5.0, "{}", r.distance_m);
}

#[test]
fn a_shape_index_that_wraps_is_refused() {
    let router = grid(vec![], &[]);
    let mut bad = router.graph().clone();
    bad.edges[0].shape_start = u32::MAX;
    bad.edges[0].shape_len = 2;
    assert!(bad.validate().is_err());
}
