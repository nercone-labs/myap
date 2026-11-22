//! The core with live data: traffic for Myap Router, faster routes during guidance, the position
//! estimated without fixes, areas to avoid, the night, and elevations added to a region.

use fetch::Response as HttpResponse;
use geo::{Bounds, Coordinate};
use geocoding::Place;
use myap::*;
use routing::{Avoid, TravelMode};
use std::path::PathBuf;

const T0: i64 = 1_795_000_000_000;
const WEST: f64 = 139.760;
const SOUTH: f64 = 35.680;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("myap-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fetches(response: &myap::Response) -> Vec<(u64, fetch::Request)> {
    response.effects.iter().filter_map(|e| if let Effect::Fetch { id, request } = e { Some((*id, request.clone())) } else { None }).collect()
}

fn builds(response: &myap::Response) -> Vec<(u64, GraphBuild)> {
    response
        .effects
        .iter()
        .filter_map(|e| match e {
            Effect::BuildGraph { id, input, format, output, elevation } => {
                Some((*id, GraphBuild { input: input.clone(), format: *format, output: output.clone(), elevation: elevation.clone() }))
            }
            _ => None,
        })
        .collect()
}

fn point(column: f64, row: f64) -> Coordinate {
    Coordinate::new(SOUTH + row * 0.001, WEST + column * 0.001)
}

/// A direct primary road east from A (column 0) to B (column 20), and a detour by a road three
/// rows north.
fn network() -> String {
    let mut elements = Vec::new();
    let mut direct = Vec::new();
    for c in 0..=20 {
        elements.push(format!(r#"{{"type":"node","id":{},"lat":{},"lon":{}}}"#, c + 1, SOUTH, WEST + c as f64 * 0.001));
        direct.push((c + 1).to_string());
    }
    elements.push(format!(r#"{{"type":"node","id":100,"lat":{},"lon":{WEST}}}"#, SOUTH + 0.003));
    elements.push(format!(r#"{{"type":"node","id":101,"lat":{},"lon":{}}}"#, SOUTH + 0.003, WEST + 0.020));
    elements.push(format!(r#"{{"type":"way","id":1,"nodes":[{}],"tags":{{"highway":"primary","name":"本通り"}}}}"#, direct.join(",")));
    elements.push(r#"{"type":"way","id":2,"nodes":[1,100,101,21],"tags":{"highway":"primary","name":"裏通り"}}"#.into());
    format!(r#"{{"elements":[{}]}}"#, elements.join(","))
}

/// A core with a region of the network ready, no elevations, and the settings changed by `f`.
fn with_region(name: &str, f: impl Fn(&mut Settings)) -> (Core, PathBuf) {
    let dir = temp_dir(name);
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: Some(dir.to_string_lossy().into_owned()), locale: "ja-JP".into(), utc_offset_minutes: 540 });
    let mut settings = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap().settings;
    settings.elevation = ElevationSource::Off;
    f(&mut settings);
    core.handle(Input::Configure { now: T0, settings });
    let bounds = Bounds { south: SOUTH - 0.002, west: WEST - 0.002, north: SOUTH + 0.005, east: WEST + 0.022 };
    let r = core.handle(Input::AddRegionArea { now: T0, name: "Test".into(), bounds });
    let (id, request) = fetches(&r).remove(0);
    std::fs::write(request.save_to.unwrap(), network()).unwrap();
    let r = core.handle(Input::Fetched { now: T0, id, response: HttpResponse { status: 200, ..Default::default() } });
    let (job, build) = builds(&r).remove(0);
    let result: Result<GraphStats, String> = serde_json::from_str(&GraphBuild::run_json(&serde_json::to_string(&build).unwrap())).unwrap();
    let r = core.handle(Input::GraphBuilt { now: T0, id: job, result });
    assert_eq!(r.snapshot.unwrap().regions[0].status, RegionStatus::Ready);
    (core, dir)
}

fn place(c: Coordinate) -> Endpoint {
    Endpoint::Place { place: Box::new(Place::new(format!("p{}", c.longitude), geocoding::PlaceSource::Coordinate, "P", c)) }
}

fn plan(core: &mut Core, now: i64) -> myap::Response {
    core.handle(Input::Plan {
        now,
        origin: place(point(0.0, 0.0)),
        destination: place(point(20.0, 0.0)),
        stops: vec![],
        mode: TravelMode::Car,
        avoid: Avoid::default(),
        router: RouterChoice::Only(RouterKind::Myap),
    })
}

/// A flow answer of HERE for the direct road, eastward, at `speed` m/s of 14 free.
fn flow(speed: f64, closed: bool) -> String {
    let points: Vec<String> = (0..=20).map(|c| format!(r#"{{"lat":{SOUTH},"lng":{}}}"#, WEST + c as f64 * 0.001)).collect();
    let traversability = if closed { "closed" } else { "open" };
    format!(
        r#"{{"sourceUpdated":"2026-11-22T00:00:00Z","results":[{{"location":{{"length":1800,"shape":{{"links":[{{"points":[{}],"length":1800}}]}}}},"currentFlow":{{"speed":{speed},"speedUncapped":{speed},"freeFlow":14,"jamFactor":9.5,"confidence":0.9,"traversability":"{traversability}"}}}}]}}"#,
        points.join(",")
    )
}

const NO_INCIDENTS: &str = r#"{"sourceUpdated":"2026-11-22T00:00:00Z","results":[]}"#;

/// Answers the fetches of HERE with a flow and no incidents.
fn answer_traffic(core: &mut Core, response: &myap::Response, now: i64, body: &str) -> myap::Response {
    let sent = fetches(response);
    assert!(!sent.is_empty());
    let mut last = None;
    for (id, request) in sent {
        assert_eq!(request.host().as_deref(), Some("data.traffic.hereapi.com"));
        let answer = if request.url.contains("/v7/flow") { body } else { NO_INCIDENTS };
        last = Some(core.handle(Input::Fetched { now, id, response: HttpResponse::ok(answer) }));
    }
    last.unwrap()
}

fn max_latitude(core: &mut Core, now: i64) -> f64 {
    let overlay: serde_json::Value = serde_json::from_str(&core.handle(Input::Snapshot { now }).snapshot.unwrap().overlay).unwrap();
    let route = overlay["features"].as_array().unwrap().iter().find(|f| f["properties"]["kind"] == "route").unwrap().clone();
    route["geometry"]["coordinates"].as_array().unwrap().iter().map(|c| c[1].as_f64().unwrap()).fold(f64::MIN, f64::max)
}

#[test]
fn myap_router_waits_for_the_traffic_and_routes_round_a_jam() {
    let (mut core, _dir) = with_region("traffic", |s| s.traffic.here_api_key = "test-key".into());
    let r = plan(&mut core, T0 + 1);
    let d = r.snapshot.as_ref().unwrap().directions.clone().unwrap();
    assert!(d.loading && d.routes.is_empty(), "Myap Router waits for the traffic of now");
    // The key goes only to HERE, in its parameter.
    assert!(fetches(&r).iter().all(|(_, q)| q.url.contains("apiKey=test-key")));
    let r = answer_traffic(&mut core, &r, T0 + 2, &flow(1.0, false));
    let d = r.snapshot.unwrap().directions.unwrap();
    assert_eq!(d.routes.len(), 1, "{:?}", d.error);
    assert!(d.routes[0].traffic);
    assert!(max_latitude(&mut core, T0 + 3) > SOUTH + 0.002, "the jam of the direct road sends the route round it");
}

#[test]
fn without_a_key_nothing_goes_to_here() {
    let (mut core, _dir) = with_region("nokey", |_| {});
    let r = plan(&mut core, T0 + 1);
    assert!(fetches(&r).is_empty());
    let d = r.snapshot.unwrap().directions.unwrap();
    assert!(!d.routes.is_empty());
    assert!(d.routes.iter().all(|r| !r.traffic));
}

#[test]
fn a_faster_route_is_offered_during_guidance_and_taken() {
    let (mut core, _dir) = with_region("faster", |s| {
        s.traffic.here_api_key = "test-key".into();
        s.guidance.faster_route_minutes = 1;
    });
    let r = plan(&mut core, T0 + 1);
    answer_traffic(&mut core, &r, T0 + 2, NO_INCIDENTS);
    assert!(max_latitude(&mut core, T0 + 3) < SOUTH + 0.0005, "free roads: the direct route");
    let fix = |c: f64, t: i64| Location { coordinate: point(c, 0.0), accuracy_m: Some(5.0), bearing: Some(90.0), speed_mps: Some(12.0), time_ms: t };
    core.handle(Input::Located { now: T0 + 4, location: fix(0.0, T0 + 4) });
    core.handle(Input::StartNavigation { now: T0 + 5 });
    // Five minutes later the traffic is checked: the road ahead is closed.
    let t = T0 + 5 + 5 * 60_000;
    let r = core.handle(Input::Located { now: t, location: fix(1.0, t) });
    let r = answer_traffic(&mut core, &r, t + 1, &flow(0.5, true));
    let nav = r.snapshot.unwrap().navigation.unwrap();
    let offer = nav.faster_route.expect("a faster route is offered");
    assert!(offer.saving_minutes >= 1);
    assert!(r.effects.iter().any(|e| matches!(e, Effect::Speak { text, .. } if text.contains("早いルート"))));
    let r = core.handle(Input::AcceptFasterRoute { now: t + 2 });
    assert!(r.snapshot.unwrap().navigation.unwrap().faster_route.is_none());
    assert!(max_latitude(&mut core, t + 3) > SOUTH + 0.002, "the route followed now goes round");
}

#[test]
fn the_position_is_estimated_in_a_tunnel() {
    let (mut core, _dir) = with_region("tunnel", |_| {});
    plan(&mut core, T0 + 1);
    let fix = |c: f64, t: i64| Location { coordinate: point(c, 0.0), accuracy_m: Some(5.0), bearing: Some(90.0), speed_mps: Some(10.0), time_ms: t };
    core.handle(Input::Located { now: T0 + 2, location: fix(0.0, T0 + 2) });
    core.handle(Input::StartNavigation { now: T0 + 3 });
    let r = core.handle(Input::Located { now: T0 + 4, location: fix(1.0, T0 + 4) });
    assert!(r.effects.iter().any(|e| matches!(e, Effect::Wake { .. })), "a wake-up to estimate if no fix comes");
    let before = r.snapshot.unwrap().navigation.unwrap().progress;
    // No fix for ten seconds: the position moves on at 10 m/s.
    let r = core.handle(Input::Tick { now: T0 + 4 + 10_000 });
    let nav = r.snapshot.unwrap().navigation.unwrap();
    assert!(nav.estimated);
    assert!(nav.progress > before + 0.03, "{} {}", before, nav.progress);
    // A fix again: not estimated.
    let r = core.handle(Input::Located { now: T0 + 15_000, location: fix(3.0, T0 + 15_000) });
    assert!(!r.snapshot.unwrap().navigation.unwrap().estimated);
}

#[test]
fn areas_to_avoid_are_kept_and_used() {
    let (mut core, dir) = with_region("areas", |_| {});
    plan(&mut core, T0 + 1);
    assert!(max_latitude(&mut core, T0 + 2) < SOUTH + 0.0005);
    let r = core.handle(Input::AvoidArea { now: T0 + 3, name: "工事".into(), center: point(10.0, 0.0), radius_m: 50.0, hours: Some(2) });
    let snapshot = r.snapshot.unwrap();
    assert_eq!(snapshot.settings.routing.avoid_areas.len(), 1);
    assert!(snapshot.overlay.contains("avoid_area"));
    assert!(max_latitude(&mut core, T0 + 4) > SOUTH + 0.002, "planned again round the area");
    // Kept in the settings across a restart.
    let mut again = Core::new();
    let s = again
        .handle(Input::Start { now: T0 + 5, data_dir: Some(dir.to_string_lossy().into_owned()), locale: "ja-JP".into(), utc_offset_minutes: 540 })
        .snapshot
        .unwrap();
    assert_eq!(s.settings.routing.avoid_areas.len(), 1);
    // After its hours it is not avoided.
    let id = s.settings.routing.avoid_areas[0].id.clone();
    plan(&mut again, T0 + 3 * 3_600_000);
    assert!(max_latitude(&mut again, T0 + 3 * 3_600_000) < SOUTH + 0.0005);
    let r = again.handle(Input::RemoveAvoidArea { now: T0 + 3 * 3_600_000, id });
    assert!(r.snapshot.unwrap().settings.routing.avoid_areas.is_empty());
}

#[test]
fn the_map_is_dark_after_sunset_when_asked() {
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: None, locale: "ja-JP".into(), utc_offset_minutes: 540 });
    let mut settings = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap().settings;
    settings.guidance.night_mode = NightMode::Sun;
    core.handle(Input::Configure { now: T0, settings });
    let tokyo = Coordinate::new(35.68, 139.77);
    // 2026-11-25 03:00 UTC is noon in Tokyo; 15:00 UTC midnight.
    let noon = 1_795_575_600_000;
    let at = |core: &mut Core, now: i64| {
        core.handle(Input::Located { now, location: Location { coordinate: tokyo, accuracy_m: Some(5.0), bearing: None, speed_mps: None, time_ms: now } })
            .snapshot
            .unwrap()
            .style
    };
    assert!(at(&mut core, noon).url.unwrap().ends_with("liberty"));
    assert!(at(&mut core, noon + 12 * 3_600_000).url.unwrap().ends_with("dark"));
}

/// A PNG of one colour (RGB, 8 bits), as the terrain tiles are.
#[allow(clippy::same_item_push)]
fn png(width: u32, rgb: [u8; 3]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    };
    let mut raw = Vec::new();
    for _ in 0..width {
        raw.push(0);
        for _ in 0..width {
            raw.extend_from_slice(&rgb);
        }
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[test]
fn elevations_are_downloaded_and_added_to_a_region() {
    let dir = temp_dir("elevation");
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: Some(dir.to_string_lossy().into_owned()), locale: "ja-JP".into(), utc_offset_minutes: 540 });
    let bounds = Bounds { south: SOUTH - 0.002, west: WEST - 0.002, north: SOUTH + 0.005, east: WEST + 0.022 };
    let r = core.handle(Input::AddRegionArea { now: T0, name: "Test".into(), bounds });
    let (id, request) = fetches(&r).remove(0);
    std::fs::write(request.save_to.unwrap(), network()).unwrap();
    let r = core.handle(Input::Fetched { now: T0, id, response: HttpResponse { status: 200, ..Default::default() } });
    let (job, build) = builds(&r).remove(0);
    let result: Result<GraphStats, String> = serde_json::from_str(&GraphBuild::run_json(&serde_json::to_string(&build).unwrap())).unwrap();
    let mut r = core.handle(Input::GraphBuilt { now: T0, id: job, result });
    // Terrarium: 100 m is 32868 = 128·256 + 100, so (128, 100, 0).
    let tile = png(256, [128, 100, 0]);
    let mut pending = fetches(&r);
    assert!(!pending.is_empty());
    assert!(pending.len() <= 4, "a few tiles at a time");
    let mut elevation_build = None;
    while let Some((id, request)) = pending.pop() {
        assert!(request.url.starts_with("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/12/"));
        std::fs::write(request.save_to.as_ref().unwrap(), &tile).unwrap();
        r = core.handle(Input::Fetched { now: T0 + 1, id, response: HttpResponse { status: 200, ..Default::default() } });
        pending.extend(fetches(&r));
        elevation_build = elevation_build.or(builds(&r).into_iter().next());
    }
    let (job, build) = elevation_build.expect("a build adding the elevations");
    let result: Result<GraphStats, String> = serde_json::from_str(&GraphBuild::run_json(&serde_json::to_string(&build).unwrap())).unwrap();
    assert!(result.as_ref().unwrap().elevations > 0);
    let r = core.handle(Input::GraphBuilt { now: T0 + 2, id: job, result });
    let region = &r.snapshot.unwrap().regions[0];
    assert_eq!(region.status, RegionStatus::Ready);
    assert_eq!(region.elevations, Some(ElevationSource::Terrarium));
    let d = plan(&mut core, T0 + 3).snapshot.unwrap().directions.unwrap();
    let route = &d.routes[0];
    assert!(!route.profile.is_empty() && route.profile.iter().all(|h| (h - 100.0).abs() < 0.5), "{:?}", route.profile);
    assert!(!std::path::Path::new(&build.elevation.unwrap().directory).exists(), "the tiles are removed once added");
}
