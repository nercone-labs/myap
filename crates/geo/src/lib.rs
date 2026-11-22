//! Geometry on a sphere of the mean radius of the Earth, for maps and navigation.
//!
//! - [`Coordinate`]: a point, with distances, bearings and destinations.
//! - [`Bounds`]: a rectangle of latitudes and longitudes.
//! - [`Line`]: a line of points (a road, a route) measured along its length, with the closest
//!   point to a location and the point at a distance.
//! - [`Polygon`]: an area bounded by a ring of points (an area to avoid), with containment and
//!   intersection with lines.
//! - [`Polyline`]: the encoded polyline format of Google, also used by OSRM and Valhalla.
//! - [`Sun`]: where the Sun is in the sky, for telling day from night.
//! - [`Tile`]: the tiles of the Web Mercator projection, as map servers number them.
//!
//! The spherical model is within about 0.5 % of the ellipsoid, finer than the accuracy of the
//! location of a phone over the distances of a route.

mod bounds;
mod coordinate;
mod line;
mod polygon;
mod polyline;
mod sun;
mod tile;

pub use bounds::Bounds;
pub use coordinate::Coordinate;
pub use line::{Line, Projection};
pub use polygon::Polygon;
pub use polyline::{Polyline, PolylineError};
pub use sun::Sun;
pub use tile::Tile;
