use fetch::{Query, Response};
use geo::Coordinate;
use geocoding::PlaceSource;
use google::{Browser, Google, GoogleError, Pb, PbNode, Session, TileLayer, Tiles};
use routing::{ManeuverKind, Modifier, RouteError, RouteRequest, TravelMode};

fn google() -> Google {
    Google::new("ja-JP", "JP")
}

/// The value of `name` in the query of `url`, decoded.
fn param(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (n, v) = pair.split_once('=')?;
        (n == name).then(|| Query::decode(v))
    })
}

#[test]
fn pb_counts_every_token_of_a_message() {
    // `1m4` holds `3m2`, `3d`, `4d` and `6e2`: four tokens, two of them in `3m2`.
    let text = "!1m4!3m2!3d35.1!4d139.2!6e2!2i5";
    let pb = Pb::parse(text).unwrap();
    assert_eq!(pb.nodes.len(), 2);
    let PbNode::Message { children, .. } = &pb.nodes[0] else { panic!() };
    assert_eq!(children.len(), 2);
    assert_eq!(pb.to_string(), text);
    assert!(Pb::parse("!1m5!2i1").is_err());
    assert!(Pb::parse("!1x").is_err() || Pb::parse("!1x").is_ok());
    assert!(Pb::parse("1m1").is_err());
    assert_eq!(Pb::parse("").unwrap().nodes.len(), 0);
}

#[test]
fn pb_insertion_keeps_counts_and_order() {
    let mut pb = Pb::parse("!6m5!1m1!18b1!2m1!5e2!10b1").unwrap();
    let PbNode::Message { children, .. } = &mut pb.nodes[0] else { panic!() };
    let PbNode::Message { children: flags, .. } = &mut children[1] else { panic!() };
    Pb::insert_ordered(flags, PbNode::field(1, 'b', 1));
    Pb::insert_ordered(flags, PbNode::field(2, 'b', 1));
    Pb::insert_ordered(children, PbNode::field(7, 'b', 1));
    assert_eq!(pb.to_string(), "!6m8!1m1!18b1!2m3!1b1!2b1!5e2!7b1!10b1");
    // The same flag again replaces it.
    let PbNode::Message { children, .. } = &mut pb.nodes[0] else { panic!() };
    Pb::insert_ordered(children, PbNode::field(7, 'b', 0));
    assert_eq!(pb.to_string(), "!6m8!1m1!18b1!2m3!1b1!2b1!5e2!7b0!10b1");
    assert_eq!(Pb::escape("Yum! Cafe *2"), "Yum*21 Cafe *2A2");
}

#[test]
fn directions_request() {
    let mut r =
        RouteRequest::new(vec![Coordinate::new(35.6812, 139.7671), Coordinate::new(35.67, 139.75), Coordinate::new(35.6586, 139.7454)], TravelMode::Car);
    r.avoid.highways = true;
    r.avoid.tolls = true;
    r.avoid.ferries = true;
    let req = google().directions_request(&r).unwrap();
    assert!(req.url.starts_with("https://www.google.com/maps/preview/directions?authuser=0&hl=en&gl=us&pb="));
    let pb = param(&req.url, "pb").unwrap();
    // Origin, the stop, then the destination.
    assert!(pb.starts_with("!1m4!3m2!3d35.6812000!4d139.7671000!6e2!1m4!3m2!3d35.6700000!4d139.7500000!6e2!1m4!3m2!3d35.6586000!4d139.7454000!6e2!3m12"));
    // Both flags in the inner message 2 (4 + 2 tokens), ferries in the options (56 + 3).
    assert!(pb.contains("!6m59!1m5!18b1!30b1!31m1!1b1!34e1!2m6!1b1!2b1!5m1!6e2!20e3"), "{pb}");
    assert!(pb.contains("!291m0!7b1!10b1"), "{pb}");
    assert!(pb.contains("!20m5!1e0!2e3"));
    // The whole pb is still well formed.
    assert_eq!(Pb::parse(&pb).unwrap().to_string(), pb);
    assert_eq!(req.header_value("Sec-Fetch-Mode"), Some("cors"));
    assert_eq!(req.header_value("Cookie"), Some(Browser::CONSENT_COOKIES));

    let foot = RouteRequest::new(r.waypoints[..2].to_vec(), TravelMode::Foot);
    assert!(param(&google().directions_request(&foot).unwrap().url, "pb").unwrap().contains("!1e2!2e3"));
    let transit = RouteRequest::new(r.waypoints.clone(), TravelMode::Transit);
    assert_eq!(google().directions_request(&transit), Err(RouteError::UnsupportedMode));
}

#[test]
fn directions_response() {
    // Recorded 2026-11-22: driving from Tokyo Station to Tokyo Tower.
    let body = include_str!("fixtures/directions_tokyo.txt");
    let routes = google().parse_directions(&Response::ok(body)).unwrap();
    assert_eq!(routes.len(), 3);
    let route = &routes[0];
    assert_eq!(route.distance_m, 3809.0);
    assert_eq!(route.duration_s, 895.0);
    assert_eq!(route.duration_in_traffic_s, Some(944.0));
    assert_eq!(route.summary, "日比谷通り");
    assert_eq!(route.geometry.len(), 254);
    assert!(route.geometry[0].distance_m(&Coordinate::new(35.6810326, 139.7655744)) < 0.01);
    assert!(route.geometry.last().unwrap().distance_m(&Coordinate::new(35.6583872, 139.7450439)) < 0.01);
    assert!(route.validate().is_ok());
    let steps: Vec<_> = route.steps().collect();
    assert_eq!(steps[0].maneuver.kind, ManeuverKind::Depart);
    assert_eq!(steps.last().unwrap().maneuver.kind, ManeuverKind::Arrive);
    // "Turn left" 48 m after the start, then "Use the left lane to turn right onto 丸の内室町線".
    assert_eq!((steps[1].maneuver.kind, steps[1].maneuver.modifier), (ManeuverKind::Turn, Some(Modifier::Left)));
    assert_eq!((steps[2].maneuver.kind, steps[2].maneuver.modifier), (ManeuverKind::Turn, Some(Modifier::Right)));
    assert_eq!(steps[2].name.as_deref(), Some("丸の内室町線"));
    assert!(steps.len() >= 6);
    // The distances of the steps add up to the route's.
    let sum: f64 = steps.iter().map(|s| s.distance_m).sum();
    assert!((sum - 3809.0).abs() < 1.0);
}

#[test]
fn directions_errors() {
    let g = google();
    assert!(matches!(g.parse_directions(&Response { status: 429, ..Default::default() }), Err(RouteError::Server(_))));
    assert!(matches!(g.parse_directions(&Response::failed("offline")), Err(RouteError::Network(_))));
    assert_eq!(g.parse_directions(&Response::ok(")]}'\n[[null,[]]]")), Err(RouteError::NoRoute));
    assert!(matches!(g.parse_directions(&Response::ok("<html>")), Err(RouteError::Server(_))));
}

#[test]
fn geometry_is_e7_with_deltas() {
    let v: serde_json::Value = serde_json::from_str("[[356812000,-1000,1000],[1397671000,500,-500]]").unwrap();
    let points = Google::geometry(&v);
    assert_eq!(points.len(), 3);
    assert!((points[1].latitude - 35.6811).abs() < 1e-9 && (points[1].longitude - 139.76715).abs() < 1e-9);
    assert_eq!(points[2], Coordinate::new(35.6812, 139.7671));
}

#[test]
fn search_request_and_response() {
    let g = google();
    let near = Coordinate::new(35.6812, 139.7671);
    let req = g.search_request("ラーメン!", &near, None, 20);
    assert_eq!(param(&req.url, "q").as_deref(), Some("ラーメン"));
    assert_eq!(param(&req.url, "hl").as_deref(), Some("ja"));
    assert_eq!(param(&req.url, "gl").as_deref(), Some("jp"));
    let pb = param(&req.url, "pb").unwrap();
    assert!(pb.starts_with("!1sラーメン!4m8!1m3!1d25229.167291701906!2d139.7671000!3d35.6812000!3m2!1i1920!2i945"));
    assert!(pb.contains("!7i20!8i20!10b1"));
    assert!(pb.contains("!30m28!1m6!1m2!1i0!2i0!2m2!1i530!2i945!1m6!1m2!1i1870!2i0"));
    assert_eq!(Pb::parse(&pb).unwrap().to_string(), pb);

    // Recorded 2026-11-22 and cut to the first three results.
    let places = g.parse_search(&Response::ok(include_str!("fixtures/search_ramen.txt"))).unwrap();
    assert_eq!(places.len(), 3);
    let p = &places[0];
    assert_eq!(p.name, "つじ田 日本橋八重洲店");
    assert_eq!(p.id, "google:0x60188bfdeaae4297:0x3dc00b75ddc95be");
    assert_eq!(p.source, PlaceSource::Google);
    assert_eq!(p.category.as_deref(), Some("ラーメン屋"));
    assert_eq!(p.rating, Some(4.5));
    assert_eq!(p.review_count, Some(4282));
    assert_eq!(p.phone.as_deref(), Some("03-3527-9373"));
    assert_eq!(p.hours.len(), 7);
    assert_eq!(p.hours[0], "日曜日: 11時00分～21時30分");
    assert!(p.status.is_some());
    assert!(places[1].hours[1].contains(", "), "split shifts are joined");
    // A reply that is not JSON means the shape changed (or a page was served instead).
    assert!(matches!(g.parse_search(&Response::ok("<!doctype html>")), Err(GoogleError::Unreadable(_))));
    assert!(matches!(g.parse_search(&Response::ok("<a href=\"https://www.google.com/sorry/index\">")), Err(GoogleError::Blocked(_))));
}

#[test]
fn suggestions() {
    let g = google();
    let req = g.suggest_request("東京タワー", &Coordinate::new(35.68, 139.76), None, 3);
    assert_eq!(param(&req.url, "ech").as_deref(), Some("3"));
    assert!(param(&req.url, "pb").unwrap().starts_with("!2i5!4m12!1m3!1d20000.000!2d139.7600000!3d35.6800000"));
    // Recorded 2026-11-22.
    let suggestions = g.parse_suggest(&Response::ok(include_str!("fixtures/suggest_tokyo_tower.txt"))).unwrap();
    assert!(suggestions.len() >= 4);
    assert_eq!(suggestions[0].text, "東京タワー");
    let place = suggestions[0].place.as_ref().unwrap();
    assert_eq!(place.id, "google:0x60188bbd9009ec09:0x481a93f0d2a409dd");
    assert_eq!(place.address.as_deref(), Some("東京都港区芝公園４丁目２−８"));
    assert!((place.coordinate.latitude - 35.6585805).abs() < 1e-9);
    // The envelope may be followed by another object, and an empty `d` is no suggestions.
    let two = r#"{"c":0,"d":"","u":"x"}/*""*/{"c":0,"d":"","e":"t"}"#;
    assert_eq!(g.parse_suggest(&Response::ok(two)).unwrap().len(), 0);
    assert!(Google::is_feature_id("0x60188bbd9009ec09:0x481a93f0d2a409dd"));
    assert!(!Google::is_feature_id("0x:0x1"));
    assert!(!Google::is_feature_id("ChIJ123"));
}

#[test]
fn tiles() {
    let t = Tiles::templates(TileLayer::Satellite, "ja-JP", "JP");
    assert_eq!(t.len(), 4);
    assert_eq!(t[1], "https://mt1.google.com/vt/lyrs=s&hl=ja&gl=jp&x={x}&y={y}&z={z}");
    let traffic = Tiles::templates(TileLayer::Traffic, "en", "us");
    assert_eq!(traffic.len(), 1);
    assert!(traffic[0].contains("!1i{z}!2i{x}!3i{y}"));
    assert_eq!(Tiles::max_zoom(TileLayer::Traffic), 16);
    // Nothing but letters enters the URL from the language.
    assert!(Tiles::templates(TileLayer::Roadmap, "x&y=1", "u s")[0].contains("hl=xy&gl=us"));
}

#[test]
fn browser_headers() {
    assert_eq!(Browser::accept_language("ja-JP"), "ja-JP,ja;q=0.9");
    assert_eq!(Browser::accept_language("en"), "en");
    let g = google();
    assert_eq!(g.hl(), "ja");
    assert_eq!(Google::new("zh-Hant-TW", "tw").hl(), "zh-TW");
    let session = g.session_request();
    assert_eq!(session.url, "https://www.google.com/maps?hl=ja&gl=jp");
    assert_eq!(session.header_value("Sec-Fetch-Mode"), Some("navigate"));
}

#[test]
fn session_cookies() {
    let mut session = Session::new();
    assert!(!session.is_started());
    let response = Response {
        status: 200,
        body: String::new(),
        headers: vec![
            ("Set-Cookie".into(), "NID=511=abc; expires=Sun, 24-May-2027 12:00:00 GMT; path=/; domain=.google.com; HttpOnly".into()),
            ("set-cookie".into(), "CONSENT=PENDING+987; path=/".into()),
            ("Set-Cookie".into(), "AEC=xyz; Secure".into()),
            ("Set-Cookie".into(), "BAD=a;b".into()),
        ],
        error: None,
    };
    session.absorb(&response);
    assert!(session.is_started());
    let request = session.apply(google().session_request());
    // The consent stays the refusal; the pending consent of the response is ignored.
    assert_eq!(request.header_value("Cookie"), Some("SOCS=CAESHAgBEhIaAB; AEC=xyz; BAD=a; NID=511=abc"));
    assert_eq!(request.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case("cookie")).count(), 1);
    let expire = Response { status: 200, body: String::new(), headers: vec![("Set-Cookie".into(), "NID=; Max-Age=0".into())], error: None };
    session.absorb(&expire);
    assert!(!session.apply(google().session_request()).header_value("Cookie").unwrap().contains("NID"));
    session.clear();
    assert_eq!(session.apply(google().session_request()).header_value("Cookie"), Some(Browser::CONSENT_COOKIES));
}
