//! Whether two places of two sources are the same place, and how one fills what the other lacks.

use geo::{Bounds, Coordinate};
use geocoding::{Filled, Place, PlaceField, PlaceMatch, PlaceSource, Wheelchair};

fn osm(id: &str, name: &str, latitude: f64, longitude: f64) -> Place {
    Place::new(id, PlaceSource::OpenStreetMap, name, Coordinate::new(latitude, longitude))
}

fn google(id: &str, name: &str, latitude: f64, longitude: f64) -> Place {
    Place::new(id, PlaceSource::Google, name, Coordinate::new(latitude, longitude))
}

#[test]
fn names_are_folded_before_they_are_compared() {
    assert_eq!(PlaceMatch::normalize("Ｓｔａｒｂｕｃｋｓ　Coffee"), "starbuckscoffee");
    assert_eq!(PlaceMatch::normalize("とうきょう・タワー"), "トウキョウタワー");
    assert_eq!(PlaceMatch::normalize("McDonald's 渋谷店"), "mcdonalds渋谷店");
    let m = PlaceMatch::default();
    assert_eq!(m.similarity("東京タワー", "東京 タワー"), 1.0);
    assert_eq!(m.similarity("ｶﾌｪ", "ｶﾌｪ"), 1.0);
    // A name held in the other is alike, at least `contained_similarity`.
    assert!(m.similarity("スターバックス", "スターバックス コーヒー 渋谷店") >= m.contained_similarity);
    // One character in common is not enough; nor are names in two scripts.
    assert!(m.similarity("東京駅", "京都駅") < m.min_similarity);
    assert_eq!(m.similarity("Tokyo Tower", "東京タワー"), 0.0);
    assert_eq!(m.similarity("", "東京"), 0.0);
}

#[test]
fn the_same_place_is_alike_and_near() {
    let m = PlaceMatch::default();
    let g = google("google:1", "つじ田 日本橋八重洲店", 35.6800, 139.7700);
    // 50 m away and the same name: the same.
    let near = osm("osm:N1", "つじ田 八重洲店", 35.68045, 139.7700);
    assert!(m.score(&g, &near).is_some());
    // The same name 1 km away is another branch.
    let far = osm("osm:N2", "つじ田 日本橋八重洲店", 35.6890, 139.7700);
    assert_eq!(m.score(&g, &far), None);
    // Another name at the same point is another shop.
    let other = osm("osm:N3", "一蘭", 35.6800, 139.7700);
    assert_eq!(m.score(&g, &other), None);
    // The nearer of two alike is surer.
    let nearer = osm("osm:N4", "つじ田 日本橋八重洲店", 35.68001, 139.7700);
    let candidates = vec![near.clone(), far, other, nearer];
    assert_eq!(m.best(&g, &candidates).map(|p| p.id.as_str()), Some("osm:N4"));
    // An area whose bounds hold the other is the same further away (a park and its centre).
    let mut park = osm("osm:W5", "代々木公園", 35.6717, 139.6949);
    park.bounds = Some(Bounds { south: 35.665, west: 139.688, north: 35.678, east: 139.701 });
    let pin = google("google:2", "代々木公園", 35.6760, 139.6990);
    assert!(park.coordinate.distance_m(&pin.coordinate) > m.max_distance_m);
    assert!(m.score(&pin, &park).is_some());
}

#[test]
fn filling_never_replaces_and_says_where_each_field_came_from() {
    let mut g = google("google:1", "カフェ", 35.68, 139.76);
    g.phone = Some("03-1111-1111".into());
    g.rating = Some(4.2);
    g.review_count = Some(10);
    let mut o = osm("osm:N9", "カフェ", 35.68, 139.76);
    o.phone = Some("03-2222-2222".into());
    o.hours = vec!["Mo-Fr 08:00-20:00".into()];
    o.wheelchair = Some(Wheelchair::Yes);
    o.address = Some("千代田区".into());

    let filled = g.fill_from(&o);
    assert_eq!(filled, vec![PlaceField::Address, PlaceField::Hours, PlaceField::Wheelchair]);
    // What Google knew stays: its phone, its rating; its name, id and source.
    assert_eq!(g.phone.as_deref(), Some("03-1111-1111"));
    assert_eq!(g.rating, Some(4.2));
    assert_eq!((g.id.as_str(), g.source), ("google:1", PlaceSource::Google));
    assert_eq!(g.hours, vec!["Mo-Fr 08:00-20:00"]);
    assert_eq!(g.source_of(PlaceField::Hours), PlaceSource::OpenStreetMap);
    assert_eq!(g.source_of(PlaceField::Phone), PlaceSource::Google);
    assert_eq!(g.links, vec!["osm:N9"]);
    assert!(g.knows(PlaceSource::OpenStreetMap));
    assert_eq!(g.id_in(PlaceSource::OpenStreetMap), Some("osm:N9"));

    // Filled again from the same place: nothing new, no link twice.
    assert!(g.fill_from(&o).is_empty());
    assert_eq!(g.links.len(), 1);

    // The other way: the place of OpenStreetMap filled from Google's.
    let mut o2 = osm("osm:N9", "カフェ", 35.68, 139.76);
    let mut g2 = google("google:1", "カフェ", 35.68, 139.76);
    g2.rating = Some(4.0);
    g2.review_count = Some(3);
    o2.fill_from(&g2);
    assert_eq!((o2.rating, o2.review_count), (Some(4.0), Some(3)));
    assert_eq!(o2.filled, vec![Filled { field: PlaceField::Rating, source: PlaceSource::Google }]);

    // A field another source filled keeps that source when it is passed on.
    let mut third = osm("osm:N10", "カフェ", 35.68, 139.76);
    third.fill_from(&g);
    assert_eq!(third.source_of(PlaceField::Rating), PlaceSource::Google);
    assert_eq!(third.source_of(PlaceField::Hours), PlaceSource::OpenStreetMap);
    assert!(!third.filled.iter().any(|f| f.field == PlaceField::Hours), "the place's own source is not recorded");
}

#[test]
fn updating_from_the_own_source_replaces() {
    let mut g = google("google:1", "カフェ", 35.68, 139.76);
    let mut o = osm("osm:N9", "カフェ", 35.68, 139.76);
    o.hours = vec!["24/7".into()];
    g.fill_from(&o);
    assert_eq!(g.source_of(PlaceField::Hours), PlaceSource::OpenStreetMap);
    // Google's own details come later, with hours: they replace those of OpenStreetMap.
    let mut details = google("google:1", "カフェ", 35.68, 139.76);
    details.hours = vec!["月曜日: 8時00分～20時00分".into()];
    details.status = Some("営業中".into());
    let replaced = g.update_from(&details);
    assert_eq!(replaced, vec![PlaceField::Hours, PlaceField::Status]);
    assert_eq!(g.hours, vec!["月曜日: 8時00分～20時00分"]);
    assert_eq!(g.source_of(PlaceField::Hours), PlaceSource::Google);
    assert!(g.filled.is_empty());
    // What the newer lookup does not know stays.
    assert_eq!(g.links, vec!["osm:N9"]);
}

#[test]
fn two_searches_are_combined_one_to_one() {
    let m = PlaceMatch::default();
    let primary = vec![google("google:1", "ラーメン一番", 35.6800, 139.7700), google("google:2", "ラーメン二番", 35.6810, 139.7700)];
    let mut a = osm("osm:N1", "ラーメン一番", 35.68002, 139.7700);
    a.wheelchair = Some(Wheelchair::No);
    // Alike to both, but nearer the second.
    let b = osm("osm:N2", "ラーメン二番", 35.68098, 139.7700);
    let lone = osm("osm:N3", "駐車場", 35.6805, 139.7705);
    let (merged, rest) = m.combine(primary, vec![lone.clone(), b, a]);
    assert_eq!(merged[0].id, "google:1");
    assert_eq!(merged[0].links, vec!["osm:N1"]);
    assert_eq!(merged[0].wheelchair, Some(Wheelchair::No));
    assert_eq!(merged[1].links, vec!["osm:N2"]);
    assert_eq!(rest, vec![lone]);
    assert_eq!(PlaceSource::of_id("osm:N1"), Some(PlaceSource::OpenStreetMap));
    assert_eq!(PlaceSource::of_id("plain"), None);
}

#[test]
fn places_of_earlier_versions_are_read() {
    // A place saved before the fields of combining existed.
    let old = r#"{"id":"osm:N1","source":"open_street_map","name":"東京駅","coordinate":{"latitude":35.68,"longitude":139.76}}"#;
    let place: Place = serde_json::from_str(old).unwrap();
    assert!(place.links.is_empty() && place.filled.is_empty() && place.wheelchair.is_none());
    // Written back without the empty fields.
    assert!(!serde_json::to_string(&place).unwrap().contains("filled"));
}
