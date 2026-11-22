//! Myap Router's avoidances, optimisations, traffic and guidance on small networks where the
//! right answer is known.

use geo::{Coordinate, Polygon};
use myap_router::{Graph, GraphBuilder, NodeFlag, Profile, Router, TrafficMap};
use osm::{Element, Node, Tags, Way};
use routing::{AlertKind, Congestion, DrivingSide, LaneIndication, ManeuverKind, Optimize, RouteRequest, TravelMode};

fn tags(pairs: &[(&str, &str)]) -> Tags {
    Tags(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

fn at(row: f64, column: f64) -> Coordinate {
    Coordinate::new(35.0 + row * 0.001, 139.0 + column * 0.001)
}

fn node(id: i64, row: f64, column: f64, pairs: &[(&str, &str)]) -> Element {
    Element::Node(Node { id, coordinate: at(row, column), tags: tags(pairs) })
}

fn way(id: i64, nodes: &[i64], pairs: &[(&str, &str)]) -> Element {
    Element::Way(Way { id, nodes: nodes.to_vec(), tags: tags(pairs) })
}

/// Two ways from A (node 1, west) to B (node 3, east): the direct road through node 2 along row 0,
/// and a detour through nodes 4 and 5 along row 1.
///
/// ```text
/// 4 ------- 5
/// |         |
/// 1 -- 2 -- 3
/// ```
fn two_ways(direct: &[(&str, &str)], middle: &[(&str, &str)]) -> Router {
    let mut elements = vec![node(1, 0.0, 0.0, &[]), node(2, 0.0, 1.0, middle), node(3, 0.0, 2.0, &[]), node(4, 1.0, 0.0, &[]), node(5, 1.0, 2.0, &[])];
    let mut d: Vec<(&str, &str)> = direct.to_vec();
    if !direct.iter().any(|(k, _)| *k == "highway") {
        d.push(("highway", "residential"));
    }
    elements.push(way(10, &[1, 2, 3], &d));
    elements.push(way(11, &[1, 4, 5, 3], &[("highway", "residential")]));
    Router::new(GraphBuilder::from_elements(&elements).unwrap().0)
}

fn request(mode: TravelMode) -> RouteRequest {
    RouteRequest::new(vec![at(0.0, 0.0), at(0.0, 2.0)], mode)
}

fn direct_m() -> f64 {
    at(0.0, 0.0).distance_m(&at(0.0, 2.0))
}

fn takes_direct(router: &mut Router, request: &RouteRequest) -> bool {
    let r = router.route(request).unwrap().remove(0);
    (r.distance_m - direct_m()).abs() < 5.0
}

#[test]
fn unpaved_steps_and_level_crossings_are_avoided_strictly() {
    let mut router = two_ways(&[("surface", "gravel")], &[]);
    let mut r = request(TravelMode::Car);
    assert!(takes_direct(&mut router, &r));
    r.avoid.unpaved = true;
    assert!(!takes_direct(&mut router, &r));
    // A paved surface is not avoided; a track without a surface is unpaved.
    let mut router = two_ways(&[("surface", "asphalt")], &[]);
    assert!(takes_direct(&mut router, &r));

    let mut router = two_ways(&[("highway", "steps")], &[]);
    let mut walk = request(TravelMode::Foot);
    assert!(takes_direct(&mut router, &walk));
    walk.avoid.steps = true;
    assert!(!takes_direct(&mut router, &walk));

    let mut router = two_ways(&[], &[("railway", "level_crossing")]);
    let mut car = request(TravelMode::Car);
    let crossing = router.route(&car).unwrap().remove(0);
    assert!(crossing.alerts.iter().any(|a| a.kind == AlertKind::LevelCrossing), "the level crossing is told");
    car.avoid.level_crossings = true;
    let around = router.route(&car).unwrap().remove(0);
    assert!(around.distance_m > direct_m() + 100.0);
    assert!(!around.ignored_avoid.level_crossings);
}

#[test]
fn what_cannot_be_avoided_is_used_and_said() {
    // Only one road, unpaved.
    let elements = vec![node(1, 0.0, 0.0, &[]), node(2, 0.0, 2.0, &[]), way(1, &[1, 2], &[("highway", "residential"), ("surface", "dirt")])];
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    let mut r = request(TravelMode::Car);
    r.avoid.unpaved = true;
    r.avoid.tolls = true;
    let route = router.route(&r).unwrap().remove(0);
    assert!(route.ignored_avoid.unpaved);
    // No toll on the way: nothing is said of tolls.
    assert!(!route.ignored_avoid.tolls);
}

#[test]
fn barriers_stop_cars_but_not_walkers() {
    let mut router = two_ways(&[], &[("barrier", "bollard")]);
    assert!(!takes_direct(&mut router, &request(TravelMode::Car)), "a bollard stops cars");
    assert!(takes_direct(&mut router, &request(TravelMode::Bicycle)), "bicycles pass bollards");
    assert!(takes_direct(&mut router, &request(TravelMode::Foot)));
    // Access tags on the barrier decide first.
    let mut router = two_ways(&[], &[("barrier", "bollard"), ("motor_vehicle", "yes")]);
    assert!(takes_direct(&mut router, &request(TravelMode::Car)));
    // A gate opens for cars; a locked one does not.
    let mut router = two_ways(&[], &[("barrier", "gate")]);
    assert!(takes_direct(&mut router, &request(TravelMode::Car)));
    let mut router = two_ways(&[], &[("barrier", "gate"), ("locked", "yes")]);
    assert!(!takes_direct(&mut router, &request(TravelMode::Car)));
    // A stile stops bicycles, not walkers.
    let mut router = two_ways(&[], &[("barrier", "stile")]);
    assert!(!takes_direct(&mut router, &request(TravelMode::Bicycle)));
    assert!(takes_direct(&mut router, &request(TravelMode::Foot)));
}

#[test]
fn narrow_and_unlit_roads_are_avoided_when_it_costs_little() {
    let mut router = two_ways(&[("width", "3")], &[]);
    let mut r = request(TravelMode::Car);
    assert!(takes_direct(&mut router, &r));
    r.avoid.narrow = true;
    assert!(!takes_direct(&mut router, &r), "the detour is about twice as long, but narrow roads cost three times");
    let mut router = two_ways(&[("lit", "no")], &[]);
    let mut walk = request(TravelMode::Foot);
    walk.avoid.unlit = true;
    assert!(!takes_direct(&mut router, &walk));
    // Cars do not care about lights.
    let mut car = request(TravelMode::Car);
    car.avoid.unlit = true;
    assert!(takes_direct(&mut router, &car));
}

/// The two ways with elevations: the direct road climbs a hill of `height` metres at node 2.
fn hill(height: f64) -> Router {
    let mut router = two_ways(&[], &[]);
    let mut graph = router.graph().clone();
    let middle = at(0.0, 1.0);
    graph.set_elevations(|p| Some(if p.distance_m(&middle) < 1.0 { height } else { 0.0 }));
    router = Router::new(graph);
    router
}

#[test]
fn hills_slow_walkers_and_cyclists_and_can_be_avoided() {
    let flat = two_ways(&[], &[]).route(&request(TravelMode::Foot)).unwrap().remove(0);
    let mut hilly = hill(15.0);
    let climbing = hilly.route(&request(TravelMode::Foot)).unwrap().remove(0);
    assert!((climbing.distance_m - direct_m()).abs() < 5.0, "the hill is still shorter for a walker who does not avoid it");
    assert!(climbing.duration_s > flat.duration_s, "Tobler: climbing 15 m over 90 m is slower");
    let (up, down) = climbing.climb_m(1.0).unwrap();
    assert!((up - 15.0).abs() < 1.0 && (down - 15.0).abs() < 1.0, "{up} {down}");
    assert_eq!(climbing.elevations.len(), climbing.geometry.len());
    assert!(climbing.alerts.iter().any(|a| a.kind == AlertKind::Steep));
    let mut avoid = request(TravelMode::Bicycle);
    avoid.avoid.hills = true;
    let around = hilly.route(&avoid).unwrap().remove(0);
    assert!(around.distance_m > direct_m() + 100.0, "a cyclist avoiding hills takes the flat detour");
    // Cars are timed alike on hills.
    let car_flat = two_ways(&[], &[]).route(&request(TravelMode::Car)).unwrap().remove(0);
    let car_hill = hilly.route(&request(TravelMode::Car)).unwrap().remove(0);
    assert!((car_flat.duration_s - car_hill.duration_s).abs() < 1.0);
}

#[test]
fn walking_speed_follows_tobler_and_cycling_valhalla_s_table() {
    let foot = Profile::new(TravelMode::Foot);
    // On the flat, the speed given.
    assert!((foot.graded_time_s(100.0, 4.8, 0.0, 0.0) - 75.0).abs() < 1e-6);
    // Tobler: W = 6 exp(-3.5 |s + 0.05|), scaled to 4.8 km/h on the flat; 10 % uphill.
    let w = |s: f64| 6.0 * (-3.5f64 * (s + 0.05).abs()).exp();
    let expected = 100.0 / (w(0.1) * 4.8 / w(0.0) / 3.6);
    assert!((foot.graded_time_s(100.0, 4.8, 10.0, 0.0) - expected).abs() < 1e-6);
    // Gentle descent is faster than the flat.
    assert!(foot.graded_time_s(100.0, 4.8, 0.0, 5.0) < 75.0);
    assert_eq!(Profile::bicycle_grade_factor(0.0), 1.0);
    assert_eq!(Profile::bicycle_grade_factor(5.0), 0.75);
    assert_eq!(Profile::bicycle_grade_factor(-10.0), 2.2);
    assert_eq!(Profile::bicycle_grade_factor(30.0), 0.3);
    assert!((Profile::bicycle_grade_factor(4.0) - 0.8).abs() < 1e-9);
}

#[test]
fn turns_across_traffic_are_avoided_on_the_side_traffic_drives() {
    // From the south (node 6) to the east (node 3), turning right at node 2, or going round left
    // three times: 6-2-1-4-5-3 is long; here a block lets the left turns go round it.
    //
    // 7 - 8 - 9
    // |   |   |
    // 4 - 5 - 6
    // |   |   |
    // 1 - 2 - 3
    let mut elements = Vec::new();
    for r in 0..3 {
        for c in 0..3 {
            elements.push(node((r * 3 + c + 1) as i64, r as f64, c as f64, &[]));
        }
    }
    for (id, nodes) in [(101, [1, 2, 3]), (102, [4, 5, 6]), (103, [7, 8, 9]), (201, [1, 4, 7]), (202, [2, 5, 8]), (203, [3, 6, 9])] {
        elements.push(way(id, &nodes, &[("highway", "residential")]));
    }
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    // From the middle of the left column, going north then needing to end east of the start.
    let mut r = RouteRequest::new(vec![at(0.5, 0.0), at(1.0, 0.5)], TravelMode::Car);
    let plain = router.route(&r).unwrap().remove(0);
    r.avoid.crossing_turns = true;
    r.driving_side = DrivingSide::Left;
    let left_side = router.route(&r).unwrap().remove(0);
    // Driving on the left, the right turn at node 4 crosses traffic; the plain route takes it.
    let first_turn = |route: &routing::Route| route.steps().find(|s| s.maneuver.kind == ManeuverKind::Turn).and_then(|s| s.maneuver.modifier);
    assert!(first_turn(&plain).is_some_and(|m| m.is_right()));
    assert!(left_side.duration_s >= plain.duration_s);
    assert!(DrivingSide::Left.crosses(true) && !DrivingSide::Left.crosses(false));
    assert!(DrivingSide::Right.crosses(false));
    assert_eq!(DrivingSide::of_country("jp"), DrivingSide::Left);
    assert_eq!(DrivingSide::of_country("US"), DrivingSide::Right);
}

#[test]
fn shortest_takes_the_short_slow_road() {
    // The direct road is a slow track; the detour a fast primary road.
    let mut elements = vec![node(1, 0.0, 0.0, &[]), node(2, 0.0, 1.0, &[]), node(3, 0.0, 2.0, &[]), node(4, 1.0, 0.0, &[]), node(5, 1.0, 2.0, &[])];
    elements.push(way(10, &[1, 2, 3], &[("highway", "service"), ("maxspeed", "10")]));
    elements.push(way(11, &[1, 4, 5, 3], &[("highway", "primary"), ("maxspeed", "60")]));
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    let mut r = request(TravelMode::Car);
    assert!(!takes_direct(&mut router, &r));
    r.optimize = Optimize::Shortest;
    assert!(takes_direct(&mut router, &r));
}

#[test]
fn areas_to_avoid_are_left_out() {
    let mut router = two_ways(&[], &[]);
    let mut r = request(TravelMode::Car);
    r.avoid_areas = vec![Polygon::circle(&at(0.0, 1.0), 30.0, 12)];
    let route = router.route(&r).unwrap().remove(0);
    assert!(route.distance_m > direct_m() + 100.0);
    assert!(!route.enters_avoided_area);
    // An area round the destination is not asked of the router.
    r.avoid_areas = vec![Polygon::circle(&at(0.0, 2.0), 30.0, 12)];
    assert!(takes_direct(&mut router, &r));
    assert!(Polygon::circle(&at(0.0, 1.0), 30.0, 12).contains(&at(0.0, 1.0)));
    assert!(!Polygon::circle(&at(0.0, 1.0), 30.0, 12).contains(&at(0.0, 1.5)));
}

#[test]
fn traffic_slows_and_closes_roads() {
    let mut router = two_ways(&[], &[]);
    let graph = router.graph().clone();
    // Every edge of the direct road (way 0 in the graph: the first way kept).
    let direct: Vec<u32> = (0..graph.edges.len() as u32).filter(|&e| graph.ways[graph.edges[e as usize].way as usize].osm_id == 10).collect();
    let mut jam = TrafficMap::default();
    for &e in &direct {
        jam.set(e, 0.1);
    }
    router.set_traffic_map(Some(jam));
    let r = router.route(&request(TravelMode::Car)).unwrap().remove(0);
    assert!(r.distance_m > direct_m() + 100.0, "a jam ten times slower makes the detour faster");
    assert!(r.duration_in_traffic_s.is_some());
    let mut closed = TrafficMap::default();
    for &e in &direct {
        closed.set(e, 0.0);
    }
    router.set_traffic_map(Some(closed));
    let r = router.route(&request(TravelMode::Car)).unwrap().remove(0);
    assert!(r.distance_m > direct_m() + 100.0);
    // A slow road taken is marked congested.
    let mut slow = TrafficMap::default();
    for e in 0..graph.edges.len() as u32 {
        slow.set(e, 0.3);
    }
    router.set_traffic_map(Some(slow));
    let r = router.route(&request(TravelMode::Car)).unwrap().remove(0);
    assert!(r.congestion.iter().all(|s| s.value == Congestion::Heavy) && !r.congestion.is_empty());
    assert!(r.duration_in_traffic_s.unwrap() > r.duration_s * 2.0);
    // Matching lines of traffic onto the roads by direction.
    let mut router = two_ways(&[], &[]);
    let eastward = myap_router::TrafficSpeed { shape: vec![at(0.0, 0.0), at(0.0, 1.0), at(0.0, 2.0)], factor: Some(0.5), closed: false };
    assert!(router.set_traffic(Some(&[eastward])) >= 1);
    let map = router.traffic().unwrap();
    let g = router.graph();
    for e in 0..g.edges.len() as u32 {
        if let Some(f) = map.factor(e) {
            let (a, b) = (g.edge_point(e, 0), g.edge_point(e, 1));
            assert!(a.bearing_to(&b) > 45.0 && a.bearing_to(&b) < 135.0, "only eastward edges: {f}");
        }
    }
}

#[test]
fn lanes_junction_names_signs_and_limits() {
    // West to east on a primary road with lanes, turning left at a named junction with signals
    // onto a link with destinations.
    let elements = vec![
        node(1, 0.0, 0.0, &[]),
        node(2, 0.0, 1.0, &[("highway", "traffic_signals"), ("name", "日比谷")]),
        node(3, 0.0, 2.0, &[]),
        node(4, 1.0, 1.0, &[]),
        way(1, &[1, 2], &[("highway", "primary"), ("oneway", "yes"), ("turn:lanes", "left|through|through;right"), ("maxspeed", "50"), ("name", "晴海通り")]),
        way(2, &[2, 3], &[("highway", "primary"), ("oneway", "yes"), ("maxspeed", "40")]),
        // Signs are on the way leaving the junction; on a two-way road they need `:forward` (wiki).
        way(3, &[2, 4], &[("highway", "secondary"), ("name", "日比谷通り"), ("destination:forward", "銀座;新橋"), ("destination:ref:forward", "R15")]),
    ];
    let mut router = Router::new(GraphBuilder::from_elements(&elements).unwrap().0);
    let route = router.route(&RouteRequest::new(vec![at(0.0, 0.2), at(0.9, 1.0)], TravelMode::Car)).unwrap().remove(0);
    let turn = route.steps().find(|s| s.maneuver.kind == ManeuverKind::Turn).expect("a turn");
    assert_eq!(turn.junction.as_deref(), Some("日比谷"));
    assert_eq!(turn.destinations.as_deref(), Some("R15, 銀座, 新橋"));
    assert_eq!(turn.lanes.len(), 3);
    assert!(turn.lanes[0].valid && !turn.lanes[1].valid && !turn.lanes[2].valid);
    assert_eq!(turn.lanes[2].indications, vec![LaneIndication::Through, LaneIndication::Right]);
    assert_eq!(route.speed_limit_at(0), Some(50));
    assert!(route.validate().is_ok());
    let graph = router.graph();
    assert!((0..graph.nodes.len() as u32).any(|n| graph.node_has(n, NodeFlag::SIGNALS)));
}

#[test]
fn graph_version_2_round_trip_and_version_1_refused() {
    let mut router = hill(10.0);
    let _ = router.route(&request(TravelMode::Car)).unwrap();
    let graph = router.graph().clone();
    assert!(graph.has_elevations());
    let mut bytes = Vec::new();
    graph.write(&mut bytes).unwrap();
    let back = Graph::read(&mut bytes.as_slice()).unwrap();
    assert_eq!(back, graph);
    let mut old = bytes.clone();
    old[8..12].copy_from_slice(&1u32.to_le_bytes());
    let error = Graph::read(&mut old.as_slice()).unwrap_err();
    assert!(error.to_string().contains("version 1"), "{error}");
    // Damaged elevations are refused.
    let mut bad = graph.clone();
    bad.elevations.pop();
    assert!(bad.validate().is_err());
    // Every single-byte change is either read or refused, never a panic.
    for i in 0..bytes.len().min(400) {
        let mut b = bytes.clone();
        b[i] ^= 0x5a;
        if let Ok(mut g) = Graph::read(&mut b.as_slice()) {
            g.compute_sources();
            let mut r = Router::new(g);
            let _ = r.route(&request(TravelMode::Foot));
        }
    }
}

#[test]
fn way_tags_are_read() {
    let t = |pairs: &[(&str, &str)]| Profile::way_flags(myap_router::Highway::Residential, &tags(pairs));
    use myap_router::WayInfo;
    assert_ne!(t(&[("surface", "compacted")]) & WayInfo::UNPAVED, 0);
    assert_eq!(t(&[("surface", "grass_paver")]) & WayInfo::UNPAVED, 0, "grass pavers are paved (OSM wiki)");
    assert_ne!(t(&[("width", "3.5")]) & WayInfo::NARROW, 0);
    assert_eq!(t(&[("width", "6 m")]) & WayInfo::NARROW, 0);
    assert_ne!(t(&[("lit", "24/7")]) & WayInfo::LIT, 0);
    assert_ne!(t(&[("lit", "no")]) & WayInfo::UNLIT, 0);
    assert_ne!(t(&[("smoothness", "very_bad")]) & WayInfo::ROUGH, 0);
    assert_eq!(Profile::parse_incline(Some("15 %")), Some(15.0));
    assert_eq!(Profile::parse_incline(Some("-10%")), Some(-10.0));
    assert_eq!(Profile::parse_incline(Some("up")), None);
    assert!((Profile::parse_incline(Some("45°")).unwrap() - 100.0).abs() < 1e-9);
    assert_eq!(Profile::parse_width(Some("4")), Some(4.0));
    assert_eq!(Profile::parse_width(Some("12'6\"")), None);
}
