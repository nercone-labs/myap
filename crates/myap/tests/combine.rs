//! OpenStreetMap and Google combined: searches of both, and the place selected looked up in each,
//! every field filled from the other saying where it came from.

use fetch::Response as HttpResponse;
use geo::Coordinate;
use geocoding::{Place, PlaceField, PlaceSource, Wheelchair};
use myap::*;

const T0: i64 = 1_795_000_000_000;

fn start() -> Core {
    let mut core = Core::new();
    core.handle(Input::Start { now: T0, data_dir: None, locale: "ja-JP".into(), utc_offset_minutes: 540 });
    core
}

fn configure(core: &mut Core, change: impl FnOnce(&mut Settings)) {
    let mut settings = core.handle(Input::Snapshot { now: T0 }).snapshot.unwrap().settings;
    change(&mut settings);
    core.handle(Input::Configure { now: T0, settings });
}

fn google_on(settings: &mut Settings) {
    settings.google.enabled = true;
    settings.google.search = true;
    settings.google.place_details = true;
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

fn to(sent: &[(u64, fetch::Request)], host: &str) -> Option<(u64, fetch::Request)> {
    sent.iter().find(|(_, r)| r.host().unwrap_or_default() == host).cloned()
}

fn answer(core: &mut Core, now: i64, id: u64, body: &str) -> myap::Response {
    core.handle(Input::Fetched { now, id, response: HttpResponse::ok(body) })
}

/// Opens Google's session, and returns the requests that waited for it.
fn open_session(core: &mut Core, now: i64, page: u64) -> Vec<(u64, fetch::Request)> {
    let response = HttpResponse { status: 200, body: "<html>".into(), headers: vec![("Set-Cookie".into(), "NID=abc; path=/".into())], error: None };
    fetches(&core.handle(Input::Fetched { now, id: page, response }))
}

/// A Photon answer with these places: (osm id, name, latitude, longitude).
fn photon(places: &[(&str, &str, f64, f64)]) -> String {
    let features: Vec<String> = places
        .iter()
        .map(|(id, name, lat, lon)| {
            let (kind, number) = id.split_at(1);
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{lon},{lat}]}},"properties":{{"osm_type":"{kind}","osm_id":{number},"osm_key":"amenity","osm_value":"restaurant","name":"{name}","city":"中央区","countrycode":"JP"}}}}"#
            )
        })
        .collect();
    format!(r#"{{"type":"FeatureCollection","features":[{}]}}"#, features.join(","))
}

fn lookup(id: &str, tags: &str) -> String {
    let (kind, number) = id.split_at(1);
    let kind = match kind {
        "N" => "node",
        "W" => "way",
        _ => "relation",
    };
    format!(
        r#"[{{"osm_type":"{kind}","osm_id":{number},"lat":"35.6825","lon":"139.7716","category":"amenity","type":"restaurant","name":"x","address":{{}},"extratags":{tags}}}]"#
    )
}

const RAMEN: &str = include_str!("../../google/tests/fixtures/search_ramen.txt");
/// Where the first place of `RAMEN` is (つじ田 日本橋八重洲店).
const TSUJITA: (f64, f64) = (35.6825127, 139.7716716);

#[test]
fn a_search_of_google_is_combined_with_openstreetmap() {
    let mut core = start();
    configure(&mut core, google_on);
    let r = core.handle(Input::Search { now: T0 + 1, query: "ラーメン".into() });
    let sent = fetches(&r);
    // Photon at once; Google after its session is open.
    let (photon_id, photon_request) = to(&sent, "photon.komoot.io").expect("OpenStreetMap is searched too");
    assert!(photon_request.url.contains("q=%E3%83%A9%E3%83%BC%E3%83%A1%E3%83%B3"));
    let (page, _) = to(&sent, "www.google.com").unwrap();
    let (google_id, _) = open_session(&mut core, T0 + 2, page).into_iter().next().unwrap();

    // OpenStreetMap answers first: nothing is shown until Google has too.
    let body = photon(&[
        ("N77", "つじ田 日本橋八重洲店", TSUJITA.0 + 0.0002, TSUJITA.1),
        // Near the results of Google but none of them: shown after them.
        ("N78", "らあめん花月", TSUJITA.0 + 0.001, TSUJITA.1 + 0.001),
        // Far from every result of Google: not shown.
        ("N79", "ラーメン横丁", 43.06, 141.35),
    ]);
    let r = answer(&mut core, T0 + 3, photon_id, &body);
    let (lookup_id, _) = to(&fetches(&r), "nominatim.openstreetmap.org").expect("the branches are looked up");
    let snapshot = r.snapshot.unwrap();
    assert!(snapshot.search.loading);
    assert!(snapshot.search.results.is_empty());
    let snapshot = answer(&mut core, T0 + 4, lookup_id, "[]").snapshot.unwrap();
    assert!(snapshot.search.loading, "Google has not answered yet");

    let snapshot = answer(&mut core, T0 + 5, google_id, RAMEN).snapshot.unwrap();
    let search = &snapshot.search;
    assert!(!search.loading);
    assert_eq!(search.sources, vec!["OpenStreetMap", "Google"]);
    let ids: Vec<&str> = search.results.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), 4, "{ids:?}");
    assert!(ids[..3].iter().all(|id| id.starts_with("google:")), "Google's order first: {ids:?}");
    assert_eq!(ids[3], "osm:N78");
    assert_eq!(search.results[0].links, vec!["osm:N77"], "the same place in OpenStreetMap is linked");
}

#[test]
fn a_place_of_google_is_filled_from_openstreetmap() {
    let mut core = start();
    configure(&mut core, google_on);
    let mut place = Place::new("google:0x1:0x2", PlaceSource::Google, "つじ田 日本橋八重洲店", Coordinate::new(TSUJITA.0, TSUJITA.1));
    // Google's details are known already: Google is not asked again.
    place.phone = Some("03-3527-9373".into());
    let r = core.handle(Input::Select { now: T0 + 1, place });
    let sent = fetches(&r);
    assert!(to(&sent, "www.google.com").is_none());
    // Its element of OpenStreetMap is looked for by its name, only around it.
    let (id, request) = to(&sent, "photon.komoot.io").unwrap();
    assert!(request.url.contains("&bbox="));
    assert!(r.snapshot.unwrap().selected.unwrap().loading);

    let r = answer(&mut core, T0 + 2, id, &photon(&[("N77", "つじ田 八重洲店", TSUJITA.0 + 0.0001, TSUJITA.1), ("N78", "一蘭", TSUJITA.0, TSUJITA.1)]));
    // Then the tags of that element.
    let (id, request) = to(&fetches(&r), "nominatim.openstreetmap.org").expect("its tags are looked up");
    assert!(request.url.contains("/lookup?osm_ids=N77&"));
    let selected = r.snapshot.unwrap().selected.unwrap();
    assert!(selected.loading);
    assert_eq!(selected.place.links, vec!["osm:N77"]);

    let tags = r#"{"opening_hours":"Mo-Su 11:00-21:30","wheelchair":"no","phone":"+81 3-0000-0000"}"#;
    let selected = answer(&mut core, T0 + 3, id, &lookup("N77", tags)).snapshot.unwrap().selected.unwrap();
    assert!(!selected.loading);
    let place = &selected.place;
    assert_eq!(place.wheelchair, Some(Wheelchair::No));
    assert_eq!(place.hours, vec!["Mo-Su 11:00-21:30"]);
    assert_eq!(place.source_of(PlaceField::Hours), PlaceSource::OpenStreetMap);
    // Google's phone stays.
    assert_eq!(place.phone.as_deref(), Some("03-3527-9373"));
    assert_eq!(place.source_of(PlaceField::Phone), PlaceSource::Google);
    // What it became is what is remembered.
    let snapshot = core.handle(Input::Snapshot { now: T0 + 4 }).snapshot.unwrap();
    assert_eq!(snapshot.recent[0].wheelchair, Some(Wheelchair::No));
}

#[test]
fn a_place_of_openstreetmap_is_filled_from_google() {
    let mut core = start();
    configure(&mut core, google_on);
    let place = Place::new("osm:N5", PlaceSource::OpenStreetMap, "つじ田 日本橋八重洲店", Coordinate::new(TSUJITA.0 + 0.0003, TSUJITA.1));
    let r = core.handle(Input::Select { now: T0 + 1, place });
    let sent = fetches(&r);
    let (nominatim, request) = to(&sent, "nominatim.openstreetmap.org").unwrap();
    assert!(request.url.contains("osm_ids=N5"));
    let (page, _) = to(&sent, "www.google.com").expect("Google is asked, details being allowed");
    let (details, _) = open_session(&mut core, T0 + 2, page).into_iter().next().unwrap();
    let snapshot = r.snapshot.unwrap();
    // What is sent is recorded (besides the opening of the session).
    let sent: Vec<&GoogleRecord> =
        snapshot.google.records.iter().filter(|r| r.purpose == GoogleUse::PlaceDetails && !r.sent.starts_with("Google Maps")).collect();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].sent.starts_with("つじ田 日本橋八重洲店"));

    // Google's answer has no result of the id: the same place is found by name and distance.
    let selected = answer(&mut core, T0 + 3, details, RAMEN).snapshot.unwrap().selected.unwrap();
    assert!(selected.loading, "OpenStreetMap has not answered");
    let place = &selected.place;
    assert_eq!(place.rating, Some(4.5));
    assert_eq!(place.source_of(PlaceField::Rating), PlaceSource::Google);
    assert_eq!(place.links, vec!["google:0x60188bfdeaae4297:0x3dc00b75ddc95be"]);
    // Its own name and position stay.
    assert_eq!(place.id, "osm:N5");
    assert!((place.coordinate.latitude - (TSUJITA.0 + 0.0003)).abs() < 1e-9);

    // OpenStreetMap's own hours replace Google's, which only filled them.
    let tags = r#"{"opening_hours":"Mo-Su 11:00-21:30"}"#;
    let selected = answer(&mut core, T0 + 4, nominatim, &lookup("N5", tags)).snapshot.unwrap().selected.unwrap();
    assert!(!selected.loading);
    assert_eq!(selected.place.hours, vec!["Mo-Su 11:00-21:30"]);
    assert_eq!(selected.place.source_of(PlaceField::Hours), PlaceSource::OpenStreetMap);
    assert_eq!(selected.place.source_of(PlaceField::Rating), PlaceSource::Google);
}

#[test]
fn each_lookup_follows_its_setting() {
    let place = || Place::new("osm:N5", PlaceSource::OpenStreetMap, "カフェ", Coordinate::new(35.68, 139.76));
    let hosts = |core: &mut Core| -> Vec<String> {
        let r = core.handle(Input::Select { now: T0 + 1, place: place() });
        fetches(&r).iter().map(|(_, r)| r.host().unwrap_or_default()).collect()
    };
    // Google off: only OpenStreetMap.
    let mut core = start();
    assert_eq!(hosts(&mut core), vec!["nominatim.openstreetmap.org"]);
    // Google's details allowed, but not combining: Google is not told of places of OpenStreetMap.
    let mut core = start();
    configure(&mut core, |s| {
        google_on(s);
        s.places.combine = false;
    });
    assert_eq!(hosts(&mut core), vec!["nominatim.openstreetmap.org"]);
    // Neither: nothing is sent.
    let mut core = start();
    configure(&mut core, |s| {
        s.places.combine = false;
        s.places.details = false;
    });
    assert!(hosts(&mut core).is_empty());
    let snapshot = core.handle(Input::Snapshot { now: T0 + 2 }).snapshot.unwrap();
    assert!(!snapshot.selected.unwrap().loading);
}

#[test]
fn answers_for_a_place_no_longer_shown_are_ignored() {
    let mut core = start();
    let r = core.handle(Input::Select { now: T0 + 1, place: Place::new("osm:N5", PlaceSource::OpenStreetMap, "カフェ", Coordinate::new(35.68, 139.76)) });
    let (id, _) = to(&fetches(&r), "nominatim.openstreetmap.org").unwrap();
    core.handle(Input::Deselect { now: T0 + 2 });
    // The same place selected again, then the answer of the first lookup.
    core.handle(Input::Select { now: T0 + 3, place: Place::new("osm:N5", PlaceSource::OpenStreetMap, "カフェ", Coordinate::new(35.68, 139.76)) });
    let selected = answer(&mut core, T0 + 4, id, &lookup("N5", r#"{"wheelchair":"yes"}"#)).snapshot.unwrap().selected.unwrap();
    assert_eq!(selected.place.wheelchair, None);
    assert!(selected.loading, "its own lookup is still waiting");
}

#[test]
fn nominatim_answers_when_photon_fails() {
    let mut core = start();
    let r = core.handle(Input::Search { now: T0 + 1, query: "東京駅".into() });
    let (id, _) = to(&fetches(&r), "photon.komoot.io").unwrap();
    let r = core.handle(Input::Fetched { now: T0 + 2, id, response: HttpResponse { status: 502, ..Default::default() } });
    let (id, request) = to(&fetches(&r), "nominatim.openstreetmap.org").expect("Nominatim is asked");
    assert!(request.url.contains("/search?"));
    let snapshot = answer(&mut core, T0 + 3, id, include_str!("../../geocoding/tests/fixtures/nominatim_search.json")).snapshot.unwrap();
    assert!(snapshot.search.error.is_none(), "{:?}", snapshot.search.error);
    assert!(!snapshot.search.loading);
    assert!(!snapshot.search.results.is_empty() || snapshot.selected.is_some());
    assert_eq!(snapshot.search.sources, vec!["OpenStreetMap ✕", "Nominatim"]);
}

#[test]
fn google_is_asked_for_its_own_details_when_openstreetmap_filled_them() {
    let mut core = start();
    configure(&mut core, google_on);
    // A result of a combined search: Google's place with the hours and phone of OpenStreetMap.
    let mut place = Place::new("google:0x1:0x2", PlaceSource::Google, "つじ田 日本橋八重洲店", Coordinate::new(TSUJITA.0, TSUJITA.1));
    let mut element = Place::new("osm:N77", PlaceSource::OpenStreetMap, "つじ田 日本橋八重洲店", Coordinate::new(TSUJITA.0, TSUJITA.1));
    element.hours = vec!["Mo-Su 11:00-21:30".into()];
    element.phone = Some("+81 3-0000-0000".into());
    place.fill_from(&element);
    let r = core.handle(Input::Select { now: T0 + 1, place });
    assert!(to(&fetches(&r), "www.google.com").is_some(), "Google's own details win over those filled");
}

/// Two branches of one chain, as OpenStreetMap maps them: the same `name`, and the branch apart.
const STARBUCKS: [(&str, &str, f64, f64); 2] =
    [("N3612023579", "スターバックス", 35.7506628, 139.8043808), ("W621235316", "スターバックス", 35.6492619, 139.6926277)];

/// A Nominatim lookup answering each (osm id, branch).
fn branches(elements: &[(&str, &str)]) -> String {
    let all: Vec<String> = elements
        .iter()
        .map(|(id, branch)| {
            let (kind, number) = id.split_at(1);
            let kind = if kind == "N" { "node" } else { "way" };
            format!(r#"{{"osm_type":"{kind}","osm_id":{number},"lat":"35.0","lon":"139.0","category":"amenity","type":"cafe","name":"スターバックス","address":{{}},"extratags":{{"branch":"{branch}","brand":"スターバックス"}}}}"#)
        })
        .collect();
    format!("[{}]", all.join(","))
}

#[test]
fn the_results_of_photon_are_named_with_their_branches() {
    let mut core = start();
    let r = core.handle(Input::Search { now: T0 + 1, query: "スターバックス".into() });
    let (photon_id, _) = to(&fetches(&r), "photon.komoot.io").unwrap();
    let r = answer(&mut core, T0 + 2, photon_id, &photon(&STARBUCKS));
    let (lookup_id, request) = to(&fetches(&r), "nominatim.openstreetmap.org").expect("Nominatim is asked for the branches");
    assert!(request.url.contains("/lookup?"), "{}", request.url);
    assert!(request.url.contains("osm_ids=N3612023579%2CW621235316") || request.url.contains("osm_ids=N3612023579,W621235316"), "{}", request.url);
    assert!(request.url.contains("extratags=1"), "{}", request.url);
    assert!(r.snapshot.unwrap().search.results.is_empty(), "not shown before their branches");

    // The second has no branch in the answer: it keeps its name.
    let snapshot = answer(&mut core, T0 + 3, lookup_id, &branches(&[("N3612023579", "北千住マルイ店")])).snapshot.unwrap();
    assert!(!snapshot.search.loading);
    let names: Vec<&str> = snapshot.search.results.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["スターバックス 北千住マルイ店", "スターバックス"]);
    assert_eq!(snapshot.search.results[0].branch.as_deref(), Some("北千住マルイ店"));
}

#[test]
fn no_branch_is_looked_up_while_the_details_are_off() {
    let mut core = start();
    configure(&mut core, |s| s.places.details = false);
    let r = core.handle(Input::Search { now: T0 + 1, query: "スターバックス".into() });
    let (photon_id, _) = to(&fetches(&r), "photon.komoot.io").unwrap();
    let r = answer(&mut core, T0 + 2, photon_id, &photon(&STARBUCKS));
    assert!(to(&fetches(&r), "nominatim.openstreetmap.org").is_none(), "nothing reaches Nominatim");
    let snapshot = r.snapshot.unwrap();
    assert!(!snapshot.search.loading);
    assert_eq!(snapshot.search.results.len(), 2);
    assert!(snapshot.search.results.iter().all(|p| p.name == "スターバックス"));
}

#[test]
fn the_results_are_shown_when_the_branches_cannot_be_looked_up() {
    let mut core = start();
    let r = core.handle(Input::Search { now: T0 + 1, query: "スターバックス".into() });
    let (photon_id, _) = to(&fetches(&r), "photon.komoot.io").unwrap();
    let r = answer(&mut core, T0 + 2, photon_id, &photon(&STARBUCKS));
    let (lookup_id, _) = to(&fetches(&r), "nominatim.openstreetmap.org").unwrap();
    let snapshot = core.handle(Input::Fetched { now: T0 + 3, id: lookup_id, response: HttpResponse { status: 503, ..Default::default() } }).snapshot.unwrap();
    assert!(!snapshot.search.loading);
    assert!(snapshot.search.error.is_none());
    assert_eq!(snapshot.search.results.len(), 2);
    assert_eq!(snapshot.search.sources, vec!["OpenStreetMap"], "Photon answered; only the branches are missing");
}

#[test]
fn the_branches_of_an_earlier_search_change_nothing() {
    let mut core = start();
    let r = core.handle(Input::Search { now: T0 + 1, query: "スターバックス".into() });
    let (photon_id, _) = to(&fetches(&r), "photon.komoot.io").unwrap();
    let r = answer(&mut core, T0 + 2, photon_id, &photon(&STARBUCKS));
    let (lookup_id, _) = to(&fetches(&r), "nominatim.openstreetmap.org").unwrap();
    let r = core.handle(Input::Search { now: T0 + 3, query: "ドトール".into() });
    let (photon_id, _) = to(&fetches(&r), "photon.komoot.io").unwrap();
    let snapshot = answer(&mut core, T0 + 4, lookup_id, &branches(&[("N3612023579", "北千住マルイ店")])).snapshot.unwrap();
    assert!(snapshot.search.loading, "the new search still waits for Photon");
    assert!(snapshot.search.results.is_empty());
    assert_eq!(snapshot.search.query, "ドトール");
    let _ = photon_id;
}

#[test]
fn a_place_opened_takes_the_branch_of_its_element() {
    let mut core = start();
    let place = Place::new("osm:N3612023579", PlaceSource::OpenStreetMap, "スターバックス", Coordinate::new(35.7506628, 139.8043808));
    let r = core.handle(Input::Select { now: T0 + 1, place });
    let (id, _) = to(&fetches(&r), "nominatim.openstreetmap.org").unwrap();
    let selected = answer(&mut core, T0 + 2, id, &branches(&[("N3612023579", "北千住マルイ店")])).snapshot.unwrap().selected.unwrap();
    assert_eq!(selected.place.name, "スターバックス 北千住マルイ店");
}
