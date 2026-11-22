use crate::{Filled, PlaceMatch, Wheelchair};
use geo::{Bounds, Coordinate};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceSource {
    OpenStreetMap,
    Google,
    /// A point the user chose on the map, or typed as coordinates.
    Coordinate,
}

/// A place: a point with a name, and what is known of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// An id stable within its source: `osm:N123`, `google:0x…:0x…`, `geo:35.1,139.2`.
    pub id: String,
    pub source: PlaceSource,
    /// What the place is called, with its branch ("スターバックス 渋谷店").
    pub name: String,
    /// The branch, when the source gives it apart from the name (OpenStreetMap's `branch` tag):
    /// already part of `name`. See [`set_branch`](Self::set_branch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub coordinate: Coordinate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// What the place is ("駅", "restaurant"), as the source words it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// The opening hours, one line a day, as the source words them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hours: Vec<String>,
    /// Whether it is open now, as the source words it ("Open ⋅ Closes 22:00").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The bounds of the place, for areas (a city, a park).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// Whether a wheelchair can get in, when known (OpenStreetMap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wheelchair: Option<Wheelchair>,
    /// The ids of the same place in other sources (`osm:N123` for a place of Google's), found by
    /// [`PlaceMatch`](crate::PlaceMatch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    /// The fields filled from another source than `source`, and from which.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filled: Vec<Filled>,
}

impl Place {
    pub fn new(id: impl Into<String>, source: PlaceSource, name: impl Into<String>, coordinate: Coordinate) -> Place {
        Place {
            id: id.into(),
            source,
            name: name.into(),
            branch: None,
            coordinate,
            address: None,
            category: None,
            rating: None,
            review_count: None,
            phone: None,
            website: None,
            hours: Vec::new(),
            status: None,
            bounds: None,
            wheelchair: None,
            links: Vec::new(),
            filled: Vec::new(),
        }
    }

    /// Gives the place its branch: `branch` is added to the name after a space ("スターバックス"
    /// and "渋谷店" make "スターバックス 渋谷店"), as OpenStreetMap's wiki says of the `branch` tag,
    /// unless the name already holds it (spaces, case, full-width letters and kana folded as
    /// [`PlaceMatch::normalize`] does). A blank branch, or a second one, changes nothing.
    pub fn set_branch(&mut self, branch: &str) {
        let branch = branch.trim();
        if branch.is_empty() || self.branch.is_some() {
            return;
        }
        let folded = PlaceMatch::normalize(branch);
        if folded.is_empty() || !PlaceMatch::normalize(&self.name).contains(&folded) {
            self.name = format!("{} {branch}", self.name.trim_end());
        }
        self.branch = Some(branch.to_string());
    }

    /// A place for a bare point, named by its coordinates.
    pub fn at(coordinate: Coordinate) -> Place {
        let name = format!("{:.5}, {:.5}", coordinate.latitude, coordinate.longitude);
        Place::new(format!("geo:{:.6},{:.6}", coordinate.latitude, coordinate.longitude), PlaceSource::Coordinate, name, coordinate)
    }

    /// Reads "35.6812, 139.7671" (or with a space or semicolon) as a coordinate.
    pub fn parse_coordinate(text: &str) -> Option<Coordinate> {
        let parts: Vec<&str> = text.split([',', ' ', ';', '、']).map(str::trim).filter(|s| !s.is_empty()).collect();
        let [lat, lon] = parts.as_slice() else { return None };
        Coordinate::checked(lat.parse().ok()?, lon.parse().ok()?)
    }
}

/// A search for places.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    /// Where to search first (the map's centre, or the user's location).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near: Option<Coordinate>,
    /// The visible map, for geocoders that use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// Only places within `bounds`, instead of preferring them.
    #[serde(default)]
    pub bounded: bool,
    /// A BCP 47 language tag.
    pub language: String,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum SearchError {
    Network(String),
    Server(String),
    Parse(String),
    InvalidRequest(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::Network(s) => write!(f, "network: {s}"),
            SearchError::Server(s) => write!(f, "server: {s}"),
            SearchError::Parse(s) => write!(f, "unreadable response: {s}"),
            SearchError::InvalidRequest(s) => write!(f, "invalid request: {s}"),
        }
    }
}

impl std::error::Error for SearchError {}

impl SearchError {
    /// The error of a response that did not succeed, or `None` for a success.
    pub fn of_response(response: &fetch::Response) -> Option<SearchError> {
        if response.status == 0 {
            return Some(SearchError::Network(response.error.clone().unwrap_or_default()));
        }
        (!response.is_success()).then(|| SearchError::Server(format!("HTTP {}", response.status)))
    }
}

/// A suggestion while typing: a place, or a query to search for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    /// The main line ("東京タワー").
    pub text: String,
    /// The second line (its address), when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    /// The place, when the suggestion is one; `None` for a query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
}
