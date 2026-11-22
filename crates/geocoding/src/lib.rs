//! Places and their search.
//!
//! [`Place`] is the same whoever found it. Each geocoder implements [`Geocoder`]: it builds the
//! HTTP request of a [`SearchRequest`] or a reverse lookup and reads the response; its host does
//! the fetching.
//!
//! - [`Photon`]: search as you type over OpenStreetMap data (<https://photon.komoot.io>).
//! - [`Nominatim`]: the geocoder of OpenStreetMap (<https://nominatim.org>), here for reverse
//!   lookups and submitted searches. Its public server allows one request a second and no
//!   search as you type (<https://operations.osmfoundation.org/policies/nominatim/>). Its
//!   `extratags` give the opening hours, phone, website and wheelchair access of a place.
//! - [`PlaceMatch`]: whether two places of two sources are the same place, and
//!   [`Place::fill_from`] to fill what one lacks with what the other knows, saying where each
//!   field came from.

mod address;
mod matching;
mod nominatim;
mod photon;
mod place;

pub use address::Address;
pub use matching::{Filled, PlaceField, PlaceMatch, Wheelchair};
pub use nominatim::Nominatim;
pub use photon::Photon;
pub use place::{Place, PlaceSource, SearchError, SearchRequest, Suggestion};

/// A geocoder reached over HTTP.
pub trait Geocoder {
    fn search_request(&self, request: &SearchRequest) -> Result<fetch::Request, SearchError>;
    fn parse_search(&self, response: &fetch::Response) -> Result<Vec<Place>, SearchError>;
    fn reverse_request(&self, coordinate: &geo::Coordinate, language: &str) -> Result<fetch::Request, SearchError>;
    /// The place at the coordinate, or `None` when there is nothing there.
    fn parse_reverse(&self, response: &fetch::Response) -> Result<Option<Place>, SearchError>;
}
