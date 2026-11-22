//! What a graph adds to the routes of other routers (`Router::annotate`): the speed limits, the
//! alerts and the names of junctions along them, never replacing what a route already has.

use geo::Coordinate;
use myap_router::{GraphBuilder, Router};
use osm::{Element, Node, Tags, Way};
use routing::{AlertKind, Leg, Maneuver, ManeuverKind, Modifier, Route, Span, Step, TravelMode};

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

/// A road east from node 1 through a level crossing (node 2) to a junction with signals named
/// 本町 (node 3), limited to 40 km/h; then a road north from it to node 6, limited to 60 km/h.
///
/// ```text
///           6
///           |
/// 1 -- 2 -- 3
/// ```
fn network() -> Router {
    let elements = vec![
        node(1, 0.0, 0.0, &[]),
        node(2, 0.0, 1.0, &[("railway", "level_crossing")]),
        node(3, 0.0, 2.0, &[("highway", "traffic_signals"), ("name", "本町")]),
        node(6, 2.0, 2.0, &[]),
        way(10, &[1, 2, 3], &[("highway", "secondary"), ("maxspeed", "40")]),
        way(11, &[3, 6], &[("highway", "secondary"), ("maxspeed", "60")]),
    ];
    Router::new(GraphBuilder::from_elements(&elements).unwrap().0)
}

/// A route as another router may give it: sparse points (none at the level crossing), steps
/// without junction names, and nothing known along it.
fn foreign_route(points: Vec<Coordinate>) -> Route {
    let last = points.len() - 1;
    let depart = Step::new(Maneuver::new(ManeuverKind::Depart, None, points[0]), 200.0, 20.0, 0, 1);
    let turn = Step::new(Maneuver::new(ManeuverKind::Turn, Some(Modifier::Left), points[1]), 220.0, 20.0, 1, last);
    let arrive = Step::new(Maneuver::new(ManeuverKind::Arrive, None, points[last]), 0.0, 0.0, last, last);
    Route::new(points, vec![Leg { distance_m: 420.0, duration_s: 40.0, steps: vec![depart, turn, arrive] }], 420.0, 40.0)
}

#[test]
fn a_route_of_another_router_gains_limits_alerts_and_junctions() {
    let router = network();
    let mut route = foreign_route(vec![at(0.0, 0.0), at(0.0, 2.0), at(2.0, 2.0)]);
    let added = router.annotate(&mut route, TravelMode::Car);
    assert!(added.matched_fraction > 0.95, "{added:?}");

    // The limits of each road, over the segments along it.
    assert!(added.speed_limits);
    assert_eq!(route.speed_limit_at(0), Some(40));
    assert_eq!(route.speed_limit_at(2), Some(60));
    assert_eq!(route.speed_limits.iter().map(|s| s.value).collect::<Vec<_>>(), vec![40, 60]);

    // The level crossing, at the point of the route before it (never late), where it is.
    assert_eq!(added.alerts, 1);
    let crossing = &route.alerts[0];
    assert_eq!(crossing.kind, AlertKind::LevelCrossing);
    assert_eq!(crossing.index, 0);
    assert!(crossing.location.distance_m(&at(0.0, 1.0)) < 1.0);

    // The turn at 本町 is named; the departure and the arrival are not junctions.
    assert_eq!(added.junctions, 1);
    let steps: Vec<&Step> = route.steps().collect();
    assert_eq!(steps[1].junction.as_deref(), Some("本町"));
    assert_eq!(steps[0].junction, None);
    assert_eq!(steps[2].junction, None);
}

#[test]
fn what_a_route_has_is_kept() {
    let router = network();
    let mut route = foreign_route(vec![at(0.0, 0.0), at(0.0, 2.0), at(2.0, 2.0)]);
    route.speed_limits = vec![Span { start: 0, end: 2, value: 50 }];
    route.legs[0].steps[1].junction = Some("交差点".into());
    let added = router.annotate(&mut route, TravelMode::Car);
    assert!(!added.speed_limits);
    assert_eq!(route.speed_limits, vec![Span { start: 0, end: 2, value: 50 }]);
    assert_eq!(route.legs[0].steps[1].junction.as_deref(), Some("交差点"));
    assert_eq!(added.junctions, 0);
}

#[test]
fn a_route_off_the_roads_of_the_graph_gains_nothing() {
    let router = network();
    // 300 m south of the roads, along them: on none of them.
    let mut route = foreign_route(vec![at(-2.7, 0.0), at(-2.7, 2.0), at(-0.7, 2.0)]);
    let added = router.annotate(&mut route, TravelMode::Car);
    assert!(added.matched_fraction < Router::MIN_MATCHED_FRACTION, "{added:?}");
    assert!(route.speed_limits.is_empty());
    assert!(route.alerts.is_empty());
    assert!(route.steps().all(|s| s.junction.is_none()));
}

#[test]
fn the_direction_of_travel_must_agree() {
    let router = network();
    // East to west along the road: the road is two-way, so it is matched (against the way).
    let mut back = foreign_route(vec![at(2.0, 2.0), at(0.0, 2.0), at(0.0, 0.0)]);
    let added = router.annotate(&mut back, TravelMode::Car);
    assert!(added.matched_fraction > 0.95);
    assert_eq!(back.speed_limit_at(0), Some(60));
    assert_eq!(back.speed_limit_at(2), Some(40));
    // A line across the roads at right angles matches none of them.
    let mut across = foreign_route(vec![at(-1.0, 1.0), at(1.0, 1.0), at(1.0, 1.01)]);
    assert!(router.annotate(&mut across, TravelMode::Car).matched_fraction < 0.5);
}
