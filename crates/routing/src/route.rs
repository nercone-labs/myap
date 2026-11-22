use crate::{Alert, Avoid, Congestion, Lane, Maneuver, Span};
use geo::{Bounds, Coordinate, Line};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A step: a maneuver, then the way to the next one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub maneuver: Maneuver,
    /// The length of the step and the time to travel it.
    pub distance_m: f64,
    pub duration_s: f64,
    /// The indices of the first and last points of the step in the geometry of its route.
    pub geometry_start: usize,
    pub geometry_end: usize,
    /// The name and the number (`ref`) of the road taken, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub road_ref: Option<String>,
    /// The destinations on the signs (方面), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destinations: Option<String>,
    /// The text of the router for this step, when it gives one. Myap words its own instructions
    /// and uses this only for maneuvers it cannot word (public transport).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    /// The line, for a public transport step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transit: Option<String>,
    /// The lanes of the road before the maneuver, from the left one, when known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
    /// The name of the junction of the maneuver (交差点名), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub junction: Option<String>,
    /// The number of the exit or junction (a motorway exit), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_number: Option<String>,
}

impl Step {
    /// A step of `maneuver` over `geometry_start..=geometry_end`, with nothing else known.
    pub fn new(maneuver: Maneuver, distance_m: f64, duration_s: f64, geometry_start: usize, geometry_end: usize) -> Step {
        Step {
            maneuver,
            distance_m,
            duration_s,
            geometry_start,
            geometry_end,
            name: None,
            road_ref: None,
            destinations: None,
            instruction: None,
            transit: None,
            lanes: Vec::new(),
            junction: None,
            exit_number: None,
        }
    }
}

/// The part of a route between two waypoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leg {
    pub distance_m: f64,
    pub duration_s: f64,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub distance_m: f64,
    /// The time without traffic, and with it when the router knows it.
    pub duration_s: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_in_traffic_s: Option<f64>,
    pub geometry: Vec<Coordinate>,
    pub legs: Vec<Leg>,
    /// The main roads, for telling routes apart ("国道1号, 首都高速"), when known.
    #[serde(default)]
    pub summary: String,
    /// Whether the route uses toll roads or ferries, when known.
    #[serde(default)]
    pub has_tolls: bool,
    #[serde(default)]
    pub has_ferries: bool,
    /// What was asked to be avoided but the router could not avoid.
    #[serde(default)]
    pub ignored_avoid: Avoid,
    /// Whether the route passes through an area that was to be avoided.
    #[serde(default)]
    pub enters_avoided_area: bool,
    /// The speed limits along the geometry, in km/h, where known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speed_limits: Vec<Span<u16>>,
    /// The congestion along the geometry, where the router knows the traffic.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub congestion: Vec<Span<Congestion>>,
    /// The elevation of each point of the geometry in metres, or empty when unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elevations: Vec<f32>,
    /// What to warn of along the route, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<Alert>,
}

impl Route {
    /// The time to use: with traffic when known.
    pub fn expected_duration_s(&self) -> f64 {
        self.duration_in_traffic_s.unwrap_or(self.duration_s)
    }

    pub fn line(&self) -> Line {
        Line::new(self.geometry.clone())
    }

    pub fn bounds(&self) -> Option<Bounds> {
        Bounds::of(&self.geometry)
    }

    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.legs.iter().flat_map(|leg| leg.steps.iter())
    }

    /// A route of `geometry` and `legs`, with nothing else known.
    pub fn new(geometry: Vec<Coordinate>, legs: Vec<Leg>, distance_m: f64, duration_s: f64) -> Route {
        Route {
            distance_m,
            duration_s,
            duration_in_traffic_s: None,
            geometry,
            legs,
            summary: String::new(),
            has_tolls: false,
            has_ferries: false,
            ignored_avoid: Avoid::default(),
            enters_avoided_area: false,
            speed_limits: Vec::new(),
            congestion: Vec::new(),
            elevations: Vec::new(),
            alerts: Vec::new(),
        }
    }

    /// The total climb and descent along the elevations, in metres, counting only changes of at
    /// least `threshold_m` from the last turning point, so that the noise of elevation data does
    /// not add up. `None` without elevations.
    pub fn climb_m(&self, threshold_m: f64) -> Option<(f64, f64)> {
        let mut values = self.elevations.iter().map(|e| *e as f64);
        let mut anchor = values.next()?;
        let (mut up, mut down) = (0.0, 0.0);
        for e in values {
            let d = e - anchor;
            if d >= threshold_m {
                up += d;
                anchor = e;
            } else if d <= -threshold_m {
                down -= d;
                anchor = e;
            }
        }
        Some((up, down))
    }

    /// The speed limit at point `index` of the geometry, in km/h, when known.
    pub fn speed_limit_at(&self, index: usize) -> Option<u16> {
        Span::at(&self.speed_limits, index)
    }

    /// Checks what navigation relies on: a geometry of two or more valid points, and steps whose
    /// ranges lie within it, in order.
    pub fn validate(&self) -> Result<(), RouteError> {
        if self.geometry.len() < 2 || !self.geometry.iter().all(Coordinate::is_valid) {
            return Err(RouteError::Parse("route without a valid geometry".into()));
        }
        // Navigation needs a step at least in every leg (the arrival).
        if self.legs.is_empty() || self.legs.iter().any(|leg| leg.steps.is_empty()) {
            return Err(RouteError::Parse("route without steps".into()));
        }
        let mut last = 0;
        for step in self.steps() {
            if step.geometry_start > step.geometry_end || step.geometry_end >= self.geometry.len() || step.geometry_start < last {
                return Err(RouteError::Parse("step outside the geometry of its route".into()));
            }
            last = step.geometry_start;
        }
        if !self.distance_m.is_finite() || !self.duration_s.is_finite() || self.distance_m < 0.0 || self.duration_s < 0.0 {
            return Err(RouteError::Parse("route of invalid length or duration".into()));
        }
        if self.duration_in_traffic_s.is_some_and(|t| !t.is_finite() || t < 0.0) {
            return Err(RouteError::Parse("route of invalid duration in traffic".into()));
        }
        let n = self.geometry.len();
        if !self.elevations.is_empty() && (self.elevations.len() != n || !self.elevations.iter().all(|e| e.is_finite())) {
            return Err(RouteError::Parse("elevations do not match the geometry".into()));
        }
        fn spans_ok<T>(spans: &[Span<T>], n: usize) -> bool {
            spans.iter().all(|s| s.start <= s.end && s.end < n) && spans.windows(2).all(|w| w[0].end <= w[1].start)
        }
        if !spans_ok(&self.speed_limits, n) || !spans_ok(&self.congestion, n) {
            return Err(RouteError::Parse("spans outside the geometry of the route".into()));
        }
        if self.alerts.iter().any(|a| a.index >= n || !a.location.is_valid()) || self.alerts.windows(2).any(|w| w[0].index > w[1].index) {
            return Err(RouteError::Parse("alerts outside the geometry of the route".into()));
        }
        for step in self.steps() {
            if step.lanes.len() > 32 {
                return Err(RouteError::Parse("too many lanes".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum RouteError {
    /// The waypoints cannot be connected.
    NoRoute,
    /// A waypoint is too far from any road the router knows.
    NoRoad,
    /// The router has no data for this area.
    OutOfCoverage,
    /// The router does not support this mode of travel.
    UnsupportedMode,
    /// Fewer than two waypoints, or more than the router accepts.
    InvalidRequest(String),
    /// No response arrived.
    Network(String),
    /// The server refused: an HTTP error, a rate limit, an error of its own.
    Server(String),
    /// The response could not be read.
    Parse(String),
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteError::NoRoute => write!(f, "no route"),
            RouteError::NoRoad => write!(f, "no road near a waypoint"),
            RouteError::OutOfCoverage => write!(f, "out of coverage"),
            RouteError::UnsupportedMode => write!(f, "unsupported mode of travel"),
            RouteError::InvalidRequest(s) => write!(f, "invalid request: {s}"),
            RouteError::Network(s) => write!(f, "network: {s}"),
            RouteError::Server(s) => write!(f, "server: {s}"),
            RouteError::Parse(s) => write!(f, "unreadable response: {s}"),
        }
    }
}

impl std::error::Error for RouteError {}

impl RouteError {
    /// The error of a response that did not succeed, or `None` for a success.
    pub fn of_response(response: &fetch::Response) -> Option<RouteError> {
        if response.status == 0 {
            return Some(RouteError::Network(response.error.clone().unwrap_or_default()));
        }
        (!response.is_success()).then(|| RouteError::Server(format!("HTTP {}", response.status)))
    }
}

impl Route {
    /// A route of `geometry` alone, with steps found from its shape: a turn wherever its direction
    /// changes by `min_turn_degrees` or more within a short distance. For routers that give a
    /// line but no usable steps; the steps have no road names, and a sharp bend of a road counts
    /// as a turn.
    pub fn from_geometry(geometry: Vec<Coordinate>, distance_m: Option<f64>, duration_s: f64, min_turn_degrees: f64) -> Route {
        use crate::{ManeuverKind, Modifier};
        let line = Line::new(geometry.clone());
        let length = line.length_m().max(1e-9);
        let distance_m = distance_m.unwrap_or(length);
        // The bearing over the 20 m before and after each point, so that the small zigzags of a
        // drawn line are not turns.
        const WINDOW_M: f64 = 20.0;
        let mut turns: Vec<(usize, f64)> = Vec::new();
        let inner = geometry.len().saturating_sub(2);
        for (i, point) in geometry.iter().enumerate().skip(1).take(inner) {
            let at = line.along_m(i);
            let (Some(before), Some(after)) = (line.point_at(at - WINDOW_M), line.point_at(at + WINDOW_M)) else { continue };
            if before.distance_m(point) < 1.0 || after.distance_m(point) < 1.0 {
                continue;
            }
            let angle = Coordinate::bearing_difference(before.bearing_to(point), point.bearing_to(&after));
            if angle.abs() >= min_turn_degrees {
                // One turn per bend: a stronger point within the window replaces a weaker one.
                match turns.last_mut() {
                    Some((last, last_angle)) if at - line.along_m(*last) < WINDOW_M => {
                        if angle.abs() > last_angle.abs() {
                            *last = i;
                            *last_angle = angle;
                        }
                    }
                    _ => turns.push((i, angle)),
                }
            }
        }
        let mut starts: Vec<(usize, ManeuverKind, Option<Modifier>)> = vec![(0, ManeuverKind::Depart, None)];
        starts.extend(turns.iter().map(|&(i, a)| (i, ManeuverKind::Turn, Some(Modifier::from_angle(a)))));
        let last = geometry.len().saturating_sub(1);
        let mut steps = Vec::with_capacity(starts.len() + 1);
        for (n, &(index, kind, modifier)) in starts.iter().enumerate() {
            let end = starts.get(n + 1).map_or(last, |s| s.0);
            let share = (line.along_m(end) - line.along_m(index)) / length;
            let mut maneuver = Maneuver::new(kind, modifier, geometry[index]);
            maneuver.bearing_after = line.bearing_at(line.along_m(index));
            steps.push(Step::new(maneuver, distance_m * share, duration_s * share, index, end));
        }
        if let Some(end) = geometry.last() {
            steps.push(Step::new(Maneuver::new(ManeuverKind::Arrive, None, *end), 0.0, 0.0, last, last));
        }
        Route::new(geometry, vec![Leg { distance_m, duration_s, steps }], distance_m, duration_s)
    }
}
