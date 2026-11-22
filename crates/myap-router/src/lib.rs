//! Myap Router: an offline router on OpenStreetMap data.
//!
//! - [`GraphBuilder`] turns OpenStreetMap elements (from a PBF file or the Overpass API) into a
//!   [`Graph`]: the roads between junctions, with what each mode of travel may use, their speeds,
//!   names, tolls, roundabouts and turn restrictions.
//! - [`Graph`] is written to and read from a compact file (`.myapgraph`), checked whole when read
//!   so that a damaged file is an error, never a crash.
//! - [`Router`] answers a [`routing::RouteRequest`] on a graph with [`routing::Route`]s and their
//!   turn-by-turn steps, and adds what the graph knows (speed limits, alerts, junction names)
//!   to the routes of other routers ([`Router::annotate`]).
//!
//! The search is an A* over the directed edges (the state is the edge arrived by), so turn
//! restrictions, the cost of turning and the avoidance of U-turns are exact. See the developer
//! notes for what was set aside (contraction hierarchies, restrictions via ways).

mod annotate;
mod builder;
mod graph;
mod guidance;
mod index;
mod network;
mod profile;
mod router;
mod search;
mod set;
mod traffic;

pub use annotate::Annotation;
pub use builder::{BuildError, BuildStats, GraphBuilder};
pub use graph::{Edge, Graph, GraphError, NodeFlag, NodeName, NodeNameKind, Restriction, WayInfo};
pub use index::SpatialIndex;
pub use network::{Climb, EdgeClass, Network};
pub use profile::{Highway, Profile};
pub use router::{Router, RouterOptions};
pub use search::{Costs, Path, Search, Snap};
pub use set::EdgeSet;
pub use traffic::{TrafficMap, TrafficSpeed};
