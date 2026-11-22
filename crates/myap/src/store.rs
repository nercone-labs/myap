//! The files of the core, each written whole and atomically (to a temporary file that then
//! replaces the old one), so that a crash never leaves half a file. A file that cannot be read is
//! kept aside as `.broken` and the core starts without it.

use crate::settings::GoogleUse;
use geocoding::Place;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A place the user keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPlace {
    pub place: Place,
    pub label: SavedLabel,
    pub saved_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedLabel {
    Home,
    Work,
    Favorite,
}

/// The places of the user: saved ones and the history of places looked at and searched.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Places {
    pub version: u32,
    pub saved: Vec<SavedPlace>,
    /// Most recent first.
    pub recent: Vec<Place>,
    pub searches: Vec<String>,
}

impl Places {
    pub const VERSION: u32 = 1;
    pub const MAX_RECENT: usize = 50;
    pub const MAX_SEARCHES: usize = 30;

    pub fn remember(&mut self, place: &Place) {
        self.recent.retain(|p| p.id != place.id);
        self.recent.insert(0, place.clone());
        self.recent.truncate(Self::MAX_RECENT);
    }

    pub fn remember_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        self.searches.retain(|q| q != query);
        self.searches.insert(0, query.to_string());
        self.searches.truncate(Self::MAX_SEARCHES);
    }

    pub fn save(&mut self, place: Place, label: SavedLabel, now: i64) {
        // Home and work are one each.
        if label != SavedLabel::Favorite {
            self.saved.retain(|s| s.label != label);
        }
        self.saved.retain(|s| s.place.id != place.id || s.label != label);
        self.saved.push(SavedPlace { place, label, saved_ms: now });
    }

    pub fn is_saved(&self, id: &str) -> bool {
        self.saved.iter().any(|s| s.place.id == id)
    }
}

/// A request sent to Google, as the record on the phone shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleRecord {
    pub time_ms: i64,
    pub purpose: GoogleUse,
    /// What was sent: the text, the places, the area (rounded as sent).
    pub sent: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GoogleLog {
    pub version: u32,
    /// Most recent first.
    pub records: Vec<GoogleRecord>,
}

impl GoogleLog {
    pub const MAX: usize = 500;

    pub fn add(&mut self, record: GoogleRecord) {
        self.records.insert(0, record);
        self.records.truncate(Self::MAX);
    }
}

/// The directory of the files; without one, nothing is kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Store {
    pub directory: Option<PathBuf>,
}

impl Store {
    pub const SETTINGS: &'static str = "settings.json";
    pub const PLACES: &'static str = "places.json";
    pub const GOOGLE_LOG: &'static str = "google_log.json";
    pub const REGIONS: &'static str = "regions.json";
    /// Where the graphs of the regions are kept.
    pub const REGIONS_DIRECTORY: &'static str = "regions";
    /// Where downloads wait to be built.
    pub const DOWNLOADS_DIRECTORY: &'static str = "downloads";

    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.directory.as_ref().map(|d| d.join(name))
    }

    /// Reads `name`; a missing file is `Ok(None)`. A file that cannot be read as JSON is kept
    /// aside (`.broken`) so that it is neither lost nor read again, and is an error.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, String> {
        let Some(path) = self.path(name) else { return Ok(None) };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{name}: {e}")),
        };
        match serde_json::from_str(&text) {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                let _ = fs::rename(&path, path.with_extension("json.broken"));
                Err(format!("{name} could not be read and was kept aside: {e}"))
            }
        }
    }

    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        let Some(path) = self.path(name) else { return Ok(()) };
        let text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
        Self::write_atomically(&path, &text).map_err(|e| format!("{name}: {e}"))
    }

    pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&temporary, path)
    }

    /// A directory under the data directory, created if missing.
    pub fn subdirectory(&self, name: &str) -> Option<PathBuf> {
        let path = self.path(name)?;
        fs::create_dir_all(&path).ok()?;
        Some(path)
    }
}
