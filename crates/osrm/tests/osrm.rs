use fetch::Response;
use geo::Coordinate;
use osrm::Osrm;
use routing::{ManeuverKind, Modifier, RemoteRouter, RouteError, RouteRequest, TravelMode};

fn request(mode: TravelMode) -> RouteRequest {
    RouteRequest::new(vec![Coordinate::new(35.6812, 139.7671), Coordinate::new(35.685, 139.77)], mode)
}

#[test]
fn builds_the_url_of_the_route_service() {
    let osrm = Osrm::fossgis("Myap/test");
    let mut r = request(TravelMode::Car);
    r.alternatives = true;
    r.avoid.tolls = true;
    r.avoid.ferries = true;
    r.heading = Some(-10.0);
    let req = osrm.request(&r).unwrap();
    // Coordinates are longitude,latitude, separated by semicolons.
    assert_eq!(
        req.url,
        "https://routing.openstreetmap.de/routed-car/route/v1/driving/139.767100,35.681200;139.770000,35.685000?overview=false&geometries=polyline6&steps=true&alternatives=true&exclude=toll%2Cferry&bearings=350%2C60%3B"
    );
    assert_eq!(req.header_value("User-Agent"), Some("Myap/test"));
    assert!(osrm.request(&request(TravelMode::Foot)).unwrap().url.starts_with("https://routing.openstreetmap.de/routed-foot/route/v1/driving/"));
    assert_eq!(osrm.request(&request(TravelMode::Transit)), Err(RouteError::UnsupportedMode));
    assert!(matches!(osrm.request(&RouteRequest::new(vec![Coordinate::new(0.0, 0.0)], TravelMode::Car)), Err(RouteError::InvalidRequest(_))));
}

#[test]
fn reads_a_recorded_response() {
    // Recorded from routing.openstreetmap.de (foot), Tokyo Station, 2026-11-22.
    let body = include_str!("fixtures/foot_tokyo.json");
    let osrm = Osrm::fossgis("Myap/test");
    let mut r = request(TravelMode::Foot);
    r.avoid.tolls = true;
    let routes = osrm.parse(&r, &Response::ok(body)).unwrap();
    assert_eq!(routes.len(), 2);
    let route = &routes[0];
    assert_eq!(route.distance_m, 805.1);
    assert_eq!(route.duration_s, 643.9);
    assert!(route.validate().is_ok());
    let steps: Vec<_> = route.steps().collect();
    assert_eq!(steps.first().unwrap().maneuver.kind, ManeuverKind::Depart);
    assert_eq!(steps.last().unwrap().maneuver.kind, ManeuverKind::Arrive);
    assert_eq!(steps[1].maneuver.kind, ManeuverKind::Turn);
    assert_eq!(steps[1].maneuver.modifier, Some(Modifier::Right));
    assert_eq!(steps[1].name.as_deref(), Some("中央通路"));
    // Each step starts at the point of its maneuver.
    for step in &steps {
        let start = route.geometry[step.geometry_start];
        assert!(start.distance_m(&step.maneuver.location) < 1.0, "{start:?} {:?}", step.maneuver.location);
    }
    // Walking cannot exclude toll roads: the route says it was ignored.
    assert!(route.ignored_avoid.tolls);
}

#[test]
fn reads_errors() {
    let osrm = Osrm::fossgis("t");
    let r = request(TravelMode::Car);
    let no_route = Response { status: 400, body: r#"{"code":"NoRoute","message":"Impossible route between points"}"#.into(), headers: Vec::new(), error: None };
    assert_eq!(osrm.parse(&r, &no_route), Err(RouteError::NoRoute));
    let no_segment = Response { status: 400, body: r#"{"code":"NoSegment"}"#.into(), headers: Vec::new(), error: None };
    assert_eq!(osrm.parse(&r, &no_segment), Err(RouteError::NoRoad));
    let html = Response { status: 502, body: "<html>Bad gateway</html>".into(), headers: Vec::new(), error: None };
    assert!(matches!(osrm.parse(&r, &html), Err(RouteError::Server(_))));
    assert!(matches!(osrm.parse(&r, &Response::failed("offline")), Err(RouteError::Network(_))));
    let ok_but_empty = Response::ok(r#"{"code":"Ok","routes":[]}"#);
    assert_eq!(osrm.parse(&r, &ok_but_empty), Err(RouteError::NoRoute));
}
