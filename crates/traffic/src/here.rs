//! A client of HERE Traffic API v7: real-time flow (`GET /v7/flow`) and incidents
//! (`GET /v7/incidents`) within a bounding box, located by shape points
//! (<https://docs.here.com/traffic-api/docs/introduction-to-here-traffic-api-v7>).
//!
//! The API key is sent in the `apiKey` query parameter, the one place that the API reads it
//! from besides an OAuth token, and nowhere else. It is removed from every message of a
//! [`TrafficError`] that this client makes, since a host may put the URL in its network errors.
//!
//! The terms of HERE limit how long its results may be stored or cached outside its platform
//! (30 days in the HERE Platform Terms of 2020, 24 hours in Japan): the host is to keep traffic
//! for minutes, as live data, and check the terms of its own agreement.

use crate::{AreaLimit, Criticality, FlowSegment, Incident, IncidentKind, Rfc3339, Traffic, TrafficError, TrafficProvider};
use fetch::{Query, Request, Response};
use geo::{Bounds, Coordinate, Line};
use serde::Deserialize;
use std::fmt;

/// The traffic service of HERE. Its `Debug` shows the API key as `[apiKey]`.
#[derive(Clone, PartialEq)]
pub struct Here {
    /// The API key of a HERE project. It is a secret: never shown, logged or sent elsewhere.
    pub api_key: String,
    /// The base URL, before `/v7`.
    pub base: String,
    pub user_agent: String,
    /// The functional classes of the roads to ask flow and incidents for, from 1 (motorways) to 5
    /// (local roads); empty for all of them.
    pub functional_classes: Vec<u8>,
    /// The language of the texts of incidents (BCP 47, `ja-JP`); `None` for the language of the
    /// country of each incident.
    pub language: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowBody {
    #[serde(default)]
    source_updated: Option<String>,
    results: Vec<FlowItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowItem {
    #[serde(default)]
    location: Option<Location>,
    current_flow: RawFlow,
}

/// The current flow of a location, or one of its sub-segments (which have a `length` and no
/// sub-segments).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawFlow {
    #[serde(default)]
    length: Option<f64>,
    /// In metres per second, capped by the speed limit.
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    speed_uncapped: Option<f64>,
    #[serde(default)]
    free_flow: Option<f64>,
    #[serde(default)]
    jam_factor: Option<f64>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    traversability: Option<String>,
    #[serde(default)]
    sub_segments: Vec<RawFlow>,
}

#[derive(Deserialize)]
struct Location {
    #[serde(default)]
    shape: Option<Shape>,
}

#[derive(Deserialize)]
struct Shape {
    #[serde(default)]
    links: Vec<Link>,
}

#[derive(Deserialize)]
struct Link {
    #[serde(default)]
    points: Vec<Point>,
    #[serde(default)]
    length: Option<f64>,
}

#[derive(Deserialize)]
struct Point {
    lat: f64,
    lng: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncidentsBody {
    #[serde(default)]
    source_updated: Option<String>,
    results: Vec<IncidentItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncidentItem {
    #[serde(default)]
    location: Option<Location>,
    #[serde(default)]
    incident_details: Option<Details>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Details {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    original_id: Option<String>,
    #[serde(default)]
    start_time: Option<String>,
    #[serde(default)]
    end_time: Option<String>,
    #[serde(default)]
    road_closed: bool,
    #[serde(default)]
    criticality: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    description: Option<Text>,
    #[serde(default)]
    summary: Option<Text>,
}

#[derive(Deserialize)]
struct Text {
    #[serde(default)]
    value: String,
}

/// The two shapes of an error of the API: of authentication (`error`, `error_description`), and
/// of the service (`title`, `status`, `code`, `cause`, `action`, `correlationId`).
#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    cause: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

impl Here {
    /// The base URL of the service (its servers are `https://data.traffic.hereapi.com/v7`).
    pub const DEFAULT_BASE: &'static str = "https://data.traffic.hereapi.com";
    /// The largest width and height of a bounding box that the API accepts, in degrees.
    pub const MAX_SPAN_DEG: f64 = 1.0;
    /// The decimals of the coordinates in a request (about a metre).
    pub const DECIMALS: i32 = 5;

    pub fn new(api_key: impl Into<String>, user_agent: impl Into<String>) -> Here {
        Here { api_key: api_key.into(), base: Self::DEFAULT_BASE.into(), user_agent: user_agent.into(), functional_classes: Vec::new(), language: None }
    }

    /// The value of the parameter `in` for `bounds`: `bbox:{west},{south},{east},{north}`, each
    /// rounded to [`Here::DECIMALS`] decimals. [`TrafficError::TooLarge`] when the rounded box is
    /// wider or taller than [`Here::MAX_SPAN_DEG`].
    pub fn bbox(bounds: &Bounds) -> Result<String, TrafficError> {
        TrafficError::check_bounds(bounds)?;
        let [south, west, north, east] = [bounds.south, bounds.west, bounds.north, bounds.east].map(Self::round);
        // A margin for the subtraction of rounded values, far below the precision of a request.
        const EPSILON: f64 = 1e-9;
        if east - west > Self::MAX_SPAN_DEG + EPSILON || north - south > Self::MAX_SPAN_DEG + EPSILON {
            return Err(TrafficError::TooLarge);
        }
        let d = Self::DECIMALS as usize;
        Ok(format!("bbox:{west:.d$},{south:.d$},{east:.d$},{north:.d$}"))
    }

    /// `value` rounded to [`Here::DECIMALS`] decimals, without a negative zero.
    fn round(value: f64) -> f64 {
        let scale = 10f64.powi(Self::DECIMALS);
        let rounded = (value * scale).round() / scale;
        if rounded == 0.0 { 0.0 } else { rounded }
    }

    /// `text` with the API key (as is and percent-encoded) replaced by `[apiKey]`.
    pub fn redact(&self, text: &str) -> String {
        if self.api_key.is_empty() {
            return text.to_string();
        }
        let encoded = Query::encode(&self.api_key);
        text.replace(&self.api_key, "[apiKey]").replace(&encoded, "[apiKey]")
    }

    fn request(&self, endpoint: &str, bounds: &Bounds, incidents: bool) -> Result<Request, TrafficError> {
        if self.api_key.is_empty() {
            return Err(TrafficError::Unauthorized("no API key".into()));
        }
        if self.functional_classes.iter().any(|c| !(1..=5).contains(c)) {
            return Err(TrafficError::InvalidRequest("functional classes are 1 to 5".into()));
        }
        let classes = (!self.functional_classes.is_empty()).then(|| self.functional_classes.iter().map(u8::to_string).collect::<Vec<_>>().join(","));
        let query = Query::new()
            .pair("in", Self::bbox(bounds)?)
            .pair("locationReferencing", "shape")
            .optional("functionalClasses", classes)
            .optional("lang", if incidents { self.language.as_deref() } else { None })
            .pair("apiKey", &self.api_key);
        let url = query.append_to(&format!("{}/v7/{endpoint}", self.base.trim_end_matches('/')));
        Ok(Request::get(url).header("User-Agent", &self.user_agent))
    }

    /// The error of a response that did not succeed, read from its body when it has one.
    fn error(&self, response: &Response) -> Option<TrafficError> {
        let error = TrafficError::of_response(response)?;
        let Ok(body) = serde_json::from_str::<ErrorBody>(&response.body) else {
            return Some(match error {
                TrafficError::Network(s) => TrafficError::Network(self.redact(&s)),
                other => other,
            });
        };
        let parts = [body.title.or(body.error), body.cause.or(body.error_description), body.code.clone()];
        let message = self.redact(&parts.into_iter().flatten().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(": "));
        let message = if message.is_empty() { format!("HTTP {}", response.status) } else { message };
        // E608006: "Invalid BBox size"; E608017: "Circle/Corridor radius is too big".
        let too_large = matches!(body.code.as_deref(), Some("E608006" | "E608017"));
        Some(match error {
            TrafficError::Unauthorized(_) => TrafficError::Unauthorized(message),
            TrafficError::InvalidRequest(_) if too_large => TrafficError::TooLarge,
            TrafficError::InvalidRequest(_) => TrafficError::InvalidRequest(message),
            TrafficError::Server(_) => TrafficError::Server(message),
            other => other,
        })
    }

    /// The valid points of `link`, and its length (its own, or measured along its points).
    fn link(link: &Link) -> (Vec<Coordinate>, f64) {
        let points: Vec<Coordinate> = link.points.iter().filter_map(|p| Coordinate::checked(p.lat, p.lng)).collect();
        let length = link.length.filter(|l| l.is_finite() && *l >= 0.0).unwrap_or_else(|| Line::new(points.clone()).length_m());
        (points, length)
    }

    /// The segments of one location: one per link, with the flow of the sub-segment at the
    /// middle of the link when the flow has sub-segments. Links that join exactly (the last
    /// point of one is the first of the next) and carry the same flow are drawn as one segment,
    /// so that the geometry is kept exactly and no line crosses a gap between links.
    fn segments(item: FlowItem, out: &mut Vec<FlowSegment>) {
        let Some(shape) = item.location.and_then(|l| l.shape) else { return };
        let flow = item.current_flow;
        let links: Vec<(Vec<Coordinate>, f64)> = shape.links.iter().map(Self::link).collect();
        let total: f64 = links.iter().map(|(_, l)| l).sum();
        let lengths: Vec<f64> = flow.sub_segments.iter().map(|s| s.length.filter(|l| l.is_finite() && *l > 0.0).unwrap_or(0.0)).collect();
        let sub_total: f64 = lengths.iter().sum();
        let start = out.len();
        let mut along = 0.0;
        let mut last_source: Option<Option<usize>> = None;
        for (points, length) in links {
            // The sub-segments sum up to the location; a link is placed on them by the share of
            // the length of the links before its middle.
            let source = (total > 0.0 && sub_total > 0.0).then(|| {
                let at = (along + length / 2.0) / total * sub_total;
                let mut end = 0.0;
                lengths
                    .iter()
                    .position(|l| {
                        end += l;
                        at < end
                    })
                    .unwrap_or(lengths.len().saturating_sub(1))
            });
            along += length;
            if points.len() < 2 {
                last_source = None;
                continue;
            }
            let joins = out.len() > start && last_source == Some(source) && out.last().and_then(|p| p.shape.last()) == points.first();
            if let Some(previous) = out.last_mut().filter(|_| joins) {
                previous.shape.extend(points.into_iter().skip(1));
                previous.length_m = Some(previous.length_m.unwrap_or(0.0) + length);
                continue;
            }
            let values = source.and_then(|i| flow.sub_segments.get(i));
            out.push(Self::segment(points, length, values, &flow));
            last_source = Some(source);
        }
    }

    /// A segment of `points` with the values of `sub` where it has them, else of `flow`.
    fn segment(points: Vec<Coordinate>, length: f64, sub: Option<&RawFlow>, flow: &RawFlow) -> FlowSegment {
        let pick = |f: fn(&RawFlow) -> Option<f64>| sub.and_then(f).or_else(|| f(flow)).filter(|v| v.is_finite() && *v >= 0.0);
        let speed = pick(|f| f.speed).or_else(|| pick(|f| f.speed_uncapped));
        let jam_factor = pick(|f| f.jam_factor).map(|j| j.min(10.0));
        let traversability = sub.and_then(|s| s.traversability.as_deref()).or(flow.traversability.as_deref());
        // `reversibleNotRoutable` is to be treated as not routable, as HERE recommends.
        let closed = matches!(traversability, Some("closed" | "reversibleNotRoutable")) || jam_factor.is_some_and(|j| j >= 10.0);
        FlowSegment {
            shape: points,
            length_m: Some(length),
            speed_kmh: speed.map(|s| s * 3.6),
            free_flow_kmh: pick(|f| f.free_flow).map(|s| s * 3.6),
            jam_factor,
            confidence: pick(|f| f.confidence).map(|c| c.min(1.0)),
            closed,
        }
    }

    /// The links of a location as lines: links that join exactly are one line.
    fn lines(location: Option<Location>) -> Vec<Vec<Coordinate>> {
        let mut lines: Vec<Vec<Coordinate>> = Vec::new();
        let links = location.and_then(|l| l.shape).map(|s| s.links).unwrap_or_default();
        for link in &links {
            let (points, _) = Self::link(link);
            if points.len() < 2 {
                continue;
            }
            match lines.last_mut() {
                Some(line) if line.last() == points.first() => line.extend(points.into_iter().skip(1)),
                _ => lines.push(points),
            }
        }
        lines
    }

    fn kind(kind: &str) -> IncidentKind {
        match kind {
            "accident" => IncidentKind::Accident,
            "construction" => IncidentKind::Construction,
            "congestion" => IncidentKind::Congestion,
            "disabledVehicle" => IncidentKind::DisabledVehicle,
            "massTransit" => IncidentKind::MassTransit,
            "plannedEvent" => IncidentKind::PlannedEvent,
            "roadHazard" => IncidentKind::RoadHazard,
            "roadClosure" => IncidentKind::RoadClosure,
            "weather" => IncidentKind::Weather,
            "laneRestriction" => IncidentKind::LaneRestriction,
            // "other", and the types added after this client (the list is open).
            _ => IncidentKind::Other,
        }
    }

    fn criticality(criticality: &str) -> Criticality {
        match criticality {
            "critical" => Criticality::Critical,
            "major" => Criticality::Major,
            "minor" => Criticality::Minor,
            "low" => Criticality::Low,
            _ => Criticality::Unknown,
        }
    }

    fn incident(item: IncidentItem) -> Option<Incident> {
        let details = item.incident_details?;
        // `id` names a version of the incident; `originalId` stays across its updates.
        let id = details.original_id.filter(|i| !i.is_empty()).or(details.id).filter(|i| !i.is_empty())?;
        let text = |t: Option<Text>| t.map(|t| t.value).filter(|v| !v.trim().is_empty());
        Some(Incident {
            id,
            kind: details.kind.as_deref().map_or(IncidentKind::Other, Self::kind),
            criticality: details.criticality.as_deref().map_or(Criticality::Unknown, Self::criticality),
            road_closed: details.road_closed,
            description: text(details.description).or_else(|| text(details.summary)),
            shape: Self::lines(item.location),
            start_ms: details.start_time.as_deref().and_then(Rfc3339::epoch_ms),
            end_ms: details.end_time.as_deref().and_then(Rfc3339::epoch_ms),
        })
    }
}

impl fmt::Debug for Here {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Here")
            .field("api_key", &"[apiKey]")
            .field("base", &self.base)
            .field("user_agent", &self.user_agent)
            .field("functional_classes", &self.functional_classes)
            .field("language", &self.language)
            .finish()
    }
}

impl TrafficProvider for Here {
    fn flow_request(&self, bounds: &Bounds) -> Result<Request, TrafficError> {
        self.request("flow", bounds, false)
    }

    fn parse_flow(&self, response: &Response) -> Result<Traffic, TrafficError> {
        if let Some(error) = self.error(response) {
            return Err(error);
        }
        let body: FlowBody = serde_json::from_str(&response.body).map_err(|e| TrafficError::Parse(e.to_string()))?;
        let mut flow = Vec::with_capacity(body.results.len());
        for item in body.results {
            Self::segments(item, &mut flow);
        }
        Ok(Traffic { flow, incidents: Vec::new(), updated_ms: body.source_updated.as_deref().and_then(Rfc3339::epoch_ms) })
    }

    fn incidents_request(&self, bounds: &Bounds) -> Result<Request, TrafficError> {
        self.request("incidents", bounds, true)
    }

    fn parse_incidents(&self, response: &Response) -> Result<Traffic, TrafficError> {
        if let Some(error) = self.error(response) {
            return Err(error);
        }
        let body: IncidentsBody = serde_json::from_str(&response.body).map_err(|e| TrafficError::Parse(e.to_string()))?;
        let incidents = body.results.into_iter().filter_map(Self::incident).collect();
        Ok(Traffic { flow: Vec::new(), incidents, updated_ms: body.source_updated.as_deref().and_then(Rfc3339::epoch_ms) })
    }

    /// One degree of width and height, less the 0.00001° that rounding the edges to five
    /// decimals may add.
    fn max_area(&self) -> AreaLimit {
        let span = Self::MAX_SPAN_DEG - 10f64.powi(-Self::DECIMALS);
        AreaLimit { max_width_deg: Some(span), max_height_deg: Some(span), max_area_km2: None }
    }
}
