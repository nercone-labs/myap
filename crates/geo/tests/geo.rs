use geo::{Bounds, Coordinate, Line, Polyline, PolylineError, Tile};

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn distance_tokyo_osaka() {
    // Tokyo Station to Osaka Station: about 403 km on the great circle.
    let tokyo = Coordinate::new(35.681236, 139.767125);
    let osaka = Coordinate::new(34.702485, 135.495951);
    let d = tokyo.distance_m(&osaka);
    assert!(close(d, 403_000.0, 3_000.0), "{d}");
    assert_eq!(tokyo.distance_m(&tokyo), 0.0);
}

#[test]
fn bearings() {
    let origin = Coordinate::new(0.0, 0.0);
    assert!(close(origin.bearing_to(&Coordinate::new(1.0, 0.0)), 0.0, 1e-9));
    assert!(close(origin.bearing_to(&Coordinate::new(0.0, 1.0)), 90.0, 1e-9));
    assert!(close(origin.bearing_to(&Coordinate::new(-1.0, 0.0)), 180.0, 1e-9));
    assert!(close(origin.bearing_to(&Coordinate::new(0.0, -1.0)), 270.0, 1e-9));
    assert_eq!(Coordinate::bearing_difference(350.0, 10.0), 20.0);
    assert_eq!(Coordinate::bearing_difference(10.0, 350.0), -20.0);
    assert_eq!(Coordinate::bearing_difference(0.0, 180.0), 180.0);
}

#[test]
fn destination_round_trip() {
    let start = Coordinate::new(35.0, 139.0);
    let end = start.destination(45.0, 10_000.0);
    assert!(close(start.distance_m(&end), 10_000.0, 0.01));
    assert!(close(start.bearing_to(&end), 45.0, 0.01));
}

#[test]
fn checked_rejects_out_of_range() {
    assert!(Coordinate::checked(91.0, 0.0).is_none());
    assert!(Coordinate::checked(0.0, 181.0).is_none());
    assert!(Coordinate::checked(f64::NAN, 0.0).is_none());
    assert!(Coordinate::checked(-90.0, -180.0).is_some());
}

#[test]
fn polyline_reference_example() {
    // The example of Google's documentation of the format (precision 5).
    let points = vec![Coordinate::new(38.5, -120.2), Coordinate::new(40.7, -120.95), Coordinate::new(43.252, -126.453)];
    assert_eq!(Polyline::encode(&points, 5), "_p~iF~ps|U_ulLnnqC_mqNvxq`@");
    let decoded = Polyline::decode("_p~iF~ps|U_ulLnnqC_mqNvxq`@", 5).unwrap();
    assert_eq!(decoded.len(), 3);
    for (a, b) in decoded.iter().zip(&points) {
        assert!(close(a.latitude, b.latitude, 1e-5) && close(a.longitude, b.longitude, 1e-5));
    }
}

#[test]
fn polyline_precision_six_round_trip() {
    let points = vec![Coordinate::new(35.681236, 139.767125), Coordinate::new(35.6812, 139.7672), Coordinate::new(-33.8688, 151.2093)];
    let decoded = Polyline::decode(&Polyline::encode(&points, 6), 6).unwrap();
    for (a, b) in decoded.iter().zip(&points) {
        assert!(close(a.latitude, b.latitude, 1e-6) && close(a.longitude, b.longitude, 1e-6));
    }
}

#[test]
fn polyline_errors() {
    assert_eq!(Polyline::decode("_p~iF", 5), Err(PolylineError::Truncated));
    assert_eq!(Polyline::decode("_p~iF~ps|", 5), Err(PolylineError::Truncated));
    assert!(matches!(Polyline::decode("_p iF~ps|U", 5), Err(PolylineError::InvalidCharacter(2))));
    assert_eq!(Polyline::decode("", 5), Ok(vec![]));
}

#[test]
fn line_projection_and_points() {
    // A line going east along the equator, then north.
    let a = Coordinate::new(0.0, 0.0);
    let b = Coordinate::new(0.0, 0.01);
    let c = Coordinate::new(0.01, 0.01);
    let line = Line::new(vec![a, b, c]);
    let leg = a.distance_m(&b);
    assert!(close(line.length_m(), leg + b.distance_m(&c), 1e-6));

    let p = line.project(&Coordinate::new(0.001, 0.005), 0.0, f64::INFINITY).unwrap();
    assert_eq!(p.segment, 0);
    assert!(close(p.along_m, leg / 2.0, 1.0));
    assert!(close(p.distance_m, 111.2, 1.0));
    assert!(close(p.bearing, 90.0, 1e-6));

    let q = line.project(&Coordinate::new(0.005, 0.0105), 0.0, f64::INFINITY).unwrap();
    assert_eq!(q.segment, 1);

    // Limited to the first segment, the closest point is its end.
    let r = line.project(&Coordinate::new(0.005, 0.0105), 0.0, leg / 2.0).unwrap();
    assert_eq!(r.segment, 0);

    let mid = line.point_at(leg / 2.0).unwrap();
    assert!(close(mid.longitude, 0.005, 1e-9));
    assert_eq!(line.point_at(-5.0), Some(a));
    assert_eq!(line.point_at(1e9), Some(c));
    let slice = line.slice(leg / 2.0, leg + 10.0);
    assert_eq!(slice.len(), 3);
    assert_eq!(slice[1], b);
}

#[test]
fn bounds() {
    let b = Bounds::of(&[Coordinate::new(1.0, 2.0), Coordinate::new(-1.0, 5.0)]).unwrap();
    assert_eq!(b, Bounds { south: -1.0, west: 2.0, north: 1.0, east: 5.0 });
    assert!(b.contains(&Coordinate::new(0.0, 3.0)));
    assert!(!b.contains(&Coordinate::new(0.0, 6.0)));
    assert!(b.intersects(&Bounds { south: 0.5, west: 4.0, north: 3.0, east: 8.0 }));
    assert!(!b.intersects(&Bounds { south: 2.0, west: 4.0, north: 3.0, east: 8.0 }));
    assert!(Bounds::of(&[]).is_none());
}

#[test]
fn tiles() {
    // The tile of Tokyo at zoom 10 (the OpenStreetMap wiki's slippy map formulas).
    let tile = Tile::at(&Coordinate::new(35.681236, 139.767125), 10);
    assert_eq!((tile.x, tile.y), (909, 403));
    let bounds = tile.bounds();
    assert!(bounds.contains(&Coordinate::new(35.681236, 139.767125)));
    assert_eq!(Tile { zoom: 3, x: 3, y: 5 }.quadkey(), "213");
    assert_eq!(Tile::at(&Coordinate::new(0.0, 0.0), 0), Tile { zoom: 0, x: 0, y: 0 });
}

#[test]
fn polygons() {
    use geo::Polygon;
    let square = Polygon::new(vec![Coordinate::new(0.0, 0.0), Coordinate::new(0.0, 1.0), Coordinate::new(1.0, 1.0), Coordinate::new(1.0, 0.0)]);
    assert!(square.is_valid());
    assert!(square.contains(&Coordinate::new(0.5, 0.5)));
    assert!(!square.contains(&Coordinate::new(1.5, 0.5)));
    // A segment crossing the square with both ends outside touches it.
    assert!(square.touches_segment(&Coordinate::new(0.5, -1.0), &Coordinate::new(0.5, 2.0)));
    assert!(!square.touches_segment(&Coordinate::new(2.0, -1.0), &Coordinate::new(2.0, 2.0)));
    assert!(square.touches_line(&[Coordinate::new(-1.0, -1.0), Coordinate::new(0.5, 0.5)]));
    let center = Coordinate::new(35.0, 139.0);
    let circle = Polygon::circle(&center, 500.0, 16);
    assert_eq!(circle.ring.len(), 16);
    assert!(circle.ring.iter().all(|p| (p.distance_m(&center) - 500.0).abs() < 0.5));
    assert!(circle.contains(&center.destination(45.0, 400.0)));
    assert!(!circle.contains(&center.destination(45.0, 600.0)));
    assert!(!Polygon::new(vec![center, center]).is_valid());
}

#[test]
fn offsets_on_the_tangent_plane() {
    let origin = Coordinate::new(35.0, 139.0);
    let (east, north) = origin.offset_m(&origin.destination(90.0, 100.0));
    assert!((east - 100.0).abs() < 0.1 && north.abs() < 0.1, "{east} {north}");
    let (east, north) = origin.offset_m(&origin.destination(0.0, 250.0));
    assert!(east.abs() < 0.1 && (north - 250.0).abs() < 0.1);
}

#[test]
fn the_sun_is_up_by_day_and_down_by_night() {
    use geo::Sun;
    let tokyo = Coordinate::new(35.68, 139.77);
    // 2026-06-21 03:00 UTC is noon in Tokyo: the Sun near 90 - 35.7 + 23.4 ≈ 77.8°.
    let noon = 1_782_010_800_000;
    let e = Sun::elevation_deg(&tokyo, noon);
    assert!((e - 77.7).abs() < 1.0, "{e}");
    // 2026-06-21 15:00 UTC is midnight in Tokyo.
    assert!(Sun::is_down(&tokyo, noon + 12 * 3_600_000));
    assert!(!Sun::is_down(&tokyo, noon));
    // The equinox at the equator and the prime meridian: near the zenith at noon UTC.
    let equinox_noon = 1_774_008_000_000; // 2026-03-20 12:00 UTC
    let e = Sun::elevation_deg(&Coordinate::new(0.0, 0.0), equinox_noon);
    assert!(e > 87.0, "{e}");
}

#[test]
fn projection_is_the_closest_point() {
    // A bent line, and points around it: the projection is the closest point of the line, as
    // found by sampling every segment finely.
    let line = Line::new(vec![Coordinate::new(35.0, 139.0), Coordinate::new(35.001, 139.0005), Coordinate::new(35.001, 139.002), Coordinate::new(35.0, 139.003)]);
    for i in 0..40 {
        let location = Coordinate::new(34.9995 + (i % 8) as f64 * 0.0003, 138.9995 + (i / 8) as f64 * 0.0008);
        let p = line.project(&location, 0.0, f64::INFINITY).unwrap();
        let mut nearest = f64::INFINITY;
        for s in 0..line.points().len() - 1 {
            for k in 0..=1000 {
                let q = line.points()[s].lerp(&line.points()[s + 1], k as f64 / 1000.0);
                nearest = nearest.min(location.distance_m(&q));
            }
        }
        assert!((p.distance_m - nearest).abs() < 0.5, "{} vs {}", p.distance_m, nearest);
        assert!((location.distance_m(&p.point) - p.distance_m).abs() < 1e-6);
        assert!((line.point_at(p.along_m).unwrap().distance_m(&p.point)) < 0.01);
    }
}
