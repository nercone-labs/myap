//! Problems found by the independent review, each reproduced here before it was fixed.
use fetch::Response as HttpResponse;
use geo::{Coordinate, Polyline};
use geocoding::Place;
use myap::*;
use routing::{Avoid, TravelMode};

const T0: i64 = 1_795_000_000_000;

fn start() -> Core {
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: None, locale: "en-US".into(), utc_offset_minutes: 0 });
    core
}

fn fetches(response: &myap::Response) -> Vec<(u64, fetch::Request)> {
    response.effects.iter().filter_map(|e| if let Effect::Fetch { id, request } = e { Some((*id, request.clone())) } else { None }).collect()
}

fn place(id: &str, lat: f64, lon: f64) -> Endpoint {
    Endpoint::Place { place: Box::new(Place::new(id, geocoding::PlaceSource::Coordinate, id, Coordinate::new(lat, lon))) }
}

fn line(a: Coordinate, b: Coordinate, n: usize) -> Vec<Coordinate> {
    (0..=n)
        .map(|i| Coordinate::new(a.latitude + (b.latitude - a.latitude) * i as f64 / n as f64, a.longitude + (b.longitude - a.longitude) * i as f64 / n as f64))
        .collect()
}

/// An OSRM answer with one leg per pair of `legs` (each a straight line).
fn osrm(legs: &[(Coordinate, Coordinate)]) -> String {
    let mut out = Vec::new();
    let mut total = 0.0;
    for (a, b) in legs {
        let d = a.distance_m(b);
        total += d;
        let pts = line(*a, *b, 10);
        let g = Polyline::encode(&pts, 6);
        let g_end = Polyline::encode(&[*b], 6);
        out.push(format!(
            r#"{{"distance":{d},"duration":{t},"summary":"","steps":[
              {{"distance":{d},"duration":{t},"geometry":"{g}","name":"Road","mode":"walking","maneuver":{{"location":[{alon},{alat}],"type":"depart"}}}},
              {{"distance":0,"duration":0,"geometry":"{g_end}","name":"Road","mode":"walking","maneuver":{{"location":[{blon},{blat}],"type":"arrive"}}}}]}}"#,
            t = d / 1.3,
            g = g.replace('\\', "\\\\"),
            g_end = g_end.replace('\\', "\\\\"),
            alon = a.longitude,
            alat = a.latitude,
            blon = b.longitude,
            blat = b.latitude
        ));
    }
    format!(r#"{{"code":"Ok","routes":[{{"distance":{total},"duration":{},"legs":[{}]}}]}}"#, total / 1.3, out.join(","))
}

fn loc(lat: f64, lon: f64, t: i64) -> Location {
    Location { coordinate: Coordinate::new(lat, lon), accuracy_m: Some(5.0), bearing: None, speed_mps: Some(1.3), time_ms: t }
}

#[test]
fn a_reroute_after_a_stop_does_not_return_to_it() {
    let mut core = start();
    let (o, s, d) = (Coordinate::new(35.0, 139.0), Coordinate::new(35.01, 139.0), Coordinate::new(35.02, 139.0));
    let r = core.handle(Input::Plan {
        now: T0,
        origin: place("o", o.latitude, o.longitude),
        destination: place("d", d.latitude, d.longitude),
        stops: vec![place("s", s.latitude, s.longitude)],
        mode: TravelMode::Foot,
        avoid: Avoid::default(),
        router: RouterChoice::Only(RouterKind::Osrm),
    });
    let id = fetches(&r)[0].0;
    let r = core.handle(Input::Fetched { now: T0 + 1, id, response: HttpResponse::ok(osrm(&[(o, s), (s, d)])) });
    assert_eq!(r.snapshot.unwrap().directions.unwrap().routes.len(), 1);
    core.handle(Input::Located { now: T0 + 2, location: loc(35.0, 139.0, T0 + 2) });
    core.handle(Input::StartNavigation { now: T0 + 3 });
    // walk past the stop
    let mut t = T0 + 4;
    for i in 0..=15 {
        t += 1000;
        core.handle(Input::Located { now: t, location: loc(35.0 + i as f64 * 0.001, 139.0, t) });
    }
    // off route, 3 fixes
    let mut asked = Vec::new();
    for k in 0..3 {
        t += 1000;
        asked.extend(fetches(&core.handle(Input::Located { now: t, location: loc(35.015, 139.01, t + k) })));
    }
    assert_eq!(asked.len(), 1);
    assert!(!asked[0].1.url.contains("139.000000,35.010000"), "first reroute skips the stop");
    let here = Coordinate::new(35.015, 139.01);
    let r = core.handle(Input::Fetched { now: t + 1, id: asked[0].0, response: HttpResponse::ok(osrm(&[(here, d)])) });
    assert!(r.snapshot.unwrap().navigation.is_some());
    // off route again, later
    t += 20_000;
    let mut asked = Vec::new();
    for _ in 0..3 {
        t += 1000;
        asked.extend(fetches(&core.handle(Input::Located { now: t, location: loc(35.012, 139.03, t) })));
    }
    assert_eq!(asked.len(), 1);
    assert!(!asked[0].1.url.contains("139.000000,35.010000"), "the second reroute goes back to the stop already passed: {}", asked[0].1.url);
}

#[test]
fn a_route_without_steps_is_refused() {
    let mut core = start();
    let r = core.handle(Input::Plan {
        now: T0,
        origin: place("o", 35.0, 139.0),
        destination: place("d", 35.01, 139.0),
        stops: vec![],
        mode: TravelMode::Foot,
        avoid: Avoid::default(),
        router: RouterChoice::Only(RouterKind::Valhalla),
    });
    let id = fetches(&r)[0].0;
    let shape = Polyline::encode(&line(Coordinate::new(35.0, 139.0), Coordinate::new(35.01, 139.0), 5), 6);
    let body = format!(
        r#"{{"trip":{{"legs":[{{"shape":{},"summary":{{"length":1.1,"time":800}},"maneuvers":[]}}],"summary":{{"length":1.1,"time":800}}}}}}"#,
        serde_json::to_string(&shape).unwrap()
    );
    let r = core.handle(Input::Fetched { now: T0 + 1, id, response: HttpResponse::ok(body) });
    let d = r.snapshot.unwrap().directions.unwrap();
    assert!(d.routes.is_empty(), "a route without steps cannot be followed");
    assert!(!d.loading && d.error.is_some());
    // Nothing to start; nothing panics.
    let r = core.handle(Input::StartNavigation { now: T0 + 2 });
    assert!(r.snapshot.unwrap().navigation.is_none());
}

fn google_on(core: &mut Core, f: impl Fn(&mut GoogleSettings)) -> Settings {
    let mut settings = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap().settings;
    settings.google.enabled = true;
    f(&mut settings.google);
    core.handle(Input::Configure { now: T0, settings: settings.clone() });
    settings
}

#[test]
fn a_queued_google_request_is_dropped_when_its_use_is_turned_off() {
    let mut core = start();
    let mut settings = google_on(&mut core, |g| g.search = true);
    let r = core.handle(Input::Search { now: T0 + 1, query: "ramen".into() });
    // The session page (and OpenStreetMap, searched too when the sources are combined).
    let sent: Vec<_> = fetches(&r).into_iter().filter(|(_, req)| req.url.contains("google.com")).collect();
    assert_eq!(sent.len(), 1);
    settings.google.search = false; // the user turns search off (Google stays on)
    core.handle(Input::Configure { now: T0 + 2, settings });
    let page = HttpResponse { status: 200, body: "<html>".into(), headers: vec![], error: None };
    let r = core.handle(Input::Fetched { now: T0 + 3, id: sent[0].0, response: page });
    for (_, req) in fetches(&r) {
        assert!(!req.url.contains("google.com/search"), "a search was sent to Google after Search was turned off: {}", req.url);
    }
}

#[test]
fn turning_google_off_while_directions_wait_falls_back() {
    let mut core = start();
    let mut settings = google_on(&mut core, |g| g.directions = true);
    let r = core.handle(Input::Plan {
        now: T0 + 1,
        origin: place("o", 35.0, 139.0),
        destination: place("d", 35.01, 139.0),
        stops: vec![],
        mode: TravelMode::Car,
        avoid: Avoid::default(),
        router: RouterChoice::Automatic,
    });
    assert_eq!(fetches(&r).len(), 1);
    settings.google.enabled = false;
    let r = core.handle(Input::Configure { now: T0 + 2, settings });
    let d = r.snapshot.clone().unwrap().directions.unwrap();
    let next = fetches(&r);
    assert_eq!(next.len(), 1, "the next router is asked");
    assert!(!next[0].1.host().unwrap().contains("google"));
    assert!(d.loading);
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("myap-review-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn removing_a_region_during_its_build_leaves_no_graph() {
    let dir = temp_dir("orphan");
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: Some(dir.to_string_lossy().into_owned()), locale: "en-US".into(), utc_offset_minutes: 0 });
    let import = dir.join("downloads").join("import-1.osm.pbf");
    std::fs::create_dir_all(import.parent().unwrap()).unwrap();
    std::fs::write(&import, b"pbf").unwrap();
    let r = core.handle(Input::AddRegionFile { now: T0, name: "x".into(), path: import.to_string_lossy().into_owned() });
    let (job, output) = r.effects.iter().find_map(|e| if let Effect::BuildGraph { id, output, .. } = e { Some((*id, output.clone())) } else { None }).unwrap();
    let id = r.snapshot.unwrap().regions[0].id.clone();
    core.handle(Input::RemoveRegion { now: T0 + 1, id });
    assert!(!import.exists(), "the copy of the imported file is removed with its region");
    // The build, which could not be stopped, ends and writes its graph: it is removed.
    std::fs::write(&output, b"graph").unwrap();
    let stats = GraphStats { nodes: 1, edges: 1, bounds: geo::Bounds { south: 0.0, west: 0.0, north: 1.0, east: 1.0 }, elevations: 0, size_bytes: 5 };
    core.handle(Input::GraphBuilt { now: T0 + 2, id: job, result: Ok(stats) });
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn an_imported_file_is_removed_once_built_but_nothing_outside_the_downloads() {
    let dir = temp_dir("import");
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: Some(dir.to_string_lossy().into_owned()), locale: "en-US".into(), utc_offset_minutes: 0 });
    let outside = dir.join("elsewhere.osm.pbf");
    std::fs::write(&outside, b"pbf").unwrap();
    let r = core.handle(Input::AddRegionFile { now: T0, name: "x".into(), path: outside.to_string_lossy().into_owned() });
    let job = r.effects.iter().find_map(|e| if let Effect::BuildGraph { id, .. } = e { Some(*id) } else { None }).unwrap();
    core.handle(Input::GraphBuilt { now: T0 + 1, id: job, result: Err("no roads".into()) });
    assert!(outside.exists(), "the core removes nothing outside its downloads");
}

#[test]
fn stale_and_poor_fixes_are_ignored() {
    let mut core = start();
    core.handle(Input::Located { now: T0, location: loc(35.0, 139.0, T0) });
    let older = Location { time_ms: T0 - 60_000, ..loc(36.0, 140.0, 0) };
    let s = core.handle(Input::Located { now: T0 + 1, location: older }).snapshot.unwrap();
    assert_eq!(s.location.unwrap().coordinate, Coordinate::new(35.0, 139.0));
    let poor = Location { accuracy_m: Some(800.0), ..loc(35.1, 139.1, T0 + 2_000) };
    let s = core.handle(Input::Located { now: T0 + 2, location: poor }).snapshot.unwrap();
    assert_eq!(s.location.unwrap().coordinate, Coordinate::new(35.0, 139.0));
    // Once the good fix is old, a poor one is better than none.
    let later = Location { accuracy_m: Some(800.0), ..loc(35.1, 139.1, T0 + 20_000) };
    let s = core.handle(Input::Located { now: T0 + 20, location: later }).snapshot.unwrap();
    assert_eq!(s.location.unwrap().coordinate, Coordinate::new(35.1, 139.1));
}
