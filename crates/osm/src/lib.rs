//! OpenStreetMap data.
//!
//! [`Element`] (a [`Node`], [`Way`] or [`Relation`] with its [`Tags`]) is the same whatever the
//! data was read from; each format has a reader of its own:
//!
//! - [`Pbf`]: the PBF format of planet files and extracts (<https://wiki.openstreetmap.org/wiki/PBF_Format>).
//! - [`Overpass`]: the JSON output of the Overpass API, and the request for a query.

mod element;
mod overpass;
mod pbf;

pub use element::{Element, Member, MemberKind, Node, Relation, Tags, Way};
pub use overpass::{Overpass, OverpassError};
pub use pbf::{Filter, Pbf, PbfError};
