use crate::{Google, GoogleError, Positional};
use fetch::{Request, Response};
use geo::{Bounds, Coordinate};
use geocoding::{Place, PlaceMatch, PlaceSource};
use serde_json::Value;

impl Google {
    /// The search of `query` in the map around `near`, from result `offset` (20 a page).
    pub fn search_request(&self, query: &str, near: &Coordinate, bounds: Option<&Bounds>, offset: u32) -> Request {
        // `!` would end the string in the `pb`; the page replaces it with a space.
        let text = query.replace('!', " ").trim().to_string();
        let mut pb =
            self.fill(&self.templates.search, &[("QUERY", text.clone()), ("LNG", format!("{:.7}", near.longitude)), ("LAT", format!("{:.7}", near.latitude))]);
        if let Some(b) = bounds {
            let span = Self::span(Some(b), 25_000.0, 1_000.0, 500_000.0);
            if let Some(start) = pb.find("!1d") {
                let end = pb[start + 3..].find('!').map_or(pb.len(), |e| start + 3 + e);
                pb.replace_range(start..end, &format!("!1d{span:.3}"));
            }
        }
        if offset > 0
            && let Some(at) = pb.find("!7i20")
        {
            pb.insert_str(at + "!7i20".len(), &format!("!8i{offset}"));
        }
        let query_string = Self::query().pair("tbm", "map").pair("authuser", "0").pair("hl", self.hl()).pair("gl", self.gl()).pair("q", &text).pair("pb", pb);
        self.data_request(query_string.append_to(&format!("{}/search", Self::ORIGIN)))
    }

    /// The places of a search response, in Google's order.
    pub fn parse_search(&self, response: &Response) -> Result<Vec<Place>, GoogleError> {
        let root = Self::json(response)?;
        if !root.is_array() {
            return Err(GoogleError::Unreadable("not an array".into()));
        }
        if let Some(entries) = Positional::array(&root, &[64]).filter(|e| !e.is_empty()) {
            return Ok(entries.iter().filter_map(|e| Self::place(Positional::at(e, &[1])?)).collect());
        }
        // A reply focused on one place (an address, a single business) has no list.
        Ok(Positional::at(&root, &[0, 1, 0, 14]).and_then(Self::place).into_iter().collect())
    }

    /// The place of a node of a search response.
    pub fn place(node: &Value) -> Option<Place> {
        let name = Positional::str(node, &[11])?;
        let coordinate = Coordinate::checked(Positional::f64(node, &[9, 2])?, Positional::f64(node, &[9, 3])?)?;
        let feature = Positional::str(node, &[10]).filter(|f| Self::is_feature_id(f));
        let id = match feature {
            Some(f) => format!("google:{f}"),
            None => format!("geo:{:.6},{:.6}", coordinate.latitude, coordinate.longitude),
        };
        let mut place = Place::new(id, PlaceSource::Google, name, coordinate);
        place.address = Positional::str(node, &[39]).map(str::to_string).or_else(|| {
            let parts: Vec<&str> = Positional::array(node, &[2])?.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        });
        place.category = Positional::str(node, &[13, 0]).map(str::to_string);
        place.rating = Positional::f64(node, &[4, 7]).filter(|r| (0.0..=5.0).contains(r));
        place.review_count = Positional::at(node, &[4, 8]).and_then(Value::as_u64);
        place.website = Positional::str(node, &[7, 0]).filter(|w| w.starts_with("http")).map(str::to_string);
        place.phone = Positional::str(node, &[178, 0, 0]).map(str::to_string);
        place.status = Positional::str(node, &[203, 1, 4, 0]).map(str::to_string);
        place.hours = Positional::array(node, &[203, 0])
            .map(|days| {
                days.iter()
                    .filter_map(|day| {
                        let name = Positional::str(day, &[0])?;
                        let ranges: Vec<&str> = Positional::array(day, &[3])?.iter().filter_map(|r| Positional::str(r, &[0])).collect();
                        Some(format!("{name}: {}", ranges.join(", ")))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(place)
    }

    /// The details of `place` (hours, phone, rating), by searching its name and address at its
    /// position and keeping only the result of the same feature id (or, for a place of another
    /// source, the same place by [`PlaceMatch`]). Google offers no lookup by id to a visitor
    /// without a key.
    pub fn details_request(&self, place: &Place) -> Request {
        let address = place.address.as_deref().unwrap_or("").replace(',', " ");
        let query = format!("{} {}", place.name, address);
        let tight = place.coordinate;
        let bounds = Bounds { south: tight.latitude - 0.01, west: tight.longitude - 0.01, north: tight.latitude + 0.01, east: tight.longitude + 0.01 };
        self.search_request(query.trim(), &place.coordinate, Some(&bounds), 0)
    }

    /// The details of `place` from the response of [`details_request`](Self::details_request):
    /// the result of its feature id (or linked to it); for a place of another source without such
    /// a link, the result `matcher` finds surely the same place. `None` when there is none.
    pub fn parse_details(&self, place: &Place, response: &Response, matcher: &PlaceMatch) -> Result<Option<Place>, GoogleError> {
        let results = self.parse_search(response)?;
        let ours = |p: &Place| p.id == place.id || place.links.contains(&p.id);
        if let Some(i) = results.iter().position(ours) {
            return Ok(results.into_iter().nth(i));
        }
        if place.source == PlaceSource::Google || place.links.iter().any(|l| l.starts_with("google:")) {
            return Ok(None);
        }
        Ok(matcher.best(place, &results).cloned())
    }
}
