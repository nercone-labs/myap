use geo::{Bounds, Coordinate};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Why traffic could not be had.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum TrafficError {
    /// No response arrived.
    Network(String),
    /// The service refused the credentials: missing, invalid, expired, or without access to
    /// this service (HTTP 401 and 403).
    Unauthorized(String),
    /// Too many requests (HTTP 429): to be tried again later, not at once.
    RateLimited,
    /// The request was invalid: bounds out of range, or refused by the service as malformed.
    InvalidRequest(String),
    /// The area is larger than the service answers for, or than the number of pieces allowed
    /// ([`crate::TrafficProvider::pieces`]).
    TooLarge,
    /// The service failed: an HTTP error or an error of its own.
    Server(String),
    /// The response could not be read.
    Parse(String),
}

impl fmt::Display for TrafficError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrafficError::Network(s) => write!(f, "network: {s}"),
            TrafficError::Unauthorized(s) => write!(f, "unauthorized: {s}"),
            TrafficError::RateLimited => write!(f, "rate limited"),
            TrafficError::InvalidRequest(s) => write!(f, "invalid request: {s}"),
            TrafficError::TooLarge => write!(f, "area too large"),
            TrafficError::Server(s) => write!(f, "server: {s}"),
            TrafficError::Parse(s) => write!(f, "unreadable response: {s}"),
        }
    }
}

impl std::error::Error for TrafficError {}

impl TrafficError {
    /// The error of a response that did not succeed, from its status alone, or `None` for a
    /// success. Status 0 means that no response arrived.
    pub fn of_response(response: &fetch::Response) -> Option<TrafficError> {
        let status = response.status;
        Some(match status {
            0 => TrafficError::Network(response.error.clone().unwrap_or_default()),
            _ if response.is_success() => return None,
            401 | 403 => TrafficError::Unauthorized(format!("HTTP {status}")),
            429 => TrafficError::RateLimited,
            400..=499 => TrafficError::InvalidRequest(format!("HTTP {status}")),
            _ => TrafficError::Server(format!("HTTP {status}")),
        })
    }

    /// `Ok` when `bounds` are a valid rectangle of [`geo::Bounds`]: finite, within the ranges
    /// of latitude and longitude, `south <= north` and `west <= east`.
    pub fn check_bounds(bounds: &Bounds) -> Result<(), TrafficError> {
        let corners = Coordinate::checked(bounds.south, bounds.west).zip(Coordinate::checked(bounds.north, bounds.east));
        if corners.is_none() || bounds.south > bounds.north || bounds.west > bounds.east {
            return Err(TrafficError::InvalidRequest("bounds out of range or inverted".into()));
        }
        Ok(())
    }
}
