use geo::Coordinate;
use geo::Polygon;
use routing::{
    Alert, AlertKind, Avoid, Congestion, Lane, LaneIndication, Leg, Maneuver, ManeuverKind, Modifier, Route, RouteError, RouteRequest, Span, Step, TravelMode,
};

#[test]
fn modifiers_from_angles() {
    assert_eq!(Modifier::from_angle(0.0), Modifier::Straight);
    assert_eq!(Modifier::from_angle(-19.0), Modifier::Straight);
    assert_eq!(Modifier::from_angle(30.0), Modifier::SlightRight);
    assert_eq!(Modifier::from_angle(-30.0), Modifier::SlightLeft);
    assert_eq!(Modifier::from_angle(90.0), Modifier::Right);
    assert_eq!(Modifier::from_angle(-90.0), Modifier::Left);
    assert_eq!(Modifier::from_angle(150.0), Modifier::SharpRight);
    assert_eq!(Modifier::from_angle(-150.0), Modifier::SharpLeft);
    assert_eq!(Modifier::from_angle(179.0), Modifier::UTurn);
    assert!(Modifier::Left.is_left() && !Modifier::Left.is_right());
}

fn step(start: usize, end: usize) -> Step {
    Step::new(Maneuver::new(ManeuverKind::Turn, Some(Modifier::Left), Coordinate::new(0.0, 0.0)), 1.0, 1.0, start, end)
}

fn route(steps: Vec<Step>, points: usize) -> Route {
    Route::new((0..points).map(|i| Coordinate::new(0.0, i as f64 * 0.001)).collect(), vec![Leg { distance_m: 10.0, duration_s: 5.0, steps }], 10.0, 5.0)
}

#[test]
fn validation() {
    assert!(route(vec![step(0, 1), step(1, 2)], 3).validate().is_ok());
    assert!(route(vec![step(0, 3)], 3).validate().is_err());
    assert!(route(vec![step(1, 2), step(0, 1)], 3).validate().is_err());
    assert!(route(vec![], 1).validate().is_err());
    let mut r = route(vec![], 2);
    r.duration_s = f64::NAN;
    assert!(r.validate().is_err());
}

#[test]
fn request_validity_and_serialisation() {
    let r = RouteRequest::new(vec![Coordinate::new(35.0, 139.0), Coordinate::new(35.1, 139.1)], TravelMode::Car);
    assert!(r.is_valid());
    assert!(!RouteRequest::new(vec![Coordinate::new(35.0, 139.0)], TravelMode::Car).is_valid());
    assert!(!RouteRequest::new(vec![Coordinate::new(95.0, 139.0), Coordinate::new(35.0, 139.0)], TravelMode::Car).is_valid());
    let json = serde_json::to_string(&r).unwrap();
    let back: RouteRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
    let minimal: RouteRequest = serde_json::from_str(r#"{"waypoints":[{"latitude":1,"longitude":2},{"latitude":1,"longitude":3}],"mode":"foot"}"#).unwrap();
    assert_eq!(minimal.mode, TravelMode::Foot);
    assert_eq!(minimal.language, "en");
}

#[test]
fn errors_of_responses() {
    assert_eq!(RouteError::of_response(&fetch::Response::ok("")), None);
    assert!(matches!(RouteError::of_response(&fetch::Response::failed("x")), Some(RouteError::Network(_))));
    assert!(matches!(RouteError::of_response(&fetch::Response { status: 429, ..Default::default() }), Some(RouteError::Server(_))));
}

#[test]
fn annotations_are_validated() {
    let mut r = route(vec![step(0, 1), step(1, 2)], 3);
    r.elevations = vec![1.0, 2.0];
    assert!(r.validate().is_err(), "one elevation per point");
    r.elevations = vec![1.0, 2.0, 3.0];
    assert!(r.validate().is_ok());
    r.speed_limits = vec![Span { start: 0, end: 3, value: 50 }];
    assert!(r.validate().is_err(), "a span past the geometry");
    r.speed_limits = vec![Span { start: 0, end: 1, value: 50 }, Span { start: 1, end: 2, value: 40 }];
    assert!(r.validate().is_ok());
    assert_eq!(r.speed_limit_at(0), Some(50));
    assert_eq!(r.speed_limit_at(2), Some(40));
    r.alerts = vec![Alert { kind: AlertKind::LevelCrossing, index: 5, location: Coordinate::new(0.0, 0.0), text: None }];
    assert!(r.validate().is_err());
    r.alerts.clear();
    r.duration_in_traffic_s = Some(f64::INFINITY);
    assert!(r.validate().is_err());
}

#[test]
fn spans_join_and_climbs_ignore_noise() {
    let mut spans = Vec::new();
    Span::push(&mut spans, 0, 3, Congestion::Free);
    Span::push(&mut spans, 3, 5, Congestion::Free);
    Span::push(&mut spans, 5, 9, Congestion::Heavy);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].end, 5);
    assert_eq!(Span::at(&spans, 7), Some(Congestion::Heavy));
    assert_eq!(Span::at(&spans, 10), None);
    let mut r = route(vec![step(0, 5)], 6);
    // Noise of half a metre does not count; a 10 m climb and a 4 m descent do.
    r.elevations = vec![0.0, 0.4, 0.0, 10.0, 10.3, 6.0];
    let (up, down) = r.climb_m(1.0).unwrap();
    assert!((up - 10.0).abs() < 1e-6 && (down - 4.0).abs() < 1e-6, "{up} {down}");
    assert_eq!(Congestion::of_speed_factor(0.9), Congestion::Free);
    assert_eq!(Congestion::of_speed_factor(0.1), Congestion::Stopped);
}

#[test]
fn lanes_of_osm_tags() {
    // OSM wiki (Key:turn): lanes left to right separated by `|`, values by `;`, empty is `none`.
    let mut lanes = Lane::parse_osm("left|through;right||right").unwrap();
    assert_eq!(lanes.len(), 4);
    assert_eq!(lanes[1].indications, vec![LaneIndication::Through, LaneIndication::Right]);
    assert_eq!(lanes[2].indications, vec![LaneIndication::None]);
    assert!(Lane::mark(&mut lanes, 90.0));
    assert_eq!(lanes.iter().map(|l| l.valid).collect::<Vec<_>>(), vec![false, true, false, true]);
    assert!(Lane::mark(&mut lanes, -90.0));
    assert_eq!(lanes.iter().map(|l| l.valid).collect::<Vec<_>>(), vec![true, false, false, false]);
    // Straight on where no lane says through: the lanes without arrows.
    let mut plain = Lane::parse_osm("left|none|right").unwrap();
    assert!(Lane::mark(&mut plain, 0.0));
    assert_eq!(plain.iter().map(|l| l.valid).collect::<Vec<_>>(), vec![false, true, false]);
    // A turn no arrow leads to: the lanes say nothing.
    let mut only_left = Lane::parse_osm("left|left").unwrap();
    assert!(!Lane::mark(&mut only_left, 90.0));
    assert!(Lane::parse_osm("left|sideways").is_none());
    assert!(Lane::parse_osm("").is_none());
    assert_eq!(LaneIndication::from_osm("reverse"), Some(LaneIndication::UTurn));
    assert_eq!(LaneIndication::from_osrm("slight left"), Some(LaneIndication::SlightLeft));
}

#[test]
fn avoidances_combine_and_areas_are_checked() {
    let a = Avoid { tolls: true, narrow: true, hills: true, ..Avoid::default() };
    assert_eq!(a.strict(), Avoid { tolls: true, ..Avoid::default() });
    assert_eq!(a.without(&Avoid { tolls: true, ..Avoid::default() }), Avoid { narrow: true, hills: true, ..Avoid::default() });
    assert!(a.any() && !Avoid::default().any());
    let start = Coordinate::new(35.0, 139.0);
    let mut r = RouteRequest::new(vec![start, Coordinate::new(35.1, 139.1)], TravelMode::Car);
    r.avoid_areas = vec![Polygon::circle(&start, 100.0, 8), Polygon::circle(&Coordinate::new(35.05, 139.05), 100.0, 8)];
    assert!(r.is_valid());
    assert_eq!(r.effective_avoid_areas().len(), 1, "the area round the start is not avoided");
    r.avoid_areas.push(Polygon::new(vec![start, start]));
    assert!(!r.is_valid(), "a ring of two points");
    let back: RouteRequest = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back, r);
}
