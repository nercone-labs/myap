//! Live traffic as values: how fast the roads are moving ([`FlowSegment`]) and what happened on
//! them ([`Incident`]), gathered in [`Traffic`].
//!
//! This crate performs no network access. A traffic service is a [`TrafficProvider`]: it builds
//! the [`fetch::Request`] for an area and reads the [`fetch::Response`] that the host fetched into
//! this model, so that the map and the router never depend on which service answered. The
//! client of HERE Traffic API v7 ([`here::Here`]) is one provider.
//!
//! A service answers for an area of limited size ([`AreaLimit`]); [`TrafficProvider::pieces`]
//! splits a larger area into pieces that it accepts, and [`Traffic::merge`] joins their answers.
//!
//! The values of a service are its estimates. They can be late or wrong, and a road the model
//! shows as open can be closed: they are an aid for choosing a route and for its time of
//! arrival, never a substitute for the signs and the state of the road.

mod error;
mod flow;
pub mod here;
mod incident;
mod time;
mod traffic;

pub use error::TrafficError;
pub use flow::{Congestion, FlowSegment};
pub use incident::{Criticality, Incident, IncidentKind};
pub use time::Rfc3339;
pub use traffic::Traffic;

use geo::Bounds;

/// The largest area that a provider answers for in one request. A limit left `None` does not
/// apply; a limit that is not a positive finite number is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AreaLimit {
    /// The largest difference between the east and the west edges, in degrees of longitude.
    pub max_width_deg: Option<f64>,
    /// The largest difference between the north and the south edges, in degrees of latitude.
    pub max_height_deg: Option<f64>,
    /// The largest area, in square kilometres ([`Bounds::area_km2`]).
    pub max_area_km2: Option<f64>,
}

impl AreaLimit {
    /// Whether `bounds` are within every limit.
    pub fn admits(&self, bounds: &Bounds) -> bool {
        let within = |limit: Option<f64>, value: f64| Self::usable(limit).is_none_or(|limit| value <= limit);
        within(self.max_width_deg, bounds.east - bounds.west)
            && within(self.max_height_deg, bounds.north - bounds.south)
            && within(self.max_area_km2, bounds.area_km2())
    }

    /// `limit` when it is a positive finite number.
    fn usable(limit: Option<f64>) -> Option<f64> {
        limit.filter(|l| l.is_finite() && *l > 0.0)
    }
}

/// A traffic service reached over HTTP: it says what to fetch for an area, and reads what came
/// back into the model of this crate.
pub trait TrafficProvider {
    /// The request for the flow of traffic within `bounds`. An error when the provider cannot
    /// answer for these bounds at all (invalid, or beyond [`TrafficProvider::max_area`]: see
    /// [`TrafficProvider::pieces`]).
    fn flow_request(&self, bounds: &Bounds) -> Result<fetch::Request, TrafficError>;

    /// The flow of a response to [`TrafficProvider::flow_request`], in [`Traffic::flow`] (and
    /// the time of the data, when the provider gives it).
    fn parse_flow(&self, response: &fetch::Response) -> Result<Traffic, TrafficError>;

    /// The request for the incidents within `bounds`, with the same errors as
    /// [`TrafficProvider::flow_request`].
    fn incidents_request(&self, bounds: &Bounds) -> Result<fetch::Request, TrafficError>;

    /// The incidents of a response to [`TrafficProvider::incidents_request`], in
    /// [`Traffic::incidents`].
    fn parse_incidents(&self, response: &fetch::Response) -> Result<Traffic, TrafficError>;

    /// The largest area that one request may cover.
    fn max_area(&self) -> AreaLimit;

    /// `bounds` split into a grid of equal pieces, each within [`TrafficProvider::max_area`],
    /// west to east within each row and rows south to north. The pieces cover `bounds` exactly
    /// and share their edges.
    ///
    /// [`TrafficError::TooLarge`] when more than `max_pieces` pieces would be needed (the caller
    /// decides how many requests an area is worth), and [`TrafficError::InvalidRequest`] when
    /// `bounds` are not a valid rectangle ([`geo::Bounds`]: within the ranges of latitude and
    /// longitude, `south <= north` and `west <= east`).
    fn pieces(&self, bounds: &Bounds, max_pieces: usize) -> Result<Vec<Bounds>, TrafficError> {
        TrafficError::check_bounds(bounds)?;
        let limit = self.max_area();
        let (width, height) = (bounds.east - bounds.west, bounds.north - bounds.south);
        // The count of pieces along one side, as a float first: an enormous ratio must not wrap.
        let count = |side: f64, limit: Option<f64>| -> Result<usize, TrafficError> {
            let Some(limit) = AreaLimit::usable(limit) else { return Ok(1) };
            let n = (side / limit).ceil().max(1.0);
            if n > max_pieces as f64 { Err(TrafficError::TooLarge) } else { Ok(n as usize) }
        };
        let (mut columns, mut rows) = (count(width, limit.max_width_deg)?, count(height, limit.max_height_deg)?);
        let grid = |columns: usize, rows: usize| -> Vec<Bounds> {
            let mut pieces = Vec::with_capacity(columns * rows);
            for row in 0..rows {
                let south = bounds.south + height * row as f64 / rows as f64;
                let north = if row + 1 == rows { bounds.north } else { bounds.south + height * (row + 1) as f64 / rows as f64 };
                for column in 0..columns {
                    let west = bounds.west + width * column as f64 / columns as f64;
                    let east = if column + 1 == columns { bounds.east } else { bounds.west + width * (column + 1) as f64 / columns as f64 };
                    pieces.push(Bounds { south, west, north, east });
                }
            }
            pieces
        };
        loop {
            if columns.checked_mul(rows).is_none_or(|n| n > max_pieces) {
                return Err(TrafficError::TooLarge);
            }
            let pieces = grid(columns, rows);
            let Some(largest) = pieces.iter().max_by(|a, b| a.area_km2().total_cmp(&b.area_km2())).copied() else {
                return Err(TrafficError::TooLarge);
            };
            // The counts above meet the limits of width and height (up to the rounding of the
            // division); only the area, which depends on the latitude, is left to check.
            let area_ok = AreaLimit::usable(limit.max_area_km2).is_none_or(|max| largest.area_km2() <= max);
            if area_ok {
                return Ok(pieces);
            }
            // Over the limit of area: halve the longer side of the largest piece, in kilometres.
            let middle = largest.center().latitude;
            let across = geo::Coordinate::new(middle, largest.west).distance_m(&geo::Coordinate::new(middle, largest.east));
            let along = geo::Coordinate::new(largest.south, largest.west).distance_m(&geo::Coordinate::new(largest.north, largest.west));
            if across >= along {
                columns = columns.saturating_mul(2);
            } else {
                rows = rows.saturating_mul(2);
            }
        }
    }
}
