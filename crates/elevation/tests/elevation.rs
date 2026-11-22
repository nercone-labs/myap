use elevation::{Dem, DemTile, ElevationError, Encoding, MercatorPoint, Raster, Source};
use geo::{Bounds, Coordinate, Tile};
use std::f64::consts::PI;
use std::path::PathBuf;

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// A minimal PNG encoder (8-bit RGB or RGBA, filter type 0), written from the PNG specification.
struct Png;

impl Png {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c: u32 = 0xFFFF_FFFF;
        for &b in bytes {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        c ^ 0xFFFF_FFFF
    }

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&Self::crc(&body).to_be_bytes());
    }

    /// `pixels` row by row, each of `channels` (3 or 4) bytes.
    fn encode(width: u32, height: u32, channels: usize, pixels: &[u8]) -> Vec<u8> {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, if channels == 4 { 6 } else { 2 }, 0, 0, 0]);
        Self::chunk(&mut out, b"IHDR", &ihdr);
        let mut raw = Vec::new();
        for row in pixels.chunks(width as usize * channels) {
            raw.push(0);
            raw.extend_from_slice(row);
        }
        Self::chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
        Self::chunk(&mut out, b"IEND", &[]);
        out
    }
}

/// The GSI encoding of the specification: x = h / u in 24-bit two's complement, u = 0.01 m.
fn gsi_rgb(metres: Option<f64>) -> [u8; 3] {
    let Some(metres) = metres else { return [128, 0, 0] };
    let x = (metres * 100.0).round() as i64;
    let x = if x < 0 { x + (1 << 24) } else { x } as u32;
    [(x >> 16) as u8, (x >> 8) as u8, x as u8]
}

/// A GSI tile of `size` by `size` pixels whose pixel at global indices `(gx, gy)` is `h(gx, gy)`.
fn gsi_tile(tile: Tile, size: u32, h: impl Fn(i64, i64) -> Option<f64>) -> Vec<u8> {
    let mut pixels = Vec::new();
    for py in 0..size {
        for px in 0..size {
            let gx = i64::from(tile.x) * i64::from(size) + i64::from(px);
            let gy = i64::from(tile.y) * i64::from(size) + i64::from(py);
            pixels.extend_from_slice(&gsi_rgb(h(gx, gy)));
        }
    }
    Png::encode(size, size, 3, &pixels)
}

/// The coordinate at global pixel coordinates `(x, y)` (Web Mercator, written independently).
fn at_pixel(x: f64, y: f64, zoom: u8, size: u32) -> Coordinate {
    let world = 2f64.powi(i32::from(zoom)) * f64::from(size);
    let longitude = x / world * 360.0 - 180.0;
    let latitude = (PI * (1.0 - 2.0 * y / world)).sinh().atan().to_degrees();
    Coordinate::new(latitude, longitude)
}

fn linear(gx: i64, gy: i64) -> Option<f64> {
    Some(10.0 * gx as f64 + 3.0 * gy as f64)
}

#[test]
fn terrarium_decoding() {
    // height = (R * 256 + G + B / 256) - 32768
    let t = Encoding::Terrarium;
    assert_eq!(t.height([128, 0, 0]), Some(0.0));
    assert_eq!(t.height([0, 0, 0]), Some(-32768.0));
    assert_eq!(t.height([255, 255, 255]), Some(32767.0 + 255.0 / 256.0));
    assert_eq!(t.height([130, 10, 128]), Some(522.5));
    assert_eq!(t.height([127, 255, 0]), Some(-1.0));
    assert_eq!(t.height([142, 192, 31]), Some(3776.0 + 31.0 / 256.0));
}

#[test]
fn gsi_decoding() {
    // x = R * 2^16 + G * 2^8 + B; h = x * u below 2^23, no data at 2^23, (x - 2^24) * u above.
    let g = Encoding::Gsi;
    assert_eq!(g.height([0, 0, 0]), Some(0.0));
    assert_eq!(g.height([0, 0, 1]), Some(0.01));
    assert_eq!(g.height([0, 1, 0]), Some(2.56));
    assert_eq!(g.height([0x05, 0xC3, 0x0C]), Some(3776.12));
    assert_eq!(g.height([0x7F, 0xFF, 0xFF]), Some(83886.07));
    assert_eq!(g.height([128, 0, 0]), None);
    assert_eq!(g.height(Encoding::GSI_NO_DATA), None);
    assert_eq!(g.height([128, 0, 1]), Some(-83886.07));
    assert_eq!(g.height([255, 255, 255]), Some(-0.01));
    assert_eq!(g.height([0xFF, 0xFF, 0x9C]), Some(-1.0));
    assert_eq!(g.height([0xFF, 0xF8, 0x30]), Some(-20.0));
}

#[test]
fn encoding_colors_round_trip() {
    for metres in [-32768.0, -12.5, 0.0, 3776.12109375, 32767.99609375] {
        let rgb = Encoding::Terrarium.color(metres).unwrap();
        assert_eq!(Encoding::Terrarium.height(rgb), Some(metres));
    }
    assert_eq!(Encoding::Terrarium.color(0.0), Some([128, 0, 0]));
    assert_eq!(Encoding::Terrarium.color(32768.0), None);
    assert_eq!(Encoding::Terrarium.color(-32768.1), None);
    for metres in [-83886.07, -20.0, -0.01, 0.0, 0.01, 3776.12, 83886.07] {
        let rgb = Encoding::Gsi.color(metres).unwrap();
        assert_eq!(rgb, gsi_rgb(Some(metres)));
        assert_eq!(Encoding::Gsi.height(rgb), Some(metres));
    }
    assert_eq!(Encoding::Gsi.color(83886.08), None);
    assert_eq!(Encoding::Gsi.color(f64::NAN), None);
}

#[test]
fn url_templates() {
    let tile = Tile { zoom: 15, x: 29094, y: 12926 };
    assert_eq!(Source::terrarium().url(&tile), "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/15/29094/12926.png");
    assert_eq!(Source::gsi_dem5a().url(&tile), "https://cyberjapandata.gsi.go.jp/xyz/dem5a_png/15/29094/12926.png");
    let tile = Tile { zoom: 14, x: 14547, y: 6463 };
    assert_eq!(Source::gsi().url(&tile), "https://cyberjapandata.gsi.go.jp/xyz/dem_png/14/14547/6463.png");
    let terrarium = Source::terrarium();
    assert_eq!((terrarium.min_zoom, terrarium.max_zoom, terrarium.encoding), (0, 15, Encoding::Terrarium));
    let gsi = Source::gsi();
    assert_eq!((gsi.min_zoom, gsi.max_zoom, gsi.encoding), (1, 14, Encoding::Gsi));
    assert_eq!((Source::gsi_dem5a().min_zoom, Source::gsi_dem5a().max_zoom), (1, 15));
    assert!(gsi.attribution.contains("国土地理院"));
    assert!(terrarium.attribution.contains("U.S. Geological Survey"));
}

#[test]
fn source_serde_round_trip() {
    let source = Source::gsi();
    let json = serde_json::to_string(&source).unwrap();
    assert_eq!(serde_json::from_str::<Source>(&json).unwrap(), source);
}

#[test]
fn file_names() {
    let tile = Tile { zoom: 15, x: 29094, y: 12926 };
    assert_eq!(Source::file_name(&tile), "15-29094-12926.png");
    assert_eq!(Source::parse_file_name("15-29094-12926.png"), Some(tile));
    assert_eq!(Source::parse_file_name("0-0-0.png"), Some(Tile { zoom: 0, x: 0, y: 0 }));
    for name in [
        "15-29094-12926.PNG",
        "15-29094-12926",
        "a-1-2.png",
        "1-2-0.png",
        "1-0-2.png",
        "1-+1-0.png",
        "1-1-0-0.png",
        "1--1-0.png",
        "31-0-0.png",
        "300-0-0.png",
        "",
    ] {
        assert_eq!(Source::parse_file_name(name), None, "{name}");
    }
}

#[test]
fn tiles_covering_bounds() {
    let terrarium = Source::terrarium();
    // Zoom 1: the north-west quarter of the world exactly. The tiles east and south of it only
    // share an edge with the bounds and are left out.
    let bounds = Bounds { south: 0.0, west: -180.0, north: 85.0, east: 0.0 };
    assert_eq!(terrarium.tiles(&bounds, 1), vec![Tile { zoom: 1, x: 0, y: 0 }]);
    // The whole world: four tiles at zoom 1, latitudes beyond Web Mercator clamped.
    let world = Bounds { south: -90.0, west: -180.0, north: 90.0, east: 180.0 };
    assert_eq!(terrarium.tiles(&world, 1).len(), 4);
    assert_eq!(terrarium.tile_count(&world, 3), 64);
    // Longitudes of tile column 5 at zoom 3 exactly (45 to 90 degrees).
    let bounds = Bounds { south: 10.0, west: 45.0, north: 20.0, east: 90.0 };
    let tiles = terrarium.tiles(&bounds, 3);
    assert!(tiles.iter().all(|t| t.x == 5), "{tiles:?}");
    // A point: the tile it is in.
    let tokyo = Coordinate::new(35.681236, 139.767125);
    let point = Bounds { south: tokyo.latitude, west: tokyo.longitude, north: tokyo.latitude, east: tokyo.longitude };
    assert_eq!(terrarium.tiles(&point, 12), vec![Tile::at(&tokyo, 12)]);
    // An area around Tokyo: from the tile of the north-west corner to that of the south-east
    // corner, row by row.
    let bounds = Bounds { south: 35.5, west: 139.5, north: 35.9, east: 140.0 };
    let tiles = terrarium.tiles(&bounds, 12);
    let (nw, se) = (Tile::at(&Coordinate::new(35.9, 139.5), 12), Tile::at(&Coordinate::new(35.5, 140.0), 12));
    assert_eq!(tiles.len() as u32, (se.x - nw.x + 1) * (se.y - nw.y + 1));
    assert_eq!(tiles.len() as u64, terrarium.tile_count(&bounds, 12));
    assert_eq!(tiles.first(), Some(&nw));
    assert_eq!(tiles.last(), Some(&se));
    assert_eq!(tiles[1], Tile { zoom: 12, x: nw.x + 1, y: nw.y });
    // Every tile overlaps the bounds.
    assert!(tiles.iter().all(|t| t.bounds().intersects(&bounds)));
    // The zoom is clamped to the source's range.
    assert!(terrarium.tiles(&point, 20).iter().all(|t| t.zoom == 15));
    assert!(Source::gsi().tiles(&point, 0).iter().all(|t| t.zoom == 1));
    // Invalid bounds give nothing.
    let inverted = Bounds { south: 1.0, west: 0.0, north: 0.0, east: 1.0 };
    assert!(terrarium.tiles(&inverted, 5).is_empty());
    let nan = Bounds { south: f64::NAN, west: 0.0, north: 0.0, east: 1.0 };
    assert_eq!(terrarium.tile_count(&nan, 5), 0);
}

#[test]
fn zoom_for() {
    let terrarium = Source::terrarium();
    // 45 degrees of longitude just north of the equator: one row, 2^(z - 3) columns from zoom 3.
    let bounds = Bounds { south: 0.0001, west: 0.0, north: 0.0002, east: 45.0 };
    for zoom in 3..=15 {
        assert_eq!(terrarium.tile_count(&bounds, zoom), 1 << (zoom - 3), "zoom {zoom}");
    }
    assert_eq!(terrarium.zoom_for(&bounds, 64, 15), 9);
    assert_eq!(terrarium.zoom_for(&bounds, 63, 15), 8);
    assert_eq!(terrarium.zoom_for(&bounds, 64, 8), 8);
    assert_eq!(terrarium.zoom_for(&bounds, 1, 15), 3);
    assert_eq!(terrarium.zoom_for(&bounds, 1 << 20, 20), 15);
    // Not even the lowest zoom fits: the lowest zoom.
    assert_eq!(terrarium.zoom_for(&bounds, 0, 15), 0);
    assert_eq!(Source::gsi().zoom_for(&bounds, 0, 15), 1);
}

#[test]
fn mercator() {
    let centre = MercatorPoint::from_coordinate(&Coordinate::new(0.0, 0.0)).unwrap();
    assert_eq!((centre.x, centre.y), (0.5, 0.5));
    let corner = MercatorPoint::from_coordinate(&Coordinate::new(Tile::MAX_LATITUDE, -180.0)).unwrap();
    assert!(close(corner.x, 0.0, 1e-15) && close(corner.y, 0.0, 1e-12), "{corner:?}");
    assert_eq!(MercatorPoint::from_coordinate(&Coordinate::new(86.0, 0.0)), None);
    assert_eq!(MercatorPoint::from_coordinate(&Coordinate::new(f64::NAN, 0.0)), None);
    // Pixel (px, py) of tile (x, y) has its centre at (x * 256 + px + 0.5, y * 256 + py + 0.5).
    let tile = Tile { zoom: 12, x: 3637, y: 1612 };
    let point = at_pixel(3637.0 * 256.0 + 10.5, 1612.0 * 256.0 + 20.5, 12, 256);
    let (x, y) = MercatorPoint::from_coordinate(&point).unwrap().to_pixel(12, 256, 256);
    assert!(close(x, 3637.0 * 256.0 + 10.5, 1e-6) && close(y, 1612.0 * 256.0 + 20.5, 1e-6), "{x} {y}");
    assert_eq!(Tile::at(&point, 12), tile);
    // The north-west corner of a tile is at its first global pixel.
    let (x, y) = MercatorPoint::from_coordinate(&tile.north_west()).unwrap().to_pixel(12, 256, 256);
    assert!(close(x, 3637.0 * 256.0, 1e-6) && close(y, 1612.0 * 256.0, 1e-6), "{x} {y}");
    let back = MercatorPoint::from_pixel(x, y, 12, 256, 256).to_coordinate();
    assert!(close(back.latitude, tile.north_west().latitude, 1e-9) && close(back.longitude, tile.north_west().longitude, 1e-9));
}

#[test]
fn decode_tile() {
    let tile = Tile { zoom: 3, x: 1, y: 2 };
    // Terrarium 2 by 1: 0 m and 522.5 m.
    let png = Png::encode(2, 1, 3, &[128, 0, 0, 130, 10, 128]);
    let dem = DemTile::decode(tile, Encoding::Terrarium, &png).unwrap();
    assert_eq!((dem.width, dem.height), (2, 1));
    assert_eq!(dem.heights, vec![0.0, 522.5]);
    assert_eq!(dem.pixel(1, 0), Some(522.5));
    assert_eq!(dem.pixel(2, 0), None);
    // GSI with no data, and a fully transparent pixel as no data.
    let png = Png::encode(3, 1, 4, &[0, 0, 1, 255, 128, 0, 0, 255, 0, 1, 0, 0]);
    let dem = DemTile::decode(tile, Encoding::Gsi, &png).unwrap();
    assert_eq!(dem.heights[0], 0.01);
    assert!(dem.heights[1].is_nan() && dem.heights[2].is_nan());
    assert_eq!(dem.pixel(1, 0), None);
    // Not a PNG, and a tile that does not exist.
    assert!(matches!(DemTile::decode(tile, Encoding::Gsi, b"not a png"), Err(ElevationError::Png(_))));
    let png = Png::encode(1, 1, 3, &[0, 0, 0]);
    assert!(matches!(DemTile::decode(Tile { zoom: 3, x: 8, y: 0 }, Encoding::Gsi, &png), Err(ElevationError::InvalidTile(_))));
    assert!(matches!(DemTile::new(tile, 2, 2, vec![0.0; 3]), Err(ElevationError::InvalidTile(_))));
}

#[test]
fn bilinear_within_a_tile() {
    // Zoom 2, tiles of 4 by 4 pixels; the height is linear in the global pixel indices, which
    // bilinear interpolation reproduces exactly between pixel centres.
    let (zoom, size) = (2, 4);
    let tile = Tile { zoom, x: 1, y: 1 };
    let dem = DemTile::decode(tile, Encoding::Gsi, &gsi_tile(tile, size, linear)).unwrap();
    for (x, y) in [(4.5, 4.5), (5.25, 6.75), (6.0, 5.0), (7.5, 7.5), (4.9, 7.1)] {
        let expected = 10.0 * (x - 0.5) + 3.0 * (y - 0.5);
        let h = dem.height_at(&at_pixel(x, y, zoom, size)).unwrap();
        assert!(close(h, expected, 1e-6), "({x}, {y}): {h} != {expected}");
    }
    // Between the edge of the tile and the first pixel centres, the pixels beyond the tile are
    // missing: the nearest pixel that has data is used.
    let h = dem.height_at(&at_pixel(4.2, 6.5, zoom, size)).unwrap();
    assert!(close(h, 10.0 * 4.0 + 3.0 * 6.0, 1e-6), "{h}");
    // Outside the tile.
    assert_eq!(dem.height_at(&at_pixel(3.9, 6.5, zoom, size)), None);
    assert_eq!(dem.height_at(&at_pixel(8.0, 6.5, zoom, size)), None);
    assert_eq!(dem.height_at(&Coordinate::new(89.0, 0.0)), None);
}

#[test]
fn standard_tile_size() {
    // A 256 by 256 tile: the pixel centres of the specification of the task are at + 0.5.
    let tile = Tile { zoom: 12, x: 3637, y: 1612 };
    let dem = DemTile::decode(tile, Encoding::Gsi, &gsi_tile(tile, 256, |gx, gy| Some(((gx % 256) * 100 + gy % 256) as f64))).unwrap();
    let centre = at_pixel(3637.0 * 256.0 + 10.5, 1612.0 * 256.0 + 20.5, 12, 256);
    assert!(close(dem.height_at(&centre).unwrap(), 1020.0, 1e-6));
    let between = at_pixel(3637.0 * 256.0 + 11.0, 1612.0 * 256.0 + 21.0, 12, 256);
    assert!(close(dem.height_at(&between).unwrap(), (1020.0 + 1120.0 + 1021.0 + 1121.0) / 4.0, 1e-6));
}

fn dem_of(zoom: u8, size: u32, tiles: &[(u32, u32)], h: impl Fn(i64, i64) -> Option<f64> + Copy) -> Dem {
    let mut dem = Dem::new(zoom);
    for &(x, y) in tiles {
        let tile = Tile { zoom, x, y };
        dem.insert(DemTile::decode(tile, Encoding::Gsi, &gsi_tile(tile, size, h)).unwrap()).unwrap();
    }
    dem
}

#[test]
fn bilinear_across_tile_edges() {
    let (zoom, size) = (2, 4);
    let dem = dem_of(zoom, size, &[(1, 1), (2, 1), (1, 2)], linear);
    // Between the last pixel centre of tile (1, 1) at x = 7.5 and the first of tile (2, 1) at
    // x = 8.5, on both sides of the edge.
    for (x, y) in [(7.9, 6.5), (8.2, 6.5), (8.0, 5.3), (7.6, 7.4), (6.3, 8.2)] {
        let expected = 10.0 * (x - 0.5) + 3.0 * (y - 0.5);
        let h = dem.height(&at_pixel(x, y, zoom, size)).unwrap();
        assert!(close(h, expected, 1e-6), "({x}, {y}): {h} != {expected}");
    }
    // Where tile (2, 2) is missing, its pixels count as no data: near the corner, the nearest
    // valid of the four is used.
    let h = dem.height(&at_pixel(7.9, 7.9, zoom, size)).unwrap();
    assert!(close(h, 10.0 * 7.0 + 3.0 * 7.0, 1e-6), "{h}");
    // A point in a tile that is not loaded.
    assert_eq!(dem.height(&at_pixel(9.0, 9.0, zoom, size)), None);
    assert_eq!(dem.height(&at_pixel(1.0, 1.0, zoom, size)), None);
    // The same point without tile (2, 1) falls back to the nearest pixel.
    let alone = dem_of(zoom, size, &[(1, 1)], linear);
    let h = alone.height(&at_pixel(7.9, 6.5, zoom, size)).unwrap();
    assert!(close(h, 10.0 * 7.0 + 3.0 * 6.0, 1e-6), "{h}");
}

#[test]
fn interpolation_wraps_around_the_antimeridian() {
    // Zoom 1, tiles of 2 pixels: global columns 0 to 3. West of the first pixel centre, the
    // neighbour is column 3, across longitude 180.
    let dem = dem_of(1, 2, &[(0, 0), (1, 0)], |gx, _| Some(gx as f64 * 100.0));
    let h = dem.height(&at_pixel(0.25, 0.75, 1, 2)).unwrap();
    assert!(close(h, 0.25 * 300.0 + 0.75 * 0.0, 1e-6), "{h}");
    assert!(close(dem.value(-1, 0).unwrap(), 300.0, 0.0));
    assert_eq!(dem.value(0, -1), None);
    assert_eq!(dem.value(0, 4), None);
}

#[test]
fn no_data() {
    let (zoom, size) = (2, 4);
    // Only pixel (5, 5) has data in tile (1, 1).
    let dem = dem_of(zoom, size, &[(1, 1)], |gx, gy| if (gx, gy) == (5, 5) { Some(42.0) } else { None });
    // Among the four around (5.9, 5.9), only (5, 5) has data.
    assert_eq!(dem.height(&at_pixel(5.9, 5.9, zoom, size)), Some(42.0));
    // None of the four around (6.9, 6.9) has data.
    assert_eq!(dem.height(&at_pixel(6.9, 6.9, zoom, size)), None);
    let tile = dem.get(1, 1).unwrap();
    assert_eq!(tile.height_at(&at_pixel(6.9, 6.9, zoom, size)), None);
    assert_eq!(tile.height_at(&at_pixel(5.2, 5.7, zoom, size)), Some(42.0));
    // One no-data pixel among four: the nearest valid one, not a blend with a made-up value.
    let dem = dem_of(zoom, size, &[(1, 1)], |gx, gy| if (gx, gy) == (5, 5) { None } else { linear(gx, gy) });
    let h = dem.height(&at_pixel(5.6, 5.8, zoom, size)).unwrap();
    assert!(close(h, 10.0 * 5.0 + 3.0 * 5.0 + 3.0, 1e-6), "{h}"); // (5, 6): nearest after (5, 5)
}

#[test]
fn dem_insert_rules() {
    let mut dem = Dem::new(2);
    assert!(dem.is_empty());
    assert!(dem.insert(DemTile::new(Tile { zoom: 2, x: 0, y: 0 }, 2, 2, vec![0.0; 4]).unwrap()).unwrap().is_none());
    assert_eq!(dem.tile_size(), Some((2, 2)));
    // The same tile again replaces it.
    assert!(dem.insert(DemTile::new(Tile { zoom: 2, x: 0, y: 0 }, 2, 2, vec![1.0; 4]).unwrap()).unwrap().is_some());
    assert!(matches!(dem.insert(DemTile::new(Tile { zoom: 3, x: 0, y: 0 }, 2, 2, vec![0.0; 4]).unwrap()), Err(ElevationError::InvalidTile(_))));
    assert!(matches!(dem.insert(DemTile::new(Tile { zoom: 2, x: 1, y: 0 }, 3, 3, vec![0.0; 9]).unwrap()), Err(ElevationError::InvalidTile(_))));
    let broken = DemTile { tile: Tile { zoom: 2, x: 1, y: 0 }, width: 2, height: 2, heights: vec![0.0; 3] };
    assert!(matches!(dem.insert(broken), Err(ElevationError::InvalidTile(_))));
    assert_eq!(dem.len(), 1);
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("elevation-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn load_directory() {
    let dir = TempDir::new("load");
    let (zoom, size) = (2, 4);
    for (x, y) in [(1, 1), (2, 1)] {
        let tile = Tile { zoom, x, y };
        std::fs::write(dir.0.join(Source::file_name(&tile)), gsi_tile(tile, size, linear)).unwrap();
    }
    // Corrupt: a PNG cut in half, and not a PNG at all.
    let tile = Tile { zoom, x: 3, y: 1 };
    let png = gsi_tile(tile, size, linear);
    std::fs::write(dir.0.join(Source::file_name(&tile)), &png[..png.len() / 2]).unwrap();
    std::fs::write(dir.0.join("2-0-0.png"), b"garbage").unwrap();
    // Ignored: another zoom, other names, a directory.
    std::fs::write(dir.0.join("3-0-0.png"), gsi_tile(Tile { zoom: 3, x: 0, y: 0 }, size, linear)).unwrap();
    std::fs::write(dir.0.join("notes.txt"), b"hello").unwrap();
    std::fs::create_dir(dir.0.join("sub")).unwrap();

    let (dem, skipped) = Dem::load_directory(&dir.0, zoom, Encoding::Gsi).unwrap();
    assert_eq!(skipped, 2);
    assert_eq!(dem.len(), 2);
    assert!(dem.get(1, 1).is_some() && dem.get(2, 1).is_some());
    let h = dem.height(&at_pixel(7.9, 6.5, zoom, size)).unwrap();
    assert!(close(h, 10.0 * 7.4 + 3.0 * 6.0, 1e-6), "{h}");

    assert!(matches!(Dem::load_directory(&dir.0.join("missing"), zoom, Encoding::Gsi), Err(ElevationError::Io(_))));
}
