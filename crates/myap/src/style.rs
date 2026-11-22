//! The map: its style for MapLibre (the base map, chosen in the settings), the layers laid over it
//! (traffic), and the features Myap draws on it (routes, places) as GeoJSON.

use crate::settings::{GoogleUse, MapSource, Settings};
use geo::{Bounds, Coordinate};
use google::{TileLayer, Tiles};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A raster layer over the base map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterOverlay {
    pub id: String,
    pub tiles: Vec<String>,
    pub max_zoom: u8,
    pub opacity: f64,
}

/// The style of the map for MapLibre: a URL of a style, or a style itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<String>,
    pub overlays: Vec<RasterOverlay>,
    /// The attribution to show on the map.
    pub attribution: String,
    /// Whether the base map is drawn from Google (the screen says so).
    pub google: bool,
}

/// What a feature on the map is, for the platform to draw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    Route,
    RouteAlternative,
    RoutePassed,
    Result,
    Selected,
    Origin,
    Destination,
    Stop,
    Saved,
    /// A part of the route by its congestion (property `congestion`).
    RouteTraffic,
    /// An area to avoid (a polygon).
    AvoidArea,
    /// Something to warn of on the route (property `icon`).
    Alert,
}

pub struct Style;

impl Style {
    /// The glyphs of OpenFreeMap, for the labels Myap draws on raster maps.
    const GLYPHS: &'static str = "https://tiles.openfreemap.org/fonts/{fontstack}/{range}.pbf";
    const OSM_ATTRIBUTION: &'static str = "© OpenStreetMap contributors";

    pub fn map(settings: &Settings, dark: bool, language: &str, region: &str) -> MapStyle {
        let source = settings.effective_map();
        let mut overlays = Vec::new();
        if settings.google.allows(GoogleUse::TrafficLayer) {
            overlays.push(RasterOverlay {
                id: "google-traffic".into(),
                tiles: Tiles::templates(TileLayer::Traffic, language, region),
                max_zoom: Tiles::max_zoom(TileLayer::Traffic),
                opacity: 0.7,
            });
        }
        let google_layer = match source {
            MapSource::GoogleRoadmap => Some(TileLayer::Roadmap),
            MapSource::GoogleSatellite => Some(TileLayer::Satellite),
            MapSource::GoogleHybrid => Some(TileLayer::Hybrid),
            MapSource::GoogleTerrain => Some(TileLayer::Terrain),
            _ => None,
        };
        match (source, google_layer) {
            (_, Some(layer)) => MapStyle {
                url: None,
                json: Some(Self::raster(&Tiles::templates(layer, language, region), 256, Tiles::max_zoom(layer), "© Google").to_string()),
                overlays,
                attribution: "© Google".into(),
                google: true,
            },
            (MapSource::OpenStreetMapCarto, _) => MapStyle {
                url: None,
                json: Some(Self::raster(&["https://tile.openstreetmap.org/{z}/{x}/{y}.png".to_string()], 256, 19, Self::OSM_ATTRIBUTION).to_string()),
                overlays,
                attribution: Self::OSM_ATTRIBUTION.into(),
                google: false,
            },
            _ => MapStyle {
                url: Some(format!("https://tiles.openfreemap.org/styles/{}", if dark { "dark" } else { "liberty" })),
                json: None,
                overlays,
                attribution: format!("OpenFreeMap {} · OpenMapTiles", Self::OSM_ATTRIBUTION),
                google: false,
            },
        }
    }

    /// A style of one raster source.
    fn raster(tiles: &[String], tile_size: u32, max_zoom: u8, attribution: &str) -> Value {
        json!({
            "version": 8,
            "glyphs": Self::GLYPHS,
            "sources": {
                "base": { "type": "raster", "tiles": tiles, "tileSize": tile_size, "maxzoom": max_zoom, "attribution": attribution }
            },
            "layers": [ { "id": "base", "type": "raster", "source": "base" } ]
        })
    }

    pub fn line(kind: FeatureKind, points: &[Coordinate], properties: Value) -> Value {
        let coordinates: Vec<[f64; 2]> = points.iter().map(|p| [Self::round(p.longitude), Self::round(p.latitude)]).collect();
        let mut props = properties;
        props["kind"] = json!(kind);
        json!({ "type": "Feature", "geometry": { "type": "LineString", "coordinates": coordinates }, "properties": props })
    }

    pub fn point(kind: FeatureKind, point: &Coordinate, properties: Value) -> Value {
        let mut props = properties;
        props["kind"] = json!(kind);
        json!({ "type": "Feature", "geometry": { "type": "Point", "coordinates": [Self::round(point.longitude), Self::round(point.latitude)] }, "properties": props })
    }

    pub fn polygon(kind: FeatureKind, ring: &[Coordinate], properties: Value) -> Value {
        let mut coordinates: Vec<[f64; 2]> = ring.iter().map(|p| [Self::round(p.longitude), Self::round(p.latitude)]).collect();
        if let Some(first) = coordinates.first().copied() {
            coordinates.push(first);
        }
        let mut props = properties;
        props["kind"] = json!(kind);
        json!({ "type": "Feature", "geometry": { "type": "Polygon", "coordinates": [coordinates] }, "properties": props })
    }

    pub fn collection(features: Vec<Value>) -> String {
        json!({ "type": "FeatureCollection", "features": features }).to_string()
    }

    /// Seven decimals: about a centimetre, as precise as the data.
    fn round(v: f64) -> f64 {
        (v * 1e7).round() / 1e7
    }

    /// The bounds of points, at least `min_m` wide, for the camera.
    pub fn frame(points: &[Coordinate], min_m: f64) -> Option<Bounds> {
        let b = Bounds::of(points)?;
        let size = b.center().distance_m(&Coordinate::new(b.north, b.east));
        Some(if size < min_m { Bounds::of(&[b.center()])?.expanded_m(min_m) } else { b })
    }
}
