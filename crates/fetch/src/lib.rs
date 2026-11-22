//! HTTP requests and responses as plain values.
//!
//! The libraries of Myap never open a connection: they say what to fetch ([`Request`]) and read
//! what came back ([`Response`]). The host carries the request out with the HTTP client of its
//! platform, which brings the system's TLS, proxy and certificate store. This also keeps every
//! library testable with recorded responses.

mod query;
mod request;

pub use query::Query;
pub use request::{Method, Request, Response};
