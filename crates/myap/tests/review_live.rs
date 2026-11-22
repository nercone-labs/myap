//! Problems found by the independent review of live traffic, faster routes, dead reckoning and
//! the lanes of one-way roads, each checked here so that it cannot come back.

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
    let dir = std::env::temp_dir().join(format!("myap-review-live-{name}-{}", std::process::id()));
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


const NO_INCIDENTS: &str = r#"{"sourceUpdated":"2026-11-22T00:00:00Z","results":[]}"#;



use myap_router::{Graph, GraphBuilder};
use osm::{Element, Node, Tags, Way};
use routing::{Leg, Maneuver, ManeuverKind, Modifier, Route, Step};

fn tags(pairs: &[(&str, &str)]) -> Tags {
    Tags(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

#[test]
fn estimate_runs_past_maneuvers() {
    // A straight line east 2 km, with "turns" every 200 m.
    let geometry: Vec<Coordinate> = (0..=20).map(|i| Coordinate::new(SOUTH, WEST + i as f64 * 0.0011)).collect();
    let mut steps = vec![Step::new(Maneuver::new(ManeuverKind::Depart, None, geometry[0]), 200.0, 20.0, 0, 2)];
    for k in 1..10 {
        steps.push(Step::new(Maneuver::new(ManeuverKind::Turn, Some(Modifier::Right), geometry[2 * k]), 200.0, 20.0, 2 * k, 2 * k + 2));
    }
    steps.push(Step::new(Maneuver::new(ManeuverKind::Arrive, None, geometry[20]), 0.0, 0.0, 20, 20));
    let route = Route::new(geometry.clone(), vec![Leg { distance_m: 2000.0, duration_s: 200.0, steps }], 2000.0, 200.0);
    route.validate().unwrap();
    let mut s = Session::new(route, TravelMode::Car);
    s.locate(&Location { coordinate: geometry[0], accuracy_m: Some(5.0), bearing: Some(90.0), speed_mps: Some(20.0), time_ms: 0 });
    let first_turn = s.along_m(2);
    let mut max = 0.0f64;
    for t in (4..=60).map(|k| k * 1000) {
        s.estimate(t);
        max = max.max(s.progress_m);
    }
    println!("first turn at {first_turn:.0} m, estimated progress reached {max:.0} m");
    assert!(max <= first_turn + 31.0, "estimate ran {max:.0} m, past the maneuver at {first_turn:.0} m");
}

#[test]
fn oneway_with_contraflow_cycling_loses_its_lanes() {
    let elements = vec![
        Element::Node(Node { id: 1, coordinate: Coordinate::new(35.0, 139.0), tags: tags(&[]) }),
        Element::Node(Node { id: 2, coordinate: Coordinate::new(35.0, 139.002), tags: tags(&[]) }),
        Element::Way(Way {
            id: 10,
            nodes: vec![1, 2],
            tags: tags(&[
                ("highway", "secondary"),
                ("oneway", "yes"),
                ("oneway:bicycle", "no"),
                ("lanes", "2"),
                ("turn:lanes", "left|through"),
                ("destination", "Tokyo"),
            ]),
        }),
    ];
    let graph: Graph = GraphBuilder::from_elements(&elements).unwrap().0;
    let w = &graph.ways[0];
    println!("lanes {} / {}, turn_lanes {:?}, destination {:?}", w.lanes, w.lanes_backward, graph.string(w.turn_lanes), graph.string(w.destination));
    assert_eq!(graph.string(w.turn_lanes), Some("left|through"));
    assert_eq!(w.lanes, 2);
}

#[test]
fn check_route_drops_a_search_in_progress() {
    let (mut core, _dir) = with_region("search", |_| {});
    plan(&mut core, T0 + 1);
    let fix = |c: f64, t: i64| Location { coordinate: point(c, 0.0), accuracy_m: Some(5.0), bearing: Some(90.0), speed_mps: Some(12.0), time_ms: t };
    core.handle(Input::Located { now: T0 + 4, location: fix(0.0, T0 + 4) });
    core.handle(Input::StartNavigation { now: T0 + 5 });
    let t = T0 + 5 + 5 * 60_000;
    let r = core.handle(Input::Search { now: t, query: "コンビニ".into() });
    let (id, _) = fetches(&r).remove(0);
    core.handle(Input::Located { now: t + 1, location: fix(1.0, t + 1) });
    let photon = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[139.761,35.68]},"properties":{"osm_type":"N","osm_id":1,"name":"Shop"}}]}"#;
    let r = core.handle(Input::Fetched { now: t + 2, id, response: HttpResponse::ok(photon) });
    // The branches of Photon's places are looked up before they are shown.
    let (id, _) = fetches(&r).into_iter().find(|(_, q)| q.host().as_deref() == Some("nominatim.openstreetmap.org")).expect("the branches are looked up");
    let r = core.handle(Input::Fetched { now: t + 3, id, response: HttpResponse::ok("[]") });
    let s = r.snapshot.unwrap();
    println!("loading {:?} results {}", s.search.loading, s.search.results.len());
    assert!(!s.search.loading, "the search stays loading forever");
}

#[test]
fn traffic_answer_asks_a_second_router_while_google_is_in_flight() {
    let (mut core, _dir) = with_region("double", |s| {
        s.traffic.here_api_key = "k".into();
        s.google.enabled = true;
        s.google.directions = true;
    });
    let r = core.handle(Input::Plan {
        now: T0 + 1,
        origin: place(point(0.0, 0.0)),
        destination: place(point(20.0, 0.0)),
        stops: vec![],
        mode: TravelMode::Car,
        avoid: Avoid::default(),
        router: RouterChoice::Automatic,
    });
    let sent = fetches(&r);
    for (_, q) in &sent {
        println!("plan sent {}", q.host().unwrap_or_default());
    }
    let mut after = Vec::new();
    for (id, _) in sent.iter().filter(|(_, q)| q.host().as_deref() == Some("data.traffic.hereapi.com")) {
        let body = NO_INCIDENTS;
        let r = core.handle(Input::Fetched { now: T0 + 2, id: *id, response: HttpResponse::ok(body) });
        after.extend(fetches(&r));
    }
    for (_, q) in &after {
        println!("after traffic sent {}", q.host().unwrap_or_default());
    }
    let d = core.handle(Input::Snapshot { now: T0 + 3 }).snapshot.unwrap().directions.unwrap();
    println!("routes {} loading {}", d.routes.len(), d.loading);
    assert!(after.is_empty() && d.routes.is_empty(), "another router was asked while Google's request is in flight");
}

#[test]
fn failed_traffic_marks_the_area_covered() {
    let (mut core, _dir) = with_region("failed", |s| s.traffic.here_api_key = "k".into());
    let r = plan(&mut core, T0 + 1);
    for (id, _) in fetches(&r) {
        core.handle(Input::Fetched { now: T0 + 2, id, response: HttpResponse::failed("offline") });
    }
    let r = plan(&mut core, T0 + 60_000);
    let n = fetches(&r).len();
    println!("HERE fetches on the second plan: {n}");
    assert!(n > 0, "the failed traffic is never fetched again for 15 minutes");
}

#[test]
fn control_without_check() {
    let (mut core, _dir) = with_region("search2", |_| {});
    plan(&mut core, T0 + 1);
    let fix = |c: f64, t: i64| Location { coordinate: point(c, 0.0), accuracy_m: Some(5.0), bearing: Some(90.0), speed_mps: Some(12.0), time_ms: t };
    core.handle(Input::Located { now: T0 + 4, location: fix(0.0, T0 + 4) });
    core.handle(Input::StartNavigation { now: T0 + 5 });
    let t = T0 + 5 + 60_000;
    let r = core.handle(Input::Search { now: t, query: "コンビニ".into() });
    let (id, _) = fetches(&r).remove(0);
    core.handle(Input::Located { now: t + 1, location: fix(1.0, t + 1) });
    let photon = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[139.761,35.68]},"properties":{"osm_type":"N","osm_id":1,"name":"Shop"}}]}"#;
    let r = core.handle(Input::Fetched { now: t + 2, id, response: HttpResponse::ok(photon) });
    let s = r.snapshot.unwrap();
    println!("control: loading {:?} results {}", s.search.loading, s.search.results.len());
}
