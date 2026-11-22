//! A client of the web endpoints that Google Maps in a browser uses, without an account or a key.
//!
//! These endpoints are **not a public API**: Google does not document them, may change them at any
//! time, and may refuse requests that do not come from its own web page. Their use may be contrary
//! to Google's terms of service. Every request carries what the user typed or the places of the
//! trip to Google, with the device's IP address. Myap therefore uses them only when the user turns
//! each use on (see the developer notes).
//!
//! The request shapes and the positions of the fields in the responses were learnt from the
//! independent documentation of the Vela project (github.com/PimpinPumpkin/Vela) and checked
//! against recorded responses; no code of Vela is used. The shapes live in [`Templates`], so that a
//! change of Google's is a change of data.
//!
//! - [`Pb`]: the `pb` parameter, a flattened Protocol Buffers message in text.
//! - [`Browser`]: the headers of the desktop browser the endpoints expect.
//! - [`Google`]: suggestions, search, directions; [`Tiles`]: the URLs of the map tiles.

mod browser;
mod client;
mod directions;
mod pb;
mod positional;
mod search;
mod session;
mod suggest;
mod tiles;

pub use browser::Browser;
pub use client::{Google, GoogleError, Templates};
pub use pb::{Pb, PbError, PbNode};
pub use positional::Positional;
pub use session::Session;
pub use tiles::{TileLayer, Tiles};
