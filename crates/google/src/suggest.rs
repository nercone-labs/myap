use crate::{Google, GoogleError, Positional};
use fetch::{Request, Response};
use geo::{Bounds, Coordinate};
use geocoding::{Place, PlaceSource, Suggestion};
use serde_json::Value;

impl Google {
    /// The suggestions for `query` while it is typed, around `near` (the map's centre).
    /// `counter` numbers the requests of a session, as the page does (`ech`).
    pub fn suggest_request(&self, query: &str, near: &Coordinate, bounds: Option<&Bounds>, counter: u32) -> Request {
        let span = Self::span(bounds, 20_000.0, 2_000.0, 500_000.0);
        let pb = self.fill(
            &self.templates.suggest,
            &[("SPAN", format!("{span:.3}")), ("LNG", format!("{:.7}", near.longitude)), ("LAT", format!("{:.7}", near.latitude))],
        );
        let query_string = Self::query()
            .pair("tbm", "map")
            .pair("gs_ri", "maps")
            .pair("suggest", "p")
            .pair("authuser", "0")
            .pair("hl", self.hl())
            .pair("gl", self.gl())
            .pair("pb", pb)
            .pair("q", query.trim())
            .pair("tch", "1")
            .pair("ech", counter.max(1).to_string());
        self.data_request(query_string.append_to(&format!("{}/s", Self::ORIGIN)))
    }

    /// The suggestions of a response: an object whose `d` holds the guarded JSON, followed by
    /// other text.
    pub fn parse_suggest(&self, response: &Response) -> Result<Vec<Suggestion>, GoogleError> {
        Self::check(response)?;
        let unreadable = || GoogleError::Unreadable(response.body.chars().take(80).collect());
        let object = Positional::first_object(&response.body).ok_or_else(unreadable)?;
        let envelope: Value = serde_json::from_str(object).map_err(|_| unreadable())?;
        let inner = envelope.get("d").and_then(Value::as_str).ok_or_else(unreadable)?;
        if inner.is_empty() {
            return Ok(Vec::new());
        }
        let root = Positional::parse(inner).ok_or_else(unreadable)?;
        let rows = Positional::array(&root, &[0, 1]).map(Vec::as_slice).unwrap_or_default();
        Ok(rows.iter().filter_map(Self::suggestion).collect())
    }

    fn suggestion(row: &Value) -> Option<Suggestion> {
        // The content of a row is the first array whose first element is an array of a string.
        let block = row
            .as_array()?
            .iter()
            .find(|v| v.as_array().and_then(|a| a.first()).and_then(Value::as_array).and_then(|a| a.first()).is_some_and(Value::is_string))?;
        let title = Positional::str(block, &[0, 0])?;
        let primary = Positional::str(block, &[1, 0]).unwrap_or(title).to_string();
        let secondary = Positional::str(block, &[2, 0]).map(str::to_string);
        let coordinate = Self::coordinate(block, &[11]).or_else(|| Self::coordinate(block, &[13, 0, 3]));
        let place = coordinate.map(|coordinate| {
            let feature = Positional::str(block, &[13, 0, 0]).filter(|f| Self::is_feature_id(f));
            let id = feature.map_or_else(|| format!("geo:{:.6},{:.6}", coordinate.latitude, coordinate.longitude), |f| format!("google:{f}"));
            let mut place = Place::new(id, PlaceSource::Google, primary.clone(), coordinate);
            place.address = secondary.clone().filter(|s| *s != primary);
            place
        });
        Some(Suggestion { text: primary, secondary, place })
    }

    fn coordinate(value: &Value, path: &[usize]) -> Option<Coordinate> {
        let node = Positional::at(value, path)?;
        Coordinate::checked(Positional::f64(node, &[2])?, Positional::f64(node, &[3])?)
    }

    /// Whether `text` is a feature id of Google Maps: `0x<hex>:0x<hex>`.
    pub fn is_feature_id(text: &str) -> bool {
        let Some((high, low)) = text.split_once(':') else { return false };
        let hex = |s: &str| s.strip_prefix("0x").is_some_and(|h| !h.is_empty() && h.len() <= 16 && h.bytes().all(|b| b.is_ascii_hexdigit()));
        hex(high) && hex(low)
    }
}
