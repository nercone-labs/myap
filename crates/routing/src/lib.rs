//! The model of routes that every router shares.
//!
//! A router answers a [`RouteRequest`] with [`Route`]s made of [`Leg`]s (one between each pair of
//! waypoints) made of [`Step`]s, each beginning with a [`Maneuver`]. The routers of Myap (its own
//! offline router, OSRM, Valhalla, Google) each read their own format into this model, so that
//! navigation and the screens never depend on which one answered.
//!
//! Besides its steps, a route may carry what its router knows of it along its geometry: speed
//! limits, congestion and elevations ([`Span`]s and points), and what to warn of ([`Alert`]s); the
//! steps may carry their lanes ([`Lane`]), the name of their junction and the signs.
//!
//! A router reached over HTTP implements [`RemoteRouter`]: it builds the request and reads the
//! response, and its host does the fetching.

mod annotation;
mod lane;
mod maneuver;
mod request;
mod route;

pub use annotation::{Alert, AlertKind, Congestion, Span};
pub use lane::{Lane, LaneIndication};
pub use maneuver::{Maneuver, ManeuverKind, Modifier};
pub use request::{Avoid, DrivingSide, Optimize, RouteRequest, TravelMode};
pub use route::{Leg, Route, RouteError, Step};

/// A router reached over HTTP.
pub trait RemoteRouter {
    /// The HTTP request that asks for routes. An error when the router cannot answer this request
    /// at all (an unsupported mode, too many waypoints).
    fn request(&self, request: &RouteRequest) -> Result<fetch::Request, RouteError>;

    /// The routes of the response, best first.
    fn parse(&self, request: &RouteRequest, response: &fetch::Response) -> Result<Vec<Route>, RouteError>;
}
