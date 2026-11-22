use fetch::Response;
use geo::Coordinate;
use routing::{ManeuverKind, Modifier, RemoteRouter, RouteError, RouteRequest, TravelMode};
use valhalla::Valhalla;

fn request(mode: TravelMode) -> RouteRequest {
    let mut r = RouteRequest::new(vec![Coordinate::new(35.6812, 139.7671), Coordinate::new(35.685, 139.77)], mode);
    r.language = "ja-JP".into();
    r
}

#[test]
fn builds_the_body() {
    let mut r = request(TravelMode::Car);
    r.avoid.tolls = true;
    r.alternatives = true;
    r.heading = Some(370.0);
    let body = Valhalla::body(&r);
    assert_eq!(body["costing"], "auto");
    assert_eq!(body["costing_options"]["auto"]["use_tolls"], 0.0);
    assert!(body["costing_options"]["auto"].get("use_highways").is_none());
    assert_eq!(body["locations"][0]["lat"], 35.6812);
    assert_eq!(body["locations"][0]["heading"], 10);
    assert!(body["locations"][1].get("heading").is_none());
    assert_eq!(body["alternates"], 2);
    assert_eq!(body["directions_options"]["language"], "ja-JP");
    assert_eq!(Valhalla::body(&request(TravelMode::Foot))["costing"], "pedestrian");
    let req = Valhalla::fossgis("t").request(&r).unwrap();
    assert_eq!(req.url, "https://valhalla1.openstreetmap.de/route");
}

#[test]
fn reads_a_recorded_response() {
    // Recorded from valhalla1.openstreetmap.de (auto, ja-JP), Tokyo Station, 2026-11-22.
    let body = include_str!("fixtures/auto_tokyo.json");
    let routes = Valhalla::fossgis("t").parse(&request(TravelMode::Car), &Response::ok(body)).unwrap();
    assert!(!routes.is_empty());
    let route = &routes[0];
    assert!((route.distance_m - 1448.0).abs() < 0.5);
    assert!((route.duration_s - 258.321).abs() < 1e-6);
    assert!(route.validate().is_ok());
    let steps: Vec<_> = route.steps().collect();
    assert_eq!(steps[0].maneuver.kind, ManeuverKind::Depart);
    assert_eq!((steps[1].maneuver.kind, steps[1].maneuver.modifier), (ManeuverKind::Turn, Some(Modifier::Left)));
    assert_eq!(steps[1].instruction.as_deref(), Some("左方向です。"));
    assert!((steps[1].distance_m - 57.0).abs() < 1e-6);
    assert_eq!(steps.last().unwrap().maneuver.kind, ManeuverKind::Arrive);
}

#[test]
fn reads_errors() {
    let v = Valhalla::fossgis("t");
    let r = request(TravelMode::Car);
    let far = Response {
        status: 400,
        body: r#"{"error_code":154,"error":"Path distance exceeds the max distance limit: 1500000 meters","status_code":400}"#.into(),
        headers: Vec::new(),
        error: None,
    };
    assert!(matches!(v.parse(&r, &far), Err(RouteError::InvalidRequest(_))));
    let none = Response { status: 400, body: r#"{"error_code":442,"error":"No path could be found for input"}"#.into(), headers: Vec::new(), error: None };
    assert_eq!(v.parse(&r, &none), Err(RouteError::NoRoute));
    let no_edge = Response { status: 400, body: r#"{"error_code":171,"error":"No suitable edges near location"}"#.into(), headers: Vec::new(), error: None };
    assert_eq!(v.parse(&r, &no_edge), Err(RouteError::NoRoad));
    assert!(matches!(v.parse(&r, &Response { status: 503, body: "busy".into(), headers: Vec::new(), error: None }), Err(RouteError::Server(_))));
}
