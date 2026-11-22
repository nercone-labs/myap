use serde::{Deserialize, Serialize};

/// The raster layers of Google's map tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileLayer {
    Roadmap,
    Satellite,
    /// Satellite with roads and labels.
    Hybrid,
    Terrain,
    /// The traffic of now, transparent, to lay over another map.
    Traffic,
}

/// The URL templates of Google's map tiles, in the `{z}/{x}/{y}` placeholders of MapLibre.
pub struct Tiles;

impl Tiles {
    /// The highest zoom of the traffic layer: Google throttles requests beyond it.
    pub const TRAFFIC_MAX_ZOOM: u8 = 16;

    /// The templates of `layer`, one per server (requests are spread over them), with the
    /// language (`hl`) and region (`gl`) of the labels.
    pub fn templates(layer: TileLayer, language: &str, region: &str) -> Vec<String> {
        let hl = Self::code(language, "en");
        let gl = Self::code(region, "us");
        let letter = match layer {
            TileLayer::Roadmap => "m",
            TileLayer::Satellite => "s",
            TileLayer::Hybrid => "y",
            TileLayer::Terrain => "p",
            TileLayer::Traffic => {
                return vec![format!(
                    "https://www.google.com/maps/vt/pb=!1m4!1m3!1i{{z}}!2i{{x}}!3i{{y}}!2m9!1e2!2straffic!3i999999!4m2!1sincidents!2s1!4m2!1sincidents_text!2s1!3m8!2s{hl}!3s{gl}!5e1105!12m4!1e68!2m2!1sset!2sRoadmap!4e0!5m1!1e0"
                )];
            }
        };
        (0..4).map(|n| format!("https://mt{n}.google.com/vt/lyrs={letter}&hl={hl}&gl={gl}&x={{x}}&y={{y}}&z={{z}}")).collect()
    }

    pub fn max_zoom(layer: TileLayer) -> u8 {
        match layer {
            TileLayer::Traffic => Self::TRAFFIC_MAX_ZOOM,
            _ => 21,
        }
    }

    /// The primary subtag of a BCP 47 tag, or a region code, lowercased; only letters, so that
    /// nothing else can enter the URL.
    fn code(text: &str, fallback: &str) -> String {
        let code: String = text.split(['-', '_']).next().unwrap_or("").chars().filter(char::is_ascii_alphabetic).take(3).collect();
        if code.is_empty() { fallback.into() } else { code.to_ascii_lowercase() }
    }
}
