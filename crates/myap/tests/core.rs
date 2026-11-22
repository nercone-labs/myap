//! The core through its protocol, as a platform drives it.

use fetch::Response as HttpResponse;
use geo::{Bounds, Coordinate};
use geocoding::Place;
use myap::*;
use routing::{Avoid, TravelMode};
use std::path::PathBuf;

const T0: i64 = 1_795_000_000_000;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("myap-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn start(dir: Option<&PathBuf>) -> Core {
    let mut core = Core::new();
    let r = core.handle(Input::Start { now: T0, data_dir: dir.map(|d| d.to_string_lossy().into_owned()), locale: "ja-JP".into(), utc_offset_minutes: 540 });
    assert!(r.error.is_none(), "{:?}", r.error);
    core
}

fn fetches(response: &myap::Response) -> Vec<(u64, fetch::Request)> {
    response
        .effects
        .iter()
        .filter_map(|e| match e {
            Effect::Fetch { id, request } => Some((*id, request.clone())),
            _ => None,
        })
        .collect()
}

fn host(request: &fetch::Request) -> String {
    request.host().unwrap_or_default()
}

fn tokyo_station() -> Place {
    let mut p = Place::new("osm:N1", geocoding::PlaceSource::OpenStreetMap, "東京駅", Coordinate::new(35.6812, 139.7671));
    p.address = Some("東京都千代田区".into());
    p
}

fn tower() -> Place {
    Place::new("osm:N2", geocoding::PlaceSource::OpenStreetMap, "東京タワー", Coordinate::new(35.6586, 139.7454))
}

fn google_place() -> Place {
    Place::new("google:0x1:0x2", geocoding::PlaceSource::Google, "Cafe", Coordinate::new(35.68, 139.76))
}

fn plan(core: &mut Core, now: i64, router: RouterChoice, mode: TravelMode) -> myap::Response {
    core.handle(Input::Plan {
        now,
        origin: Endpoint::Place { place: Box::new(tokyo_station()) },
        destination: Endpoint::Place { place: Box::new(tower()) },
        stops: vec![],
        mode,
        avoid: Avoid::default(),
        router,
    })
}

#[test]
fn nothing_reaches_google_while_it_is_off() {
    let dir = temp_dir("privacy");
    let mut core = start(Some(&dir));
    let snapshot = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap();
    assert!(!snapshot.settings.google.enabled, "Google is off until the user turns it on");
    assert!(!snapshot.style.google);
    assert!(snapshot.style.overlays.is_empty());

    // Every use chosen, but the switch of Google off.
    let mut settings = snapshot.settings.clone();
    settings.google = GoogleSettings {
        enabled: false,
        suggestions: true,
        search: true,
        place_details: true,
        directions: true,
        map_tiles: true,
        traffic_layer: true,
        navigation_traffic: true,
        keep_record: true,
    };
    settings.map = MapSource::GoogleSatellite;
    core.handle(Input::Configure { now: T0, settings });

    let mut all = Vec::new();
    let inputs = vec![
        Input::Suggest { now: T0 + 1, query: "ラーメン".into() },
        Input::Search { now: T0 + 2, query: "ラーメン".into() },
        Input::Select { now: T0 + 3, place: google_place() },
        Input::Pin { now: T0 + 4, coordinate: Coordinate::new(35.68, 139.76) },
    ];
    for input in inputs {
        all.extend(fetches(&core.handle(input)));
    }
    all.extend(fetches(&plan(&mut core, T0 + 5, RouterChoice::Automatic, TravelMode::Car)));
    all.extend(fetches(&plan(&mut core, T0 + 6, RouterChoice::Only(RouterKind::Google), TravelMode::Car)));
    let snapshot = core.handle(Input::Snapshot { now: T0 + 7 }).snapshot.unwrap();
    assert!(!all.is_empty());
    for (_, request) in &all {
        assert!(!host(request).contains("google"), "a request reached Google: {}", request.url);
    }
    // The map is not Google's either, and nothing was recorded.
    assert!(!snapshot.style.google);
    assert!(snapshot.style.json.is_none() || !snapshot.style.json.as_ref().unwrap().contains("google"));
    assert!(snapshot.google.records.is_empty());
    let directions = snapshot.directions.unwrap();
    assert!(!directions.available.contains(&RouterKind::Google));
}

#[test]
fn google_opens_a_session_first_and_records_what_it_sends() {
    let mut core = start(None);
    let mut settings = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap().settings;
    settings.google.enabled = true;
    settings.google.search = true;
    // Google alone: the open data is asked only when Google fails.
    settings.places.combine = false;
    core.handle(Input::Configure { now: T0, settings });
    let r = core.handle(Input::Search { now: T0 + 1, query: "ラーメン".into() });
    let sent = fetches(&r);
    // Only the page first; the search waits for its cookies.
    assert_eq!(sent.len(), 1);
    assert!(sent[0].1.url.starts_with("https://www.google.com/maps?hl=ja&gl=jp"));
    let page = HttpResponse { status: 200, body: "<html>".into(), headers: vec![("Set-Cookie".into(), "NID=abc; path=/".into())], error: None };
    let r = core.handle(Input::Fetched { now: T0 + 2, id: sent[0].0, response: page });
    let sent = fetches(&r);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].1.url.starts_with("https://www.google.com/search?"));
    assert_eq!(sent[0].1.header_value("Cookie"), Some("SOCS=CAESHAgBEhIaAB; NID=abc"));
    let snapshot = r.snapshot.unwrap();
    // The opening of the session is recorded too (most recent first).
    assert_eq!(snapshot.google.records.len(), 2);
    assert_eq!(snapshot.google.records[0].sent, "Google Maps (new session)");
    assert_eq!(snapshot.google.records[1].purpose, GoogleUse::Search);
    assert!(snapshot.google.records[1].sent.starts_with("ラーメン @ "));

    // Google refuses: the search falls back to OpenStreetMap data.
    let r = core.handle(Input::Fetched { now: T0 + 3, id: sent[0].0, response: HttpResponse { status: 429, ..Default::default() } });
    let next = fetches(&r);
    assert_eq!(next.len(), 1);
    assert_eq!(host(&next[0].1), "photon.komoot.io");
    assert!(r.snapshot.unwrap().google.last_error.is_some());

    // Clearing the record also ends the session: the next request opens a new one.
    core.handle(Input::ClearGoogleRecord { now: T0 + 4 });
    let r = core.handle(Input::Search { now: T0 + 5, query: "うどん".into() });
    let sent = fetches(&r);
    assert!(sent[0].1.url.starts_with("https://www.google.com/maps?"));
    assert_eq!(r.snapshot.unwrap().google.records.len(), 2);
}

#[test]
fn routers_fall_back_in_order() {
    let mut core = start(None);
    let r = plan(&mut core, T0, RouterChoice::Automatic, TravelMode::Foot);
    let sent = fetches(&r);
    // Google is off and no region is downloaded: Valhalla first, then OSRM.
    assert_eq!(sent.len(), 1);
    assert_eq!(host(&sent[0].1), "valhalla1.openstreetmap.de");
    let snapshot = r.snapshot.unwrap();
    assert_eq!(snapshot.directions.as_ref().unwrap().available, vec![RouterKind::Valhalla, RouterKind::Osrm]);
    assert!(snapshot.directions.unwrap().loading);

    let r = core.handle(Input::Fetched { now: T0 + 1, id: sent[0].0, response: HttpResponse::failed("timeout") });
    let sent = fetches(&r);
    assert_eq!(host(&sent[0].1), "routing.openstreetmap.de");
    assert!(sent[0].1.url.contains("/routed-foot/"));

    let body = include_str!("../../osrm/tests/fixtures/foot_tokyo.json");
    let r = core.handle(Input::Fetched { now: T0 + 2, id: sent[0].0, response: HttpResponse::ok(body) });
    let snapshot = r.snapshot.unwrap();
    let d = snapshot.directions.unwrap();
    assert!(!d.loading);
    assert_eq!(d.routes.len(), 2);
    assert_eq!(d.routes[0].router, RouterKind::Osrm);
    assert!(d.notice.as_deref().unwrap().contains("OSRM"), "the screen says another router answered");
    assert_eq!(d.routes[0].distance, "800 m");
    assert!(d.routes[0].steps[1].instruction.contains("中央通路"));
    // The map shows both routes, the selected last (on top), and the ends.
    let overlay: serde_json::Value = serde_json::from_str(&snapshot.overlay).unwrap();
    let kinds: Vec<&str> = overlay["features"].as_array().unwrap().iter().map(|f| f["properties"]["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["route_alternative", "route", "origin", "destination"]);
    assert!(snapshot.camera.unwrap().bounds.is_some());

    // Every router failing ends with the most telling error.
    let r = plan(&mut core, T0 + 3, RouterChoice::Automatic, TravelMode::Car);
    let first = fetches(&r)[0].0;
    let no_route = HttpResponse { status: 400, body: r#"{"error_code":442,"error":"No path could be found for input"}"#.into(), headers: vec![], error: None };
    let r = core.handle(Input::Fetched { now: T0 + 4, id: first, response: no_route });
    let second = fetches(&r)[0].0;
    let r = core.handle(Input::Fetched { now: T0 + 5, id: second, response: HttpResponse::failed("offline") });
    let d = r.snapshot.unwrap().directions.unwrap();
    assert!(!d.loading);
    assert_eq!(d.error.as_deref(), Some("経路が見つかりません"));
}

#[test]
fn answers_of_replaced_plans_are_ignored() {
    let mut core = start(None);
    let first = fetches(&plan(&mut core, T0, RouterChoice::Only(RouterKind::Osrm), TravelMode::Foot))[0].0;
    plan(&mut core, T0 + 1, RouterChoice::Only(RouterKind::Valhalla), TravelMode::Foot);
    let body = include_str!("../../osrm/tests/fixtures/foot_tokyo.json");
    let r = core.handle(Input::Fetched { now: T0 + 2, id: first, response: HttpResponse::ok(body) });
    let d = r.snapshot.unwrap().directions.unwrap();
    assert!(d.routes.is_empty() && d.loading);
}

/// A line of nodes going north from Tokyo Station, 0.001° apart, and a turn east.
fn overpass_json() -> String {
    let mut elements = Vec::new();
    for i in 0..10 {
        elements.push(format!(r#"{{"type":"node","id":{},"lat":{},"lon":139.7671}}"#, i + 1, 35.6812 + i as f64 * 0.001));
    }
    for i in 0..10 {
        elements.push(format!(r#"{{"type":"node","id":{},"lat":35.6902,"lon":{}}}"#, i + 101, 139.7681 + i as f64 * 0.001));
    }
    elements.push(r#"{"type":"way","id":1,"nodes":[1,2,3,4,5,6,7,8,9,10],"tags":{"highway":"primary","name":"北通り"}}"#.into());
    elements.push(r#"{"type":"way","id":2,"nodes":[10,101,102,103,104,105,106,107,108,109,110],"tags":{"highway":"primary","name":"東通り"}}"#.into());
    format!(r#"{{"elements":[{}]}}"#, elements.join(","))
}

#[test]
fn regions_download_build_and_route_offline() {
    let dir = temp_dir("regions");
    let mut core = start(Some(&dir));
    let bounds = Bounds { south: 35.68, west: 139.76, north: 35.70, east: 139.78 };
    let r = core.handle(Input::AddRegionArea { now: T0, name: "Test".into(), bounds });
    let (id, request) = fetches(&r).remove(0);
    assert!(request.url.starts_with("https://overpass-api.de/"));
    let path = request.save_to.clone().expect("saved to a file");
    // The platform downloads it to the file.
    std::fs::write(&path, overpass_json()).unwrap();
    let r = core.handle(Input::Fetched { now: T0 + 1, id, response: HttpResponse { status: 200, ..Default::default() } });
    let (job, build) = r
        .effects
        .iter()
        .find_map(|e| match e {
            Effect::BuildGraph { id, input, format, output, elevation } => {
                Some((*id, GraphBuild { input: input.clone(), format: *format, output: output.clone(), elevation: elevation.clone() }))
            }
            _ => None,
        })
        .expect("a build");
    assert_eq!(r.snapshot.unwrap().regions[0].status, RegionStatus::Building);
    // The platform builds it on another thread, through the same library.
    let result: Result<GraphStats, String> = serde_json::from_str(&GraphBuild::run_json(&serde_json::to_string(&build).unwrap())).unwrap();
    let r = core.handle(Input::GraphBuilt { now: T0 + 2, id: job, result });
    // Built: it routes at once, while the terrain tiles of its elevations are fetched.
    let tiles = fetches(&r);
    let region = &r.snapshot.unwrap().regions[0];
    assert_eq!(region.status, RegionStatus::Elevating);
    assert!(region.size_bytes > 0);
    assert!(!std::path::Path::new(&path).exists(), "the download is removed once built");
    assert!(!tiles.is_empty() && tiles.iter().all(|(_, t)| host(t) == "s3.amazonaws.com" && t.save_to.is_some()));
    // No tile came (the sea, an error): the region is ready without elevations.
    let mut pending: Vec<u64> = tiles.iter().map(|(id, _)| *id).collect();
    let mut build = None;
    let mut now = T0 + 2;
    while let Some(id) = pending.pop() {
        now += 1;
        let r = core.handle(Input::Fetched { now, id, response: HttpResponse { status: 404, ..Default::default() } });
        pending.extend(fetches(&r).into_iter().map(|(id, _)| id));
        build = build.or(r.effects.iter().find_map(|e| match e {
            Effect::BuildGraph { id, input, format, output, elevation } => {
                Some((*id, GraphBuild { input: input.clone(), format: *format, output: output.clone(), elevation: elevation.clone() }))
            }
            _ => None,
        }));
    }
    let (job, build) = build.expect("the build that adds elevations");
    assert_eq!(build.format, GraphFormat::Graph);
    let result: Result<GraphStats, String> = serde_json::from_str(&GraphBuild::run_json(&serde_json::to_string(&build).unwrap())).unwrap();
    assert_eq!(result.as_ref().unwrap().elevations, 0);
    let r = core.handle(Input::GraphBuilt { now: now + 1, id: job, result });
    let region = &r.snapshot.unwrap().regions[0];
    assert_eq!(region.status, RegionStatus::Ready);
    assert_eq!(region.elevations, None);

    // Routing in the region now works offline, at once, without any fetch.
    let r = core.handle(Input::Plan {
        now: T0 + 3,
        origin: Endpoint::Place { place: Box::new(tokyo_station()) },
        destination: Endpoint::Place { place: Box::new(Place::new("x", geocoding::PlaceSource::Coordinate, "East", Coordinate::new(35.6902, 139.775))) },
        stops: vec![],
        mode: TravelMode::Car,
        avoid: Avoid::default(),
        router: RouterChoice::Only(RouterKind::Myap),
    });
    assert!(fetches(&r).is_empty());
    let d = r.snapshot.unwrap().directions.unwrap();
    assert_eq!(d.routes.len(), 1, "{:?}", d.error);
    assert_eq!(d.routes[0].router, RouterKind::Myap);
    assert!(d.available.contains(&RouterKind::Myap));
    assert!(d.routes[0].steps.iter().any(|s| s.instruction.contains("東通り")));

    // The regions are kept across restarts; removing one removes its graph.
    let mut again = start(Some(&dir));
    let snapshot = again.handle(Input::Snapshot { now: T0 + 4 }).snapshot.unwrap();
    assert_eq!(snapshot.regions.len(), 1);
    let id = snapshot.regions[0].id.clone();
    again.handle(Input::RemoveRegion { now: T0 + 5, id: id.clone() });
    assert!(!dir.join("regions").join(format!("{id}.myapgraph")).exists());
}

#[test]
fn too_large_an_area_is_refused() {
    let dir = temp_dir("large");
    let mut core = start(Some(&dir));
    let r = core.handle(Input::AddRegionArea { now: T0, name: "Japan".into(), bounds: Bounds { south: 30.0, west: 130.0, north: 40.0, east: 140.0 } });
    assert!(fetches(&r).is_empty());
    let s = r.snapshot.unwrap();
    assert!(s.regions.is_empty());
    assert!(s.message.is_some());
}

#[test]
fn navigation_guides_and_reroutes() {
    let mut core = start(None);
    let sent = fetches(&plan(&mut core, T0, RouterChoice::Only(RouterKind::Osrm), TravelMode::Foot));
    let body = include_str!("../../osrm/tests/fixtures/foot_tokyo.json");
    let r = core.handle(Input::Fetched { now: T0 + 1, id: sent[0].0, response: HttpResponse::ok(body) });
    let route_points: Vec<Coordinate> = {
        let overlay: serde_json::Value = serde_json::from_str(&r.snapshot.unwrap().overlay).unwrap();
        let route = overlay["features"].as_array().unwrap().iter().find(|f| f["properties"]["kind"] == "route").unwrap().clone();
        route["geometry"]["coordinates"].as_array().unwrap().iter().map(|c| Coordinate::new(c[1].as_f64().unwrap(), c[0].as_f64().unwrap())).collect()
    };
    let location = |i: usize, t: i64| Location { coordinate: route_points[i], accuracy_m: Some(5.0), bearing: None, speed_mps: Some(1.3), time_ms: t };
    core.handle(Input::Located { now: T0 + 2, location: location(0, T0 + 2) });
    let r = core.handle(Input::StartNavigation { now: T0 + 3 });
    assert!(r.effects.contains(&Effect::Navigating { active: true }));
    assert!(r.effects.iter().any(|e| matches!(e, Effect::Speak { text, .. } if text.starts_with("案内を開始します"))));
    let nav = r.snapshot.unwrap().navigation.unwrap();
    assert_eq!(nav.state, NavigationState::Navigating);
    assert_eq!(nav.router, RouterKind::Osrm);

    // Walking the route: announcements, then the arrival.
    let mut spoken = Vec::new();
    let mut t = T0 + 4;
    for i in 0..route_points.len() {
        t += 1000;
        let r = core.handle(Input::Located { now: t, location: location(i, t) });
        for e in r.effects {
            if let Effect::Speak { text, .. } = e {
                spoken.push(text);
            }
        }
    }
    assert!(spoken.iter().any(|s| s.contains("右方向")), "{spoken:?}");
    assert_eq!(spoken.last().map(String::as_str), Some("目的地に到着しました"));
    let nav = core.handle(Input::Snapshot { now: t }).snapshot.unwrap().navigation.unwrap();
    assert_eq!(nav.state, NavigationState::Arrived);
    assert!(nav.remaining_m < 30.0 && nav.remaining_s >= 0.0);
    assert!(nav.arrival_ms >= t);

    // Back at the start and then far off the route: three fixes off it ask for a new route.
    let r = core.handle(Input::StopNavigation { now: t });
    assert!(r.effects.contains(&Effect::Navigating { active: false }));
    core.handle(Input::Located { now: t, location: location(0, t) });
    core.handle(Input::StartNavigation { now: t + 1 });
    let mut asked = Vec::new();
    for k in 0..4 {
        let off = Location {
            coordinate: Coordinate::new(35.69, 139.77),
            accuracy_m: Some(5.0),
            bearing: Some(0.0),
            speed_mps: Some(1.3),
            time_ms: t + 20_000 + k * 1000,
        };
        let r = core.handle(Input::Located { now: off.time_ms, location: off });
        asked.extend(fetches(&r));
    }
    assert_eq!(asked.len(), 1, "one reroute, not one per fix");
    assert!(asked[0].1.url.contains("139.770000,35.690000"), "from where the traveller is: {}", asked[0].1.url);
    let nav = core.handle(Input::Snapshot { now: t + 30_000 }).snapshot.unwrap().navigation.unwrap();
    assert_eq!(nav.state, NavigationState::Rerouting);
}

#[test]
fn nominatim_is_asked_once_a_second() {
    let mut core = start(None);
    let r1 = core.handle(Input::Pin { now: T0, coordinate: Coordinate::new(35.68, 139.76) });
    assert_eq!(fetches(&r1).len(), 1);
    let r2 = core.handle(Input::Pin { now: T0 + 200, coordinate: Coordinate::new(35.69, 139.76) });
    assert!(fetches(&r2).is_empty());
    assert!(r2.effects.contains(&Effect::Wake { at: T0 + 1100 }));
    let r3 = core.handle(Input::Tick { now: T0 + 1100 });
    assert_eq!(fetches(&r3).len(), 1);
    // The answer names the pin, which stays where it was dropped.
    let id = fetches(&r3)[0].0;
    let body = include_str!("../../geocoding/tests/fixtures/nominatim_reverse.json");
    let s = core.handle(Input::Fetched { now: T0 + 1200, id, response: HttpResponse::ok(body) }).snapshot.unwrap();
    let selected = s.selected.unwrap();
    assert!(!selected.loading);
    assert_eq!(selected.place.coordinate, Coordinate::new(35.69, 139.76));
    assert!(selected.place.name.contains("エレベーター"));
}

#[test]
fn files_are_kept_and_damage_is_survived() {
    let dir = temp_dir("files");
    let mut core = start(Some(&dir));
    let mut settings = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap().settings;
    settings.voice = false;
    settings.units = Units::Imperial;
    core.handle(Input::Configure { now: T0, settings });
    core.handle(Input::Save { now: T0, place: tower(), label: SavedLabel::Home });
    core.handle(Input::Save { now: T0, place: tokyo_station(), label: SavedLabel::Home });
    let s = start(Some(&dir)).handle(Input::Snapshot { now: T0 }).snapshot.unwrap();
    assert!(!s.settings.voice);
    assert!(s.imperial);
    assert_eq!(s.saved.len(), 1, "one home");
    assert_eq!(s.saved[0].place.name, "東京駅");

    std::fs::write(dir.join("settings.json"), "{ not json").unwrap();
    let mut core = Core::new();
    let r = core.handle(Input::Start { now: T0, data_dir: Some(dir.to_string_lossy().into_owned()), locale: "en-US".into(), utc_offset_minutes: 0 });
    assert!(r.error.is_some());
    assert!(dir.join("settings.json.broken").exists());
    assert!(r.snapshot.unwrap().settings.voice, "defaults after a damaged file");
}

#[test]
fn search_suggests_local_places_first_and_reads_coordinates() {
    let mut core = start(None);
    core.handle(Input::Save { now: T0, place: tower(), label: SavedLabel::Favorite });
    let r = core.handle(Input::Suggest { now: T0, query: "タワー".into() });
    assert_eq!(host(&fetches(&r)[0].1), "photon.komoot.io");
    let s = r.snapshot.unwrap();
    assert_eq!(s.search.suggestions[0].text, "東京タワー");
    let s = core.handle(Input::Search { now: T0 + 1, query: "35.6586, 139.7454".into() }).snapshot.unwrap();
    assert_eq!(s.selected.unwrap().place.coordinate, Coordinate::new(35.6586, 139.7454));
    // A short query asks no server.
    assert!(fetches(&core.handle(Input::Suggest { now: T0 + 2, query: "a".into() })).is_empty());
}

#[test]
fn json_protocol() {
    let mut core = Core::new();
    let response: serde_json::Value =
        serde_json::from_str(&core.handle_json(r#"{"type":"start","now":1,"data_dir":null,"locale":"en-GB","utc_offset_minutes":0}"#)).unwrap();
    assert!(response["snapshot"]["imperial"].as_bool().unwrap());
    assert_eq!(response["snapshot"]["language"], "en-US");
    let bad: serde_json::Value = serde_json::from_str(&core.handle_json("{\"type\":\"nonsense\"}")).unwrap();
    assert!(bad["error"].is_string());
}

#[test]
fn words() {
    use myap::words::Words;
    assert_eq!(Words::distance(43.0, false), "40 m");
    assert_eq!(Words::distance(312.0, false), "300 m");
    assert_eq!(Words::distance(1234.0, false), "1.2 km");
    assert_eq!(Words::distance(12_345.0, false), "12 km");
    assert_eq!(Words::distance(100.0, true), "350 ft");
    assert_eq!(Words::distance(5000.0, true), "3.1 mi");
    assert_eq!(Words::clock(0, 540), "09:00");
    assert_eq!(Words::clock(-60_000, 0), "23:59");
    let ja = Words::book(Language::System, "ja-JP");
    assert_eq!(ja.duration(3900.0), "1 時間 5 分");
    assert_eq!(ja.spoken_distance(300.0, Units::Metric, false), "300メートル");
    assert_eq!(ja.prepare("300メートル", "右方向です"), "300メートル先、右方向です");
    let en = Words::book(Language::English, "ja-JP");
    assert_eq!(en.duration(65.0 * 60.0), "1 h 5 min");
    assert_eq!(en.prepare("300 meters", "Turn right onto Main St"), "In 300 meters, turn right onto Main St");
    assert_eq!(en.spoken_distance(2000.0, Units::Metric, false), "2 kilometers");
}

#[test]
fn directions_from_my_location_wait_for_it() {
    let mut core = start(None);
    let r = core.handle(Input::Plan {
        now: T0,
        origin: Endpoint::MyLocation,
        destination: Endpoint::Place { place: Box::new(tower()) },
        stops: vec![],
        mode: TravelMode::Foot,
        avoid: Avoid::default(),
        router: RouterChoice::Only(RouterKind::Osrm),
    });
    assert!(fetches(&r).is_empty());
    let d = r.snapshot.unwrap().directions.unwrap();
    assert!(d.loading && d.error.is_none());
    assert_eq!(d.notice.as_deref(), Some("現在地を取得しています…"));
    let here = Location { coordinate: Coordinate::new(35.6812, 139.7671), accuracy_m: Some(10.0), bearing: None, speed_mps: None, time_ms: T0 + 1 };
    let r = core.handle(Input::Located { now: T0 + 1, location: here });
    let sent = fetches(&r);
    assert_eq!(sent.len(), 1, "planned once the location came");
    assert!(sent[0].1.url.contains("139.767100,35.681200;139.745400,35.658600"));
    // A second fix does not plan again.
    assert!(fetches(&core.handle(Input::Located { now: T0 + 2, location: here })).is_empty());
}

#[test]
fn a_new_search_closes_the_directions() {
    let mut core = start(None);
    plan(&mut core, T0, RouterChoice::Only(RouterKind::Osrm), TravelMode::Foot);
    let s = core.handle(Input::Search { now: T0 + 1, query: "ramen".into() }).snapshot.unwrap();
    assert!(s.directions.is_none());
    assert_eq!(s.search.query, "ramen");
}

#[test]
fn guidance_ends_by_itself_after_arriving() {
    let mut core = start(None);
    let sent = fetches(&plan(&mut core, T0, RouterChoice::Only(RouterKind::Osrm), TravelMode::Foot));
    let r = core.handle(Input::Fetched { now: T0 + 1, id: sent[0].0, response: HttpResponse::ok(include_str!("../../osrm/tests/fixtures/foot_tokyo.json")) });
    let overlay: serde_json::Value = serde_json::from_str(&r.snapshot.unwrap().overlay).unwrap();
    let route = overlay["features"].as_array().unwrap().iter().find(|f| f["properties"]["kind"] == "route").unwrap().clone();
    let points: Vec<Coordinate> =
        route["geometry"]["coordinates"].as_array().unwrap().iter().map(|c| Coordinate::new(c[1].as_f64().unwrap(), c[0].as_f64().unwrap())).collect();
    let at = |c: Coordinate, t: i64| Input::Located { now: t, location: Location { coordinate: c, accuracy_m: Some(5.0), bearing: None, speed_mps: Some(1.3), time_ms: t } };
    core.handle(at(points[0], T0 + 2));
    core.handle(Input::StartNavigation { now: T0 + 3 });
    let mut t = T0 + 4;
    for p in &points {
        t += 1_000;
        core.handle(at(*p, t));
    }
    let arrived = t;
    let s = core.handle(Input::Snapshot { now: t }).snapshot.unwrap();
    assert_eq!(s.navigation.unwrap().state, NavigationState::Arrived);
    // Still guiding a minute later (the default is two minutes), and woken for the end.
    let r = core.handle(at(*points.last().unwrap(), arrived + 60_000));
    assert!(r.snapshot.unwrap().navigation.is_some());
    assert!(r.effects.iter().any(|e| matches!(e, Effect::Wake { at } if *at <= arrived + 120_000)));
    // Two minutes after: ended, as if the traveller had ended it.
    let r = core.handle(Input::Tick { now: arrived + 120_000 });
    assert!(r.effects.contains(&Effect::Navigating { active: false }));
    assert!(r.snapshot.unwrap().navigation.is_none());

    // With 0, only the traveller ends it.
    let mut settings = core.handle(Input::Snapshot { now: t }).snapshot.unwrap().settings;
    settings.guidance.end_after_arrival_minutes = 0;
    core.handle(Input::Configure { now: t, settings });
    core.handle(at(points[0], arrived + 130_000));
    core.handle(Input::StartNavigation { now: arrived + 130_001 });
    let mut t = arrived + 130_002;
    for p in &points {
        t += 1_000;
        core.handle(at(*p, t));
    }
    let r = core.handle(Input::Tick { now: t + 3_600_000 });
    assert!(r.snapshot.unwrap().navigation.is_some());
}

#[test]
fn downloads_left_by_an_interrupted_run_are_removed_at_the_start() {
    let dir = temp_dir("leftovers");
    let downloads = dir.join("downloads");
    std::fs::create_dir_all(downloads.join("r1-dem")).unwrap();
    std::fs::write(downloads.join("r1.osm.pbf.part"), b"cut off").unwrap();
    std::fs::write(downloads.join("import-1.osm.pbf"), b"copied").unwrap();
    std::fs::write(downloads.join("r1-dem").join("12-1-2.png"), b"tile").unwrap();
    // A file the user keeps elsewhere in the directory of the core is not touched.
    std::fs::write(dir.join("keep.txt"), b"mine").unwrap();
    let _core = start(Some(&dir));
    assert_eq!(std::fs::read_dir(&downloads).map(|d| d.count()).unwrap_or(0), 0);
    assert!(dir.join("keep.txt").exists());
}
