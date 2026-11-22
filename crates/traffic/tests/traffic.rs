use fetch::{Method, Query, Response};
use geo::{Bounds, Coordinate};
use traffic::here::Here;
use traffic::{AreaLimit, Congestion, Criticality, FlowSegment, Incident, IncidentKind, Rfc3339, Traffic, TrafficError, TrafficProvider};

const KEY: &str = "s3cr&t key/+=";

fn here() -> Here {
    Here::new(KEY, "Myap/test")
}

fn berlin() -> Bounds {
    // The bounding box of the "Geospatial filters" page of the HERE Traffic API v7 guide.
    Bounds { south: 52.527129, west: 13.386969, north: 52.549420, east: 13.424134 }
}

fn response(status: u16, body: &str) -> Response {
    Response { status, body: body.into(), headers: Vec::new(), error: None }
}

fn segment(speed: Option<f64>, free: Option<f64>, jam: Option<f64>, confidence: Option<f64>) -> FlowSegment {
    FlowSegment {
        shape: vec![Coordinate::new(0.0, 0.0), Coordinate::new(0.0, 0.001)],
        length_m: None,
        speed_kmh: speed,
        free_flow_kmh: free,
        jam_factor: jam,
        confidence,
        closed: false,
    }
}

#[test]
fn builds_the_url_of_flow_with_the_key_last_and_encoded() {
    let request = here().flow_request(&berlin()).unwrap();
    assert_eq!(request.method, Method::Get);
    let encoded_key = Query::encode(KEY);
    assert_eq!(
        request.url,
        format!("https://data.traffic.hereapi.com/v7/flow?in=bbox%3A13.38697%2C52.52713%2C13.42413%2C52.54942&locationReferencing=shape&apiKey={encoded_key}")
    );
    // The key is in the query only, percent-encoded, and in no header.
    assert_eq!(encoded_key, "s3cr%26t%20key%2F%2B%3D");
    assert_eq!(request.url.matches(&encoded_key).count(), 1);
    assert!(!request.url.contains(KEY));
    assert!(request.headers.iter().all(|(n, v)| !v.contains("s3cr") && !n.contains("s3cr")));
    assert_eq!(request.header_value("User-Agent"), Some("Myap/test"));
    assert_eq!(request.body, None);
    assert!(!format!("{:?}", here()).contains("s3cr"));
}

#[test]
fn builds_the_url_of_incidents_with_filters() {
    let mut here = here();
    here.base = "https://example.org/traffic/".into();
    here.functional_classes = vec![1, 2];
    here.language = Some("ja-JP".into());
    let request = here.incidents_request(&Bounds { south: -33.9, west: 151.2, north: -33.8, east: 151.3 }).unwrap();
    assert!(request.url.starts_with(
        "https://example.org/traffic/v7/incidents?in=bbox%3A151.20000%2C-33.90000%2C151.30000%2C-33.80000&locationReferencing=shape&functionalClasses=1%2C2&lang=ja-JP&apiKey="
    ));
    // `lang` is a parameter of incidents only.
    assert!(!here.flow_request(&berlin()).unwrap().url.contains("lang="));
}

#[test]
fn rounds_coordinates_to_five_decimals_without_negative_zero() {
    let bounds = Bounds { south: -0.000001, west: -0.000004, north: 0.123456789, east: 0.987654321 };
    assert_eq!(Here::bbox(&bounds).unwrap(), "bbox:0.00000,0.00000,0.98765,0.12346");
}

#[test]
fn refuses_invalid_or_large_bounds_and_missing_keys() {
    let here = here();
    let inverted = Bounds { south: 1.0, west: 0.0, north: 0.0, east: 1.0 };
    assert!(matches!(here.flow_request(&inverted), Err(TrafficError::InvalidRequest(_))));
    let nan = Bounds { south: f64::NAN, west: 0.0, north: 0.0, east: 1.0 };
    assert!(matches!(here.incidents_request(&nan), Err(TrafficError::InvalidRequest(_))));
    let out_of_range = Bounds { south: 0.0, west: 179.5, north: 0.5, east: 180.5 };
    assert!(matches!(here.flow_request(&out_of_range), Err(TrafficError::InvalidRequest(_))));
    // The API accepts up to one degree of width and height.
    let one = Bounds { south: 10.0, west: 20.0, north: 11.0, east: 21.0 };
    assert!(here.flow_request(&one).is_ok());
    let wide = Bounds { south: 10.0, west: 20.0, north: 10.5, east: 21.0001 };
    assert_eq!(here.flow_request(&wide), Err(TrafficError::TooLarge));
    let mut keyless = here.clone();
    keyless.api_key.clear();
    assert!(matches!(keyless.flow_request(&berlin()), Err(TrafficError::Unauthorized(_))));
    let mut classes = here.clone();
    classes.functional_classes = vec![6];
    assert!(matches!(classes.flow_request(&berlin()), Err(TrafficError::InvalidRequest(_))));
}

#[test]
fn parses_flow_shapes_sub_segments_and_closures() {
    // Mirrors the response of "Get started with HERE Traffic API v7" (first four links and
    // its sub-segments, of lengths rescaled to the links kept), the shape of "Location
    // referencing", and the `olr` location of "How to request flow data".
    let traffic = here().parse_flow(&Response::ok(include_str!("fixtures/flow_berlin.json"))).unwrap();
    assert_eq!(traffic.updated_ms, Some(1_625_147_583_000));
    assert!(traffic.incidents.is_empty());
    let flow = &traffic.flow;
    assert_eq!(flow.len(), 6, "{flow:#?}");

    // Location 1: links 1–2 lie on the first sub-segment, links 3–4 on the second; each pair
    // joins exactly and becomes one segment.
    let (a, b) = (&flow[0], &flow[1]);
    assert_eq!(a.shape.len(), 3);
    assert_eq!(a.shape[0], Coordinate::new(52.50708997249603, 13.398289997130632));
    assert_eq!(a.shape[2], Coordinate::new(52.506729969754815, 13.399609979242086));
    assert_eq!(a.length_m, Some(98.0));
    assert!((a.speed_kmh.unwrap() - 7.2222223 * 3.6).abs() < 1e-9);
    assert!((a.free_flow_kmh.unwrap() - 11.111112 * 3.6).abs() < 1e-9);
    assert_eq!(a.jam_factor, Some(3.2));
    assert_eq!(a.confidence, Some(0.99));
    assert!(!a.closed);
    assert_eq!(b.shape.first(), a.shape.last());
    assert_eq!(b.shape.len(), 3);
    assert_eq!(b.length_m, Some(156.0));
    assert_eq!(b.jam_factor, Some(8.7));
    assert_eq!(a.congestion(), Congestion::Light);
    assert_eq!(b.congestion(), Congestion::Heavy);

    // Location 2: four contiguous links of a closed road, one segment of every point.
    let closed = &flow[2];
    assert_eq!(closed.shape.len(), 3 + 2 + 3 + 9 - 3);
    assert_eq!(closed.length_m, Some(659.0));
    assert!(closed.closed);
    assert_eq!(closed.congestion(), Congestion::Closed);
    assert_eq!(closed.speed_factor(), None);

    // Location 3: links that do not join stay apart; the missing values stay missing, and a
    // missing link length is measured.
    let (c, d) = (&flow[3], &flow[4]);
    assert_eq!(c.shape.len(), 2);
    assert_eq!(d.shape[0], Coordinate::new(52.532, 13.402));
    assert_eq!(c.length_m, Some(100.0));
    let measured = Coordinate::new(52.532, 13.402).distance_m(&Coordinate::new(52.5325, 13.4025));
    assert!((d.length_m.unwrap() - measured).abs() < 1e-6);
    assert_eq!(c.speed_kmh, None);
    assert_eq!(c.confidence, None);
    assert_eq!(c.jam_factor, Some(1.2));
    assert!(!c.closed);
    assert_eq!(c.congestion(), Congestion::Free);
    assert_eq!(c.speed_factor(), None);

    // Location 4: a reversible road that is not routable now counts as closed.
    assert!(flow[5].closed);
    assert_eq!(flow[5].congestion(), Congestion::Closed);
    // Location 5 has no shape and is left out.
}

#[test]
fn parses_an_empty_flow_and_rejects_malformed_bodies() {
    // The empty answer of "How to request flow data".
    let empty = here().parse_flow(&Response::ok(r#"{"sourceUpdated":"2021-06-08T10:07:23Z","results":[]}"#)).unwrap();
    assert!(empty.flow.is_empty());
    assert!(empty.updated_ms.is_some());
    assert!(matches!(here().parse_flow(&Response::ok("{}")), Err(TrafficError::Parse(_))));
    assert!(matches!(here().parse_flow(&Response::ok("<html>")), Err(TrafficError::Parse(_))));
    assert!(matches!(here().parse_flow(&Response::ok("")), Err(TrafficError::Parse(_))));
    // Points out of range are dropped; a link left with fewer than two points is dropped.
    let body = r#"{"results":[{"location":{"length":1,"shape":{"links":[{"points":[{"lat":95,"lng":0},{"lat":1,"lng":1}],"length":1}]}},"currentFlow":{"freeFlow":1,"jamFactor":0}}]}"#;
    assert!(here().parse_flow(&Response::ok(body)).unwrap().flow.is_empty());
}

#[test]
fn classifies_congestion() {
    // By jam factor, when there is one.
    let jam = |j: f64| segment(None, None, Some(j), Some(0.9)).congestion();
    assert_eq!(jam(0.0), Congestion::Free);
    assert_eq!(jam(1.99), Congestion::Free);
    assert_eq!(jam(2.0), Congestion::Light);
    assert_eq!(jam(4.0), Congestion::Moderate);
    assert_eq!(jam(7.0), Congestion::Heavy);
    assert_eq!(jam(9.0), Congestion::Stopped);
    assert_eq!(jam(10.0), Congestion::Closed);
    // Else by the ratio of the speeds.
    let ratio = |speed: f64| segment(Some(speed), Some(100.0), None, None).congestion();
    assert_eq!(ratio(120.0), Congestion::Free);
    assert_eq!(ratio(85.0), Congestion::Free);
    assert_eq!(ratio(70.0), Congestion::Light);
    assert_eq!(ratio(50.0), Congestion::Moderate);
    assert_eq!(ratio(30.0), Congestion::Heavy);
    assert_eq!(ratio(10.0), Congestion::Stopped);
    // Values derived from the speed limit (confidence 0.5 or less) say nothing about traffic.
    assert_eq!(segment(None, None, Some(8.0), Some(0.5)).congestion(), Congestion::Unknown);
    assert_eq!(segment(None, None, None, None).congestion(), Congestion::Unknown);
    assert_eq!(segment(Some(10.0), Some(0.0), None, None).congestion(), Congestion::Unknown);
    assert_eq!(segment(Some(f64::NAN), Some(50.0), Some(f64::NAN), None).congestion(), Congestion::Unknown);
    let mut closed = segment(Some(50.0), Some(50.0), Some(0.0), Some(0.2));
    closed.closed = true;
    assert_eq!(closed.congestion(), Congestion::Closed);
}

#[test]
fn computes_speed_factors() {
    assert_eq!(segment(Some(50.0), Some(100.0), None, Some(0.99)).speed_factor(), Some(0.5));
    // Clamped to [0.05, 1.5].
    assert_eq!(segment(Some(0.0), Some(100.0), None, Some(0.99)).speed_factor(), Some(0.05));
    assert_eq!(segment(Some(300.0), Some(100.0), None, Some(0.99)).speed_factor(), Some(1.5));
    // Historical speeds (0.5 < confidence <= 0.7) are kept; speed limits are not.
    assert_eq!(segment(Some(60.0), Some(100.0), None, Some(0.6)).speed_factor(), Some(0.6));
    assert_eq!(segment(Some(60.0), Some(100.0), None, Some(0.5)).speed_factor(), None);
    assert_eq!(segment(Some(60.0), Some(100.0), None, None).speed_factor(), Some(0.6));
    assert_eq!(segment(None, Some(100.0), None, None).speed_factor(), None);
    assert_eq!(segment(Some(60.0), None, None, None).speed_factor(), None);
    assert_eq!(segment(Some(60.0), Some(0.0), None, None).speed_factor(), None);
    assert_eq!(segment(Some(-1.0), Some(10.0), None, None).speed_factor(), None);
    assert_eq!(segment(Some(f64::INFINITY), Some(10.0), None, None).speed_factor(), None);
}

#[test]
fn parses_incidents() {
    // Mirrors the response of "How to request incident data" (with shapes in place of `olr`),
    // the times of the `IncidentDetails` example of the API reference, and kinds and values
    // the documentation lists.
    let traffic = here().parse_incidents(&Response::ok(include_str!("fixtures/incidents_leverkusen.json"))).unwrap();
    assert_eq!(traffic.updated_ms, Some(1_625_566_921_000));
    assert!(traffic.flow.is_empty());
    let incidents = &traffic.incidents;
    assert_eq!(incidents.len(), 3);

    let first = &incidents[0];
    assert_eq!(first.id, "3195531507295233286");
    assert_eq!(first.kind, IncidentKind::Other);
    assert_eq!(first.criticality, Criticality::Minor);
    assert!(first.road_closed);
    assert_eq!(first.description.as_deref(), Some("Bei Leverkusen - Vorübergehende Begrenzung der Breite 7.0 m. Bauarbeiten auf Abschnitt."));
    assert_eq!(first.start_ms, Some(1_575_414_000_000));
    assert_eq!(first.end_ms, Some(1_678_661_999_000));
    // Two links join, the third does not.
    assert_eq!(first.shape.len(), 2);
    assert_eq!(first.shape[0].len(), 3);
    assert_eq!(first.shape[1].len(), 2);
    assert!(first.is_active_at(1_600_000_000_000));
    assert!(!first.is_active_at(1_700_000_000_000));
    assert!(!first.is_active_at(1_500_000_000_000));

    let closure = &incidents[1];
    // The stable `originalId`, not the `id` of this version.
    assert_eq!(closure.id, "2349753562597427858");
    assert_eq!(closure.kind, IncidentKind::RoadClosure);
    assert_eq!(closure.criticality, Criticality::Critical);
    assert_eq!(closure.description.as_deref(), Some("Closed due to roadwork."));
    assert_eq!(closure.start_ms, Some(1_570_864_800_000));
    assert_eq!(closure.end_ms, None);
    assert!(closure.is_active_at(i64::MAX));

    let unknown = &incidents[2];
    assert_eq!(unknown.kind, IncidentKind::Other);
    assert_eq!(unknown.criticality, Criticality::Low);
    assert!(!unknown.road_closed);
    assert!(unknown.shape.is_empty());
    assert_eq!(unknown.description, None);
}

#[test]
fn maps_every_documented_incident_type() {
    let types = [
        ("accident", IncidentKind::Accident),
        ("construction", IncidentKind::Construction),
        ("congestion", IncidentKind::Congestion),
        ("disabledVehicle", IncidentKind::DisabledVehicle),
        ("massTransit", IncidentKind::MassTransit),
        ("plannedEvent", IncidentKind::PlannedEvent),
        ("roadHazard", IncidentKind::RoadHazard),
        ("roadClosure", IncidentKind::RoadClosure),
        ("weather", IncidentKind::Weather),
        ("laneRestriction", IncidentKind::LaneRestriction),
        ("other", IncidentKind::Other),
    ];
    for (name, kind) in types {
        let body = format!(r#"{{"results":[{{"incidentDetails":{{"id":"1","originalId":"1","roadClosed":false,"type":"{name}","criticality":"major"}}}}]}}"#);
        let incidents = here().parse_incidents(&Response::ok(body)).unwrap().incidents;
        assert_eq!(incidents[0].kind, kind, "{name}");
        assert_eq!(incidents[0].criticality, Criticality::Major);
    }
}

#[test]
fn reads_rfc3339_times() {
    assert_eq!(Rfc3339::epoch_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(Rfc3339::epoch_ms("1969-12-31T23:59:59Z"), Some(-1_000));
    assert_eq!(Rfc3339::epoch_ms("2024-02-29T12:00:00Z"), Some(1_709_208_000_000));
    assert_eq!(Rfc3339::epoch_ms("2021-07-04T11:18:01+09:00"), Some(1_625_365_081_000));
    assert_eq!(Rfc3339::epoch_ms("2021-07-04T02:18:01-00:00"), Some(1_625_365_081_000));
    assert_eq!(Rfc3339::epoch_ms("2021-07-04t02:18:01.123456z"), Some(1_625_365_081_123));
    assert_eq!(Rfc3339::epoch_ms("2021-07-04 02:18:01.5Z"), Some(1_625_365_081_500));
    // Without an offset: UTC, as HERE states.
    assert_eq!(Rfc3339::epoch_ms("2021-07-04T02:18:01"), Some(1_625_365_081_000));
    // A leap second is the next minute's first.
    assert_eq!(Rfc3339::epoch_ms("2016-12-31T23:59:60Z"), Rfc3339::epoch_ms("2017-01-01T00:00:00Z"));
    for bad in [
        "",
        "2021",
        "2023-02-29T00:00:00Z",
        "2021-13-01T00:00:00Z",
        "2021-00-01T00:00:00Z",
        "2021-04-31T00:00:00Z",
        "2021-01-01T24:00:00Z",
        "2021-01-01T00:60:00Z",
        "2021-01-01T00:00:61Z",
        "2021-01-01T00:00:00.Z",
        "2021-01-01T00:00:00+0900",
        "2021-01-01T00:00:00+24:00",
        "2021-01-01T00:00:00ZZ",
        "2021-01-01X00:00:00Z",
        "+021-01-01T00:00:00Z",
        "２０２１-01-01T00:00:00Z",
    ] {
        assert_eq!(Rfc3339::epoch_ms(bad), None, "{bad:?}");
    }
    assert_eq!(Rfc3339::days_from_civil(i64::MIN, 1, 1), None);
    assert_eq!(Rfc3339::days_from_civil(2000, 3, 1), Some(11_017));
}

#[test]
fn reads_errors_without_revealing_the_key() {
    let here = here();
    // The error bodies of the API reference.
    let unauthorized = response(401, r#"{"error":"Unauthorized","error_description":"ApiKey invalid. ApiKey not found."}"#);
    assert_eq!(here.parse_flow(&unauthorized), Err(TrafficError::Unauthorized("Unauthorized: ApiKey invalid. ApiKey not found.".into())));
    let forbidden = response(403, r#"{"error":"Forbidden","error_description":"These credentials do not authorize access."}"#);
    assert!(matches!(here.parse_incidents(&forbidden), Err(TrafficError::Unauthorized(_))));
    assert_eq!(here.parse_flow(&response(429, "Too Many Requests")), Err(TrafficError::RateLimited));
    let invalid = response(
        400,
        r#"{"title":"Malformed request","status":400,"code":"E608002","cause":"parameter `in` is missing","action":"request must adhere to API specification","correlationId":"87a6fe3b-6290-41db-8d79-edf4f4019a74"}"#,
    );
    assert_eq!(here.parse_flow(&invalid), Err(TrafficError::InvalidRequest("Malformed request: parameter `in` is missing: E608002".into())));
    let size = response(400, r#"{"title":"Malformed request","status":400,"code":"E608006","cause":"Invalid BBox size"}"#);
    assert_eq!(here.parse_flow(&size), Err(TrafficError::TooLarge));
    let internal = response(
        500,
        r#"{"title":"Internal server error","status":500,"code":"E608666","cause":"Internal Server Error","action":"please contact system administrator","correlationId":"895f87ec-5373-4940-b9c0-670f5b8da682"}"#,
    );
    assert!(matches!(here.parse_flow(&internal), Err(TrafficError::Server(m)) if m.contains("E608666")));
    assert_eq!(here.parse_flow(&response(502, "<html>Bad gateway</html>")), Err(TrafficError::Server("HTTP 502".into())));
    // A host may put the URL, with the key, in its network error.
    let url = here.flow_request(&berlin()).unwrap().url;
    let offline = Response::failed(format!("error sending request for url ({url}); raw {KEY}"));
    let Err(TrafficError::Network(message)) = here.parse_flow(&offline) else { panic!("not a network error") };
    assert!(!message.contains(KEY) && !message.contains(&Query::encode(KEY)), "{message}");
    assert!(message.contains("[apiKey]"));
    // An error body that echoes the key.
    let echo = response(401, &format!(r#"{{"error":"Unauthorized","error_description":"key {KEY} unknown"}}"#));
    assert_eq!(here.parse_flow(&echo), Err(TrafficError::Unauthorized("Unauthorized: key [apiKey] unknown".into())));
}

#[test]
fn reads_the_status_of_responses() {
    assert_eq!(TrafficError::of_response(&Response::ok("")), None);
    assert_eq!(TrafficError::of_response(&response(204, "")), None);
    assert_eq!(TrafficError::of_response(&Response::failed("offline")), Some(TrafficError::Network("offline".into())));
    assert_eq!(TrafficError::of_response(&response(0, "")), Some(TrafficError::Network(String::new())));
    assert!(matches!(TrafficError::of_response(&response(401, "")), Some(TrafficError::Unauthorized(_))));
    assert!(matches!(TrafficError::of_response(&response(403, "")), Some(TrafficError::Unauthorized(_))));
    assert_eq!(TrafficError::of_response(&response(429, "")), Some(TrafficError::RateLimited));
    assert!(matches!(TrafficError::of_response(&response(414, "")), Some(TrafficError::InvalidRequest(_))));
    assert!(matches!(TrafficError::of_response(&response(503, "")), Some(TrafficError::Server(_))));
    assert_eq!(TrafficError::RateLimited.to_string(), "rate limited");
}

#[test]
fn splits_large_bounds_into_accepted_pieces() {
    let here = here();
    // Tokyo and its surroundings: 2.5° by 1.2°.
    let bounds = Bounds { south: 35.0, west: 138.5, north: 36.2, east: 141.0 };
    let pieces = here.pieces(&bounds, 16).unwrap();
    assert_eq!(pieces.len(), 3 * 2);
    for piece in &pieces {
        assert!(here.max_area().admits(piece));
        assert!(here.flow_request(piece).is_ok(), "{piece:?}");
    }
    // They cover the bounds exactly, edge to edge, west to east and south to north.
    assert_eq!(pieces[0].south, bounds.south);
    assert_eq!(pieces[0].west, bounds.west);
    assert_eq!(pieces[5].north, bounds.north);
    assert_eq!(pieces[5].east, bounds.east);
    assert_eq!(pieces[0].east, pieces[1].west);
    assert_eq!(pieces[0].north, pieces[3].south);
    let area: f64 = pieces.iter().map(Bounds::area_km2).sum();
    assert!((area - bounds.area_km2()).abs() / bounds.area_km2() < 0.01);
    // Within the limit: one piece, the bounds themselves.
    assert_eq!(here.pieces(&berlin(), 1).unwrap(), vec![berlin()]);
    // Exactly one degree is more than the rounding margin allows: two pieces a side.
    assert_eq!(here.pieces(&Bounds { south: 0.0, west: 0.0, north: 1.0, east: 1.0 }, 4).unwrap().len(), 4);
    // A point is one piece.
    let point = Bounds { south: 1.0, west: 1.0, north: 1.0, east: 1.0 };
    assert_eq!(here.pieces(&point, 1).unwrap(), vec![point]);
    // Never more than the cap.
    assert_eq!(here.pieces(&bounds, 5), Err(TrafficError::TooLarge));
    assert_eq!(here.pieces(&bounds, 0), Err(TrafficError::TooLarge));
    let world = Bounds { south: -90.0, west: -180.0, north: 90.0, east: 180.0 };
    assert_eq!(here.pieces(&world, usize::MAX).unwrap().len(), 361 * 181);
    assert_eq!(here.pieces(&world, 1000), Err(TrafficError::TooLarge));
    assert!(matches!(here.pieces(&Bounds { south: 1.0, west: 0.0, north: 0.0, east: 1.0 }, 10), Err(TrafficError::InvalidRequest(_))));
}

struct ByArea;

impl TrafficProvider for ByArea {
    fn flow_request(&self, _: &Bounds) -> Result<fetch::Request, TrafficError> {
        Err(TrafficError::TooLarge)
    }
    fn parse_flow(&self, _: &Response) -> Result<Traffic, TrafficError> {
        Ok(Traffic::default())
    }
    fn incidents_request(&self, _: &Bounds) -> Result<fetch::Request, TrafficError> {
        Err(TrafficError::TooLarge)
    }
    fn parse_incidents(&self, _: &Response) -> Result<Traffic, TrafficError> {
        Ok(Traffic::default())
    }
    fn max_area(&self) -> AreaLimit {
        AreaLimit { max_width_deg: None, max_height_deg: Some(f64::NAN), max_area_km2: Some(1000.0) }
    }
}

#[test]
fn splits_by_area_for_providers_that_limit_it() {
    // About 111 km by 111 km at the equator: 12 300 km², so 16 pieces of at most 1 000 km².
    let bounds = Bounds { south: 0.0, west: 0.0, north: 1.0, east: 1.0 };
    let pieces = ByArea.pieces(&bounds, 64).unwrap();
    assert!(pieces.iter().all(|p| p.area_km2() <= 1000.0));
    assert_eq!(pieces.len(), 16);
    assert_eq!(ByArea.pieces(&bounds, 15), Err(TrafficError::TooLarge));
    // An unusable limit (NaN) does not apply.
    assert!(ByArea.max_area().admits(&Bounds { south: 0.0, west: 0.0, north: 0.1, east: 0.1 }));
}

#[test]
fn merges_pieces_without_repeating_roads_or_incidents() {
    let road = segment(Some(30.0), Some(50.0), Some(3.0), Some(0.9));
    let mut other_road = road.clone();
    other_road.shape[1] = Coordinate::new(0.0, 0.002);
    let incident = |id: &str| Incident {
        id: id.into(),
        kind: IncidentKind::Accident,
        criticality: Criticality::Major,
        road_closed: false,
        description: None,
        shape: Vec::new(),
        start_ms: None,
        end_ms: None,
    };
    let mut traffic = Traffic { flow: vec![road.clone()], incidents: vec![incident("a")], updated_ms: Some(2_000) };
    let mut repeated = road.clone();
    repeated.speed_kmh = Some(10.0);
    traffic.merge(Traffic { flow: vec![repeated, other_road.clone()], incidents: vec![incident("a"), incident("b")], updated_ms: Some(1_000) });
    assert_eq!(traffic.flow, vec![road, other_road]);
    assert_eq!(traffic.incidents.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(traffic.updated_ms, Some(1_000));
    traffic.merge(Traffic { updated_ms: Some(500), ..Traffic::default() });
    assert_eq!(traffic.updated_ms, Some(500));
    let mut empty = Traffic::default();
    empty.merge(Traffic { updated_ms: Some(7), ..Traffic::default() });
    assert_eq!(empty.updated_ms, Some(7));
    empty.merge(Traffic::default());
    assert_eq!(empty.updated_ms, Some(7));
}

#[test]
fn merges_the_answers_of_two_pieces_of_a_real_response() {
    let here = here();
    let body = include_str!("fixtures/flow_berlin.json");
    let mut traffic = here.parse_flow(&Response::ok(body)).unwrap();
    traffic.merge(here.parse_flow(&Response::ok(body)).unwrap());
    traffic.merge(here.parse_incidents(&Response::ok(include_str!("fixtures/incidents_leverkusen.json"))).unwrap());
    assert_eq!(traffic.flow.len(), 6);
    assert_eq!(traffic.incidents.len(), 3);
    assert_eq!(traffic.updated_ms, Some(1_625_147_583_000));
}

#[test]
fn keeps_the_model_serializable() {
    let traffic = here().parse_flow(&Response::ok(include_str!("fixtures/flow_berlin.json"))).unwrap();
    let json = serde_json::to_string(&traffic).unwrap();
    // serde_json reads floats back to within an ulp (exactly only with its `float_roundtrip`).
    let back = serde_json::from_str::<Traffic>(&json).unwrap();
    assert_eq!(back.flow.len(), traffic.flow.len());
    for (a, b) in back.flow.iter().zip(&traffic.flow) {
        assert_eq!(a.shape.len(), b.shape.len());
        assert_eq!(a.closed, b.closed);
        assert_eq!(a.confidence, b.confidence);
        assert!(a.speed_kmh.zip(b.speed_kmh).is_none_or(|(x, y)| (x - y).abs() < 1e-9));
    }
    assert_eq!(back.updated_ms, traffic.updated_ms);
    assert_eq!(serde_json::to_string(&Congestion::Stopped).unwrap(), r#""stopped""#);
    assert_eq!(serde_json::to_string(&IncidentKind::RoadClosure).unwrap(), r#""road_closure""#);
    assert_eq!(serde_json::to_string(&TrafficError::TooLarge).unwrap(), r#"{"kind":"too_large"}"#);
}
