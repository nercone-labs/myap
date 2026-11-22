use crate::{Address, Geocoder, Place, PlaceSource, SearchError, SearchRequest};
use fetch::{Query, Request, Response};
use geo::{Bounds, Coordinate};
use serde::Deserialize;

/// A Photon server.
#[derive(Debug, Clone, PartialEq)]
pub struct Photon {
    /// The base URL, before `/api`.
    pub base: String,
    pub user_agent: String,
}

#[derive(Deserialize)]
struct Collection {
    #[serde(default)]
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    geometry: Geometry,
    #[serde(default)]
    properties: Properties,
}

#[derive(Deserialize)]
struct Geometry {
    coordinates: [f64; 2],
}

#[derive(Deserialize, Default)]
struct Properties {
    osm_type: Option<String>,
    osm_id: Option<i64>,
    osm_key: Option<String>,
    osm_value: Option<String>,
    name: Option<String>,
    housenumber: Option<String>,
    street: Option<String>,
    locality: Option<String>,
    district: Option<String>,
    city: Option<String>,
    county: Option<String>,
    state: Option<String>,
    country: Option<String>,
    countrycode: Option<String>,
    postcode: Option<String>,
    /// `[west, north, east, south]`.
    extent: Option<[f64; 4]>,
}

impl Photon {
    /// The public server of komoot (fair use, no search as you type at high volume).
    pub fn komoot(user_agent: &str) -> Photon {
        Photon { base: "https://photon.komoot.io".into(), user_agent: user_agent.into() }
    }

    /// The languages Photon's public server has names in; others get the local names.
    const LANGUAGES: [&'static str; 4] = ["en", "de", "fr", "it"];

    fn language(tag: &str) -> &'static str {
        let primary = tag.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        Self::LANGUAGES.iter().find(|l| **l == primary).copied().unwrap_or("default")
    }

    fn places(response: &Response) -> Result<Vec<Place>, SearchError> {
        if let Some(e) = SearchError::of_response(response) {
            return Err(e);
        }
        let collection: Collection = serde_json::from_str(&response.body).map_err(|e| SearchError::Parse(e.to_string()))?;
        Ok(collection.features.into_iter().filter_map(Self::place).collect())
    }

    fn place(feature: Feature) -> Option<Place> {
        let [lon, lat] = feature.geometry.coordinates;
        let coordinate = Coordinate::checked(lat, lon)?;
        let p = feature.properties;
        let address = Address {
            country_code: p.countrycode.clone(),
            country: p.country.clone(),
            postcode: p.postcode.clone(),
            state: p.state.clone(),
            city: p.city.clone().or(p.county.clone()),
            district: p.district.clone(),
            locality: p.locality.clone(),
            street: p.street.clone(),
            house_number: p.housenumber.clone(),
        };
        let name = p.name.clone().or_else(|| match (&p.street, &p.housenumber) {
            (Some(s), Some(n)) => Some(format!("{s} {n}")),
            (Some(s), None) => Some(s.clone()),
            _ => p.city.clone(),
        })?;
        let id = match (p.osm_type.as_deref(), p.osm_id) {
            (Some(t), Some(id)) => format!("osm:{t}{id}"),
            _ => format!("geo:{lat:.6},{lon:.6}"),
        };
        let mut place = Place::new(id, PlaceSource::OpenStreetMap, name, coordinate);
        place.address = address.format();
        place.category = p.osm_value.filter(|v| v != "yes").or(p.osm_key);
        place.bounds = p.extent.and_then(|[west, north, east, south]| {
            let b = Bounds { south, west, north, east };
            (south <= north && west <= east).then_some(b)
        });
        Some(place)
    }
}

impl Geocoder for Photon {
    fn search_request(&self, request: &SearchRequest) -> Result<Request, SearchError> {
        let query = request.query.trim();
        if query.is_empty() {
            return Err(SearchError::InvalidRequest("empty query".into()));
        }
        let mut q = Query::new().pair("q", query).pair("limit", request.limit.clamp(1, 50).to_string()).pair("lang", Self::language(&request.language));
        if let Some(near) = request.near {
            q = q.pair("lat", format!("{:.5}", near.latitude)).pair("lon", format!("{:.5}", near.longitude));
        }
        if let (true, Some(b)) = (request.bounded, request.bounds) {
            // `minLon,minLat,maxLon,maxLat`.
            q = q.pair("bbox", format!("{:.5},{:.5},{:.5},{:.5}", b.west, b.south, b.east, b.north));
        }
        Ok(Request::get(q.append_to(&format!("{}/api/", self.base.trim_end_matches('/')))).header("User-Agent", &self.user_agent))
    }

    fn parse_search(&self, response: &Response) -> Result<Vec<Place>, SearchError> {
        Self::places(response)
    }

    fn reverse_request(&self, coordinate: &Coordinate, language: &str) -> Result<Request, SearchError> {
        let q = Query::new()
            .pair("lat", format!("{:.6}", coordinate.latitude))
            .pair("lon", format!("{:.6}", coordinate.longitude))
            .pair("lang", Self::language(language))
            .pair("limit", "1");
        Ok(Request::get(q.append_to(&format!("{}/reverse", self.base.trim_end_matches('/')))).header("User-Agent", &self.user_agent))
    }

    fn parse_reverse(&self, response: &Response) -> Result<Option<Place>, SearchError> {
        Ok(Self::places(response)?.into_iter().next())
    }
}
