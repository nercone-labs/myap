use crate::{Address, Geocoder, Place, PlaceSource, SearchError, SearchRequest, Wheelchair};
use fetch::{Query, Request, Response};
use geo::{Bounds, Coordinate};
use serde::Deserialize;
use std::collections::BTreeMap;

/// A Nominatim server.
#[derive(Debug, Clone, PartialEq)]
pub struct Nominatim {
    /// The base URL, before `/search` and `/reverse`.
    pub base: String,
    pub user_agent: String,
}

#[derive(Deserialize)]
struct RawPlace {
    #[serde(default)]
    osm_type: Option<String>,
    #[serde(default)]
    osm_id: Option<i64>,
    lat: String,
    lon: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    address: BTreeMap<String, String>,
    /// `[south, north, west, east]`, as strings.
    #[serde(default)]
    boundingbox: Option<[String; 4]>,
    /// The other tags of the element, asked with `extratags=1` (values may be `null`).
    #[serde(default)]
    extratags: Option<BTreeMap<String, Option<String>>>,
    #[serde(default)]
    error: Option<String>,
}

impl Nominatim {
    /// The public server of the OpenStreetMap Foundation: one request a second at most.
    pub fn openstreetmap(user_agent: &str) -> Nominatim {
        Nominatim { base: "https://nominatim.openstreetmap.org".into(), user_agent: user_agent.into() }
    }

    fn first<'a>(address: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a String> {
        keys.iter().find_map(|k| address.get(*k))
    }

    fn place(raw: RawPlace) -> Option<Place> {
        let coordinate = Coordinate::checked(raw.lat.parse().ok()?, raw.lon.parse().ok()?)?;
        let a = &raw.address;
        let address = Address {
            country_code: a.get("country_code").cloned(),
            country: a.get("country").cloned(),
            postcode: a.get("postcode").cloned(),
            state: Self::first(a, &["state", "province", "region"]).cloned(),
            city: Self::first(a, &["city", "town", "village", "municipality", "county"]).cloned(),
            district: Self::first(a, &["city_district", "district", "suburb", "borough"]).cloned(),
            locality: Self::first(a, &["neighbourhood", "quarter", "hamlet"]).cloned(),
            street: Self::first(a, &["road", "pedestrian", "footway"]).cloned(),
            house_number: a.get("house_number").cloned(),
        };
        let formatted = address.format();
        let named = raw.name.as_deref().is_some_and(|n| !n.is_empty());
        let name = raw
            .name
            .filter(|n| !n.is_empty())
            .or_else(|| formatted.clone())
            .or_else(|| raw.display_name.as_deref().and_then(|d| d.split(',').next()).map(|s| s.trim().to_string()))?;
        let id = match (raw.osm_type.as_deref(), raw.osm_id) {
            (Some(t), Some(id)) => format!("osm:{}{id}", t.chars().next().unwrap_or('X').to_ascii_uppercase()),
            _ => format!("geo:{:.6},{:.6}", coordinate.latitude, coordinate.longitude),
        };
        let mut place = Place::new(id, PlaceSource::OpenStreetMap, name, coordinate);
        place.address = formatted.or(raw.display_name);
        place.category = raw.kind.filter(|k| k != "yes").or(raw.category);
        place.bounds = raw.boundingbox.and_then(|[s, n, w, e]| {
            let b = Bounds { south: s.parse().ok()?, north: n.parse().ok()?, west: w.parse().ok()?, east: e.parse().ok()? };
            (b.south <= b.north && b.west <= b.east).then_some(b)
        });
        if let Some(tags) = &raw.extratags {
            Self::read_tags(&mut place, tags);
            // A branch belongs to a name, not to the address that stands for a place without one.
            if let Some(branch) = tags.get("branch").and_then(|v| v.as_deref()).filter(|_| named) {
                place.set_branch(branch);
            }
        }
        Some(place)
    }

    /// What the tags of the element say of the place: its phone, website, opening hours
    /// and wheelchair access, by the keys the wiki documents (`contact:*` after the plain key).
    fn read_tags(place: &mut Place, tags: &BTreeMap<String, Option<String>>) {
        let tag = |keys: &[&str]| keys.iter().find_map(|k| tags.get(*k).and_then(|v| v.as_deref()).map(str::trim).filter(|v| !v.is_empty()));
        // Several values are separated by semicolons; the first is shown.
        let first = |v: &str| v.split(';').next().unwrap_or(v).trim().to_string();
        place.phone = tag(&["phone", "contact:phone", "mobile", "contact:mobile"]).map(first);
        place.website = tag(&["website", "contact:website", "url"]).map(first).filter(|w| w.starts_with("https://") || w.starts_with("http://"));
        // The rules of `opening_hours`, one a line, as OpenStreetMap writes them ("Mo-Fr 09:00-18:00").
        place.hours = tag(&["opening_hours"]).map(|v| v.split(';').map(str::trim).filter(|r| !r.is_empty()).map(str::to_string).collect()).unwrap_or_default();
        place.wheelchair = tag(&["wheelchair"]).and_then(Wheelchair::of_tag);
    }

    /// The request for the elements of `ids` (`osm:N123`, `osm:W45`, `osm:R6`; at most 50), with
    /// their tags. Ids of another form are left out; none left is an error.
    pub fn lookup_request(&self, ids: &[&str], language: &str) -> Result<Request, SearchError> {
        let wanted: Vec<&str> = ids
            .iter()
            .filter_map(|id| id.strip_prefix("osm:"))
            .filter(|id| {
                let mut chars = id.chars();
                matches!(chars.next(), Some('N' | 'W' | 'R')) && !chars.as_str().is_empty() && chars.all(|c| c.is_ascii_digit())
            })
            .take(50)
            .collect();
        if wanted.is_empty() {
            return Err(SearchError::InvalidRequest("no OpenStreetMap id".into()));
        }
        let q = Query::new()
            .pair("osm_ids", wanted.join(","))
            .pair("format", "jsonv2")
            .pair("addressdetails", "1")
            .pair("extratags", "1")
            .pair("accept-language", language);
        Ok(Request::get(q.append_to(&format!("{}/lookup", self.base.trim_end_matches('/')))).header("User-Agent", &self.user_agent))
    }

    /// The places of a response to [`lookup_request`](Self::lookup_request).
    pub fn parse_lookup(&self, response: &Response) -> Result<Vec<Place>, SearchError> {
        self.parse_search(response)
    }
}

impl Geocoder for Nominatim {
    fn search_request(&self, request: &SearchRequest) -> Result<Request, SearchError> {
        let query = request.query.trim();
        if query.is_empty() {
            return Err(SearchError::InvalidRequest("empty query".into()));
        }
        let mut q = Query::new()
            .pair("q", query)
            .pair("format", "jsonv2")
            .pair("addressdetails", "1")
            .pair("extratags", "1")
            .pair("limit", request.limit.clamp(1, 40).to_string())
            .pair("accept-language", &request.language);
        if let Some(b) = request.bounds {
            // A preference, unless the request is bounded.
            q = q.pair("viewbox", format!("{:.5},{:.5},{:.5},{:.5}", b.west, b.north, b.east, b.south));
            if request.bounded {
                q = q.pair("bounded", "1");
            }
        }
        Ok(Request::get(q.append_to(&format!("{}/search", self.base.trim_end_matches('/')))).header("User-Agent", &self.user_agent))
    }

    fn parse_search(&self, response: &Response) -> Result<Vec<Place>, SearchError> {
        if let Some(e) = SearchError::of_response(response) {
            return Err(e);
        }
        let raw: Vec<RawPlace> = serde_json::from_str(&response.body).map_err(|e| SearchError::Parse(e.to_string()))?;
        Ok(raw.into_iter().filter_map(Self::place).collect())
    }

    fn reverse_request(&self, coordinate: &Coordinate, language: &str) -> Result<Request, SearchError> {
        let q = Query::new()
            .pair("lat", format!("{:.6}", coordinate.latitude))
            .pair("lon", format!("{:.6}", coordinate.longitude))
            .pair("format", "jsonv2")
            .pair("zoom", "18")
            .pair("addressdetails", "1")
            .pair("accept-language", language);
        Ok(Request::get(q.append_to(&format!("{}/reverse", self.base.trim_end_matches('/')))).header("User-Agent", &self.user_agent))
    }

    fn parse_reverse(&self, response: &Response) -> Result<Option<Place>, SearchError> {
        if let Some(e) = SearchError::of_response(response) {
            return Err(e);
        }
        let raw: RawPlace = match serde_json::from_str(&response.body) {
            Ok(raw) => raw,
            // "Unable to geocode" comes as `{"error": ...}` without coordinates.
            Err(_) if response.body.contains("\"error\"") => return Ok(None),
            Err(e) => return Err(SearchError::Parse(e.to_string())),
        };
        if raw.error.is_some() {
            return Ok(None);
        }
        Ok(Self::place(raw))
    }
}
