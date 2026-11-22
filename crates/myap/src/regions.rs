use geo::Bounds;
use serde::{Deserialize, Serialize};

/// Where the data of a region comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegionSource {
    /// An area of the map, from the Overpass API.
    Area { bounds: Bounds },
    /// A PBF file at a URL (for example an extract of Geofabrik).
    Url { url: String },
    /// A PBF file the user chose on the device, copied by the platform to `path` (removed once
    /// built).
    File {
        #[serde(default)]
        path: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionStatus {
    Downloading,
    Building,
    /// Built; its elevations are being downloaded and added (it routes meanwhile).
    Elevating,
    Ready,
    Failed,
}

impl RegionStatus {
    /// Whether its graph can be routed on.
    pub fn is_usable(&self) -> bool {
        matches!(self, RegionStatus::Ready | RegionStatus::Elevating)
    }
}

/// A region for Myap Router: a graph built from OpenStreetMap data, kept on the device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub source: RegionSource,
    pub status: RegionStatus,
    /// The bounds of the roads of the graph, once built.
    #[serde(default)]
    pub bounds: Option<Bounds>,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub built_ms: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
    /// The fetch or build in progress, to match its answer.
    #[serde(default)]
    pub job: Option<u64>,
    /// Whether its graph has elevations, and where they come from.
    #[serde(default)]
    pub elevations: Option<crate::settings::ElevationSource>,
    /// How far the work in progress is, from 0 to 1, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Regions {
    pub version: u32,
    pub regions: Vec<Region>,
}

impl Regions {
    /// The largest area asked of Overpass at once: larger areas belong in a PBF extract.
    pub const MAX_AREA_KM2: f64 = 1_500.0;

    /// The ready region holding every point, the smallest first.
    pub fn covering(&self, points: &[geo::Coordinate]) -> Option<&Region> {
        self.regions
            .iter()
            .filter(|r| r.status.is_usable())
            .filter(|r| r.bounds.is_some_and(|b| points.iter().all(|p| b.expanded_m(500.0).contains(p))))
            .min_by(|a, b| a.bounds.map_or(f64::MAX, |x| x.area_km2()).total_cmp(&b.bounds.map_or(f64::MAX, |x| x.area_km2())))
    }

    /// Makes an id that is safe as a file name and not used yet.
    pub fn new_id(&self, now_ms: i64) -> String {
        let mut n = now_ms.max(0) as u64;
        loop {
            let id = format!("r{n:x}");
            if !self.regions.iter().any(|r| r.id == id) {
                return id;
            }
            n += 1;
        }
    }
}
