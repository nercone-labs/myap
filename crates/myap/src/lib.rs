//! The core of Myap: nearly everything the application does, for every platform.
//!
//! [`Core`] is a state machine without network access: the platform hands it [`Input`]s (what
//! the user did, the device's location, the responses of the fetches it asked for) and carries
//! out the [`Effect`]s of each [`Response`] (fetch a URL, say a text, build a graph, wake it up),
//! then draws the [`Snapshot`]. The protocol is JSON (`protocol.rs`), so a platform's host is a
//! few hundred lines.
//!
//! - Search: Photon and Nominatim (OpenStreetMap data), and Google when allowed.
//! - Routes: Myap Router on the device, OSRM, Valhalla and Google, tried in the order of the
//!   settings, each falling back to the next.
//! - Navigation: progress along the route, announcements, leaving the route and a new route.
//! - Regions: OpenStreetMap data downloaded and built into graphs for Myap Router.
//! - Google: every use off until turned on, in one place (`GoogleSettings`), with a record on the
//!   device of what was sent.

mod core;
mod graphs;
mod navigation;
mod protocol;
mod regions;
mod settings;
mod store;
mod style;
pub mod words;

pub use crate::core::Core;
pub use graphs::GraphBuild;
pub use navigation::{Event, LocalPoint, Location, NavigationState, Session};
pub use protocol::*;
pub use regions::{Region, RegionSource, RegionStatus, Regions};
pub use settings::{
    AvoidArea, DrivingSideSetting, ElevationSource, GoogleSettings, GoogleUse, GuidanceSettings, Language, MapSource, NightMode, PlaceSettings, RouterKind,
    RoutingSettings, Servers, Settings, TrafficSettings, Units,
};
pub use store::{GoogleLog, GoogleRecord, Places, SavedLabel, SavedPlace, Store};
pub use style::{FeatureKind, MapStyle, RasterOverlay, Style};
