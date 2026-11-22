use fetch::Response;
use geo::Coordinate;
use geocoding::{Address, Geocoder, Nominatim, Photon, Place, PlaceSource, SearchRequest, Wheelchair};

fn request(query: &str) -> SearchRequest {
    SearchRequest { query: query.into(), near: Some(Coordinate::new(35.68, 139.76)), bounds: None, bounded: false, language: "ja-JP".into(), limit: 5 }
}

#[test]
fn photon_request_and_response() {
    let p = Photon::komoot("Myap/test");
    let r = p.search_request(&request("東京駅")).unwrap();
    // Photon has no Japanese names: the local names are asked for.
    assert_eq!(r.url, "https://photon.komoot.io/api/?q=%E6%9D%B1%E4%BA%AC%E9%A7%85&limit=5&lang=default&lat=35.68000&lon=139.76000");
    assert!(p.search_request(&request("  ")).is_err());
    // Recorded from photon.komoot.io, 2026-11-22.
    let places = p.parse_search(&Response::ok(include_str!("fixtures/photon_tokyo.json"))).unwrap();
    assert_eq!(places.len(), 2);
    assert_eq!(places[0].id, "osm:W1444524669");
    assert_eq!(places[0].name, "東京駅");
    assert_eq!(places[0].source, PlaceSource::OpenStreetMap);
    assert_eq!(places[0].address.as_deref(), Some("東京都千代田区丸の内二丁目"));
    assert_eq!(places[0].category.as_deref(), Some("footway"));
    assert!((places[0].coordinate.latitude - 35.6814486).abs() < 1e-9);
}

#[test]
fn nominatim_reverse_and_search() {
    let n = Nominatim::openstreetmap("Myap/test");
    let r = n.reverse_request(&Coordinate::new(35.6812, 139.7671), "ja").unwrap();
    assert_eq!(r.url, "https://nominatim.openstreetmap.org/reverse?lat=35.681200&lon=139.767100&format=jsonv2&zoom=18&addressdetails=1&accept-language=ja");
    assert_eq!(r.header_value("User-Agent"), Some("Myap/test"));
    // Recorded from nominatim.openstreetmap.org, 2026-11-22.
    let place = n.parse_reverse(&Response::ok(include_str!("fixtures/nominatim_reverse.json"))).unwrap().unwrap();
    assert_eq!(place.id, "osm:N12612240169");
    // No name: the address names it.
    assert_eq!(place.name, "千代田区丸の内一丁目エレベーター;地下1階");
    let places = n.parse_search(&Response::ok(include_str!("fixtures/nominatim_search.json"))).unwrap();
    assert_eq!(places[0].name, "東京駅(改札外);M12出入口");
    assert!(places[0].bounds.is_some());
    assert_eq!(n.parse_reverse(&Response::ok(r#"{"error":"Unable to geocode"}"#)).unwrap(), None);
    assert!(n.parse_search(&Response { status: 429, ..Default::default() }).is_err());
}

#[test]
fn addresses() {
    let jp = Address {
        country_code: Some("JP".into()),
        state: Some("東京都".into()),
        city: Some("千代田区".into()),
        locality: Some("丸の内一丁目".into()),
        house_number: Some("9-1".into()),
        ..Default::default()
    };
    assert_eq!(jp.format().as_deref(), Some("東京都千代田区丸の内一丁目9-1"));
    let gb = Address {
        country_code: Some("gb".into()),
        city: Some("London".into()),
        street: Some("Downing Street".into()),
        house_number: Some("10".into()),
        ..Default::default()
    };
    assert_eq!(gb.format().as_deref(), Some("10 Downing Street, London"));
    assert_eq!(Address::default().format(), None);
    // A Japanese address in the Latin alphabet is written as elsewhere.
    let latin = Address {
        country_code: Some("JP".into()),
        state: Some("Tokyo".into()),
        city: Some("Minato".into()),
        locality: Some("Shibakōen 4".into()),
        street: Some("Tokyo Tower Street".into()),
        ..Default::default()
    };
    assert_eq!(latin.format().as_deref(), Some("Tokyo Tower Street, Shibakōen 4, Minato, Tokyo"));
    // Parts in two scripts (Photon gives some names in Japanese, some in the Latin alphabet) are
    // written with separators, not run together.
    let mixed = Address {
        country_code: Some("JP".into()),
        state: Some("東京都".into()),
        city: Some("Tokyo".into()),
        district: Some("Senju".into()),
        locality: Some("Senju 3-chome".into()),
        ..Default::default()
    };
    assert_eq!(mixed.format().as_deref(), Some("Senju 3-chome, Senju, Tokyo, 東京都"));
}

#[test]
fn coordinates_as_text() {
    assert_eq!(Place::parse_coordinate("35.6812, 139.7671"), Some(Coordinate::new(35.6812, 139.7671)));
    assert_eq!(Place::parse_coordinate("35.6812 139.7671"), Some(Coordinate::new(35.6812, 139.7671)));
    assert_eq!(Place::parse_coordinate("95, 139"), None);
    assert_eq!(Place::parse_coordinate("東京駅"), None);
    assert_eq!(Place::at(Coordinate::new(1.0, 2.0)).id, "geo:1.000000,2.000000");
}

#[test]
fn nominatim_lookup_and_extra_tags() {
    let n = Nominatim::openstreetmap("Myap/test");
    let r = n.lookup_request(&["osm:N123", "google:0x1:0x2", "osm:W45", "osm:Xbad", "osm:N"], "ja").unwrap();
    // Only ids of OpenStreetMap elements, as `osm_ids` takes them (N, W, R and the number).
    assert_eq!(r.url, "https://nominatim.openstreetmap.org/lookup?osm_ids=N123%2CW45&format=jsonv2&addressdetails=1&extratags=1&accept-language=ja");
    assert_eq!(r.header_value("User-Agent"), Some("Myap/test"));
    assert!(n.lookup_request(&["google:0x1:0x2"], "ja").is_err());
    // The shape of /lookup's jsonv2 answer, as Nominatim's documentation gives it.
    let body = r#"[{"place_id":1,"osm_type":"node","osm_id":123,"lat":"35.68","lon":"139.76","category":"amenity","type":"cafe",
        "name":"カフェ","display_name":"カフェ, 千代田区","address":{"city":"千代田区","country_code":"jp"},
        "extratags":{"opening_hours":"Mo-Fr 08:00-20:00; Sa,Su 10:00-18:00","contact:phone":"+81 3-1234-5678;+81 3-0000-0000",
        "website":"https://example.jp/","wheelchair":"limited","fax":null}}]"#;
    let places = n.parse_lookup(&Response::ok(body)).unwrap();
    let p = &places[0];
    assert_eq!(p.id, "osm:N123");
    assert_eq!(p.category.as_deref(), Some("cafe"));
    assert_eq!(p.hours, vec!["Mo-Fr 08:00-20:00", "Sa,Su 10:00-18:00"]);
    // `contact:phone` when there is no `phone`; the first of several values.
    assert_eq!(p.phone.as_deref(), Some("+81 3-1234-5678"));
    assert_eq!(p.website.as_deref(), Some("https://example.jp/"));
    assert_eq!(p.wheelchair, Some(Wheelchair::Limited));
    // A website that is not HTTP(S) is not taken.
    let body = r#"[{"osm_type":"way","osm_id":7,"lat":"35","lon":"139","name":"x","extratags":{"website":"javascript:alert(1)","wheelchair":"designated"}}]"#;
    let p = &n.parse_lookup(&Response::ok(body)).unwrap()[0];
    assert_eq!(p.website, None);
    assert_eq!(p.wheelchair, Some(Wheelchair::Yes));
    assert_eq!(Wheelchair::of_tag("no"), Some(Wheelchair::No));
    assert_eq!(Wheelchair::of_tag("unknown"), None);
}

#[test]
fn searches_bounded_to_an_area() {
    let bounds = geo::Bounds { south: 35.67, west: 139.75, north: 35.69, east: 139.77 };
    let mut r = request("カフェ");
    r.bounds = Some(bounds);
    // A preference only, unless bounded.
    let n = Nominatim::openstreetmap("Myap/test");
    let url = n.search_request(&r).unwrap().url;
    assert!(url.contains("extratags=1"));
    assert!(url.contains("viewbox=139.75000%2C35.69000%2C139.77000%2C35.67000"));
    assert!(!url.contains("bounded"));
    assert!(!Photon::komoot("Myap/test").search_request(&r).unwrap().url.contains("bbox"));
    r.bounded = true;
    assert!(n.search_request(&r).unwrap().url.contains("&bounded=1"));
    // Photon's `bbox` is minLon,minLat,maxLon,maxLat.
    assert!(Photon::komoot("Myap/test").search_request(&r).unwrap().url.ends_with("&bbox=139.75000%2C35.67000%2C139.77000%2C35.69000"));
}

// ---- Branches (the `branch` tag: "the part of the name that differs between branches", added to `name`) ----

fn starbucks() -> Place {
    Place::new("osm:N1", PlaceSource::OpenStreetMap, "スターバックス", Coordinate::new(35.75, 139.80))
}

#[test]
fn a_branch_is_added_to_the_name_after_a_space() {
    let mut place = starbucks();
    place.set_branch("北千住マルイ店");
    assert_eq!(place.name, "スターバックス 北千住マルイ店");
    assert_eq!(place.branch.as_deref(), Some("北千住マルイ店"));
}

#[test]
fn a_branch_already_in_the_name_is_not_added_again() {
    let mut place = Place::new("osm:N1", PlaceSource::OpenStreetMap, "スターバックス 北千住マルイ店", Coordinate::new(35.75, 139.80));
    place.set_branch("北千住マルイ店");
    assert_eq!(place.name, "スターバックス 北千住マルイ店");
    // Folded as names are compared: full-width letters and spaces do not matter.
    let mut place = Place::new("osm:N2", PlaceSource::OpenStreetMap, "Starbucks Ｓｈｉｂｕｙａ", Coordinate::new(35.66, 139.70));
    place.set_branch("shibuya");
    assert_eq!(place.name, "Starbucks Ｓｈｉｂｕｙａ");
}

#[test]
fn a_branch_is_added_once() {
    let mut place = starbucks();
    place.set_branch("渋谷店");
    place.set_branch("渋谷店");
    place.set_branch("新宿店");
    assert_eq!(place.name, "スターバックス 渋谷店");
}

#[test]
fn a_blank_branch_changes_nothing() {
    let mut place = starbucks();
    place.set_branch("  ");
    assert_eq!(place.name, "スターバックス");
    assert_eq!(place.branch, None);
    place.set_branch(" 渋谷店 ");
    assert_eq!(place.name, "スターバックス 渋谷店", "trimmed");
}

#[test]
fn a_place_without_a_branch_is_read_and_written_as_before() {
    let place = starbucks();
    let json = serde_json::to_string(&place).unwrap();
    assert!(!json.contains("branch"), "{json}");
    let back: Place = serde_json::from_str(&json).unwrap();
    assert_eq!(back, place);
}

#[test]
fn nominatim_names_a_place_with_its_branch() {
    let nominatim = Nominatim::openstreetmap("test");
    let body = r#"[{"osm_type":"node","osm_id":3612023579,"lat":"35.7506628","lon":"139.8043808","category":"amenity","type":"cafe","name":"スターバックス","address":{"city":"足立区","country_code":"jp"},"extratags":{"branch":"北千住マルイ店","brand":"スターバックス"}}]"#;
    let places = nominatim.parse_lookup(&fetch::Response::ok(body)).unwrap();
    assert_eq!(places[0].name, "スターバックス 北千住マルイ店");
    assert_eq!(places[0].branch.as_deref(), Some("北千住マルイ店"));
}

#[test]
fn nominatim_adds_no_branch_to_a_place_without_a_name() {
    let nominatim = Nominatim::openstreetmap("test");
    let body = r#"[{"osm_type":"node","osm_id":1,"lat":"35.75","lon":"139.80","category":"amenity","type":"cafe","name":"","display_name":"千住三丁目, 足立区","address":{"city":"足立区","country_code":"jp"},"extratags":{"branch":"北千住店"}}]"#;
    let places = nominatim.parse_lookup(&fetch::Response::ok(body)).unwrap();
    assert!(!places[0].name.contains("北千住店"), "{}", places[0].name);
    assert_eq!(places[0].branch, None);
}
