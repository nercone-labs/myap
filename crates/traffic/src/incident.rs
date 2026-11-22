use geo::Coordinate;
use serde::{Deserialize, Serialize};

/// An event on a road: an accident, road works, a closure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Incident {
    /// The identifier of the incident at its provider, stable across its updates.
    pub id: String,
    pub kind: IncidentKind,
    pub criticality: Criticality,
    /// The incident prevents travel along the road.
    pub road_closed: bool,
    /// A text for people, in the language of the provider's choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The roads affected, as lines in the direction of travel. More than one when the affected
    /// roads are not contiguous (a fork); empty when the provider gave no geometry.
    #[serde(default)]
    pub shape: Vec<Vec<Coordinate>>,
    /// When the incident begins and ends to be valid, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
}

/// What happened, in general.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentKind {
    /// A collision.
    Accident,
    /// Building or road works.
    Construction,
    /// A build-up of vehicles.
    Congestion,
    /// A vehicle that cannot move obstructs the road.
    DisabledVehicle,
    /// An event of public transport.
    MassTransit,
    /// An organised event that disrupts traffic.
    PlannedEvent,
    /// Dangerous objects on the road.
    RoadHazard,
    /// The road is closed (by the police, for example).
    RoadClosure,
    /// The weather disrupts traffic.
    Weather,
    /// Some lanes are restricted.
    LaneRestriction,
    /// Anything else, including kinds unknown to this crate.
    Other,
}

/// How severe an incident is, for drawing it; nothing more is to be inferred from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    /// Not given, or a value unknown to this crate.
    Unknown,
    Low,
    Minor,
    Major,
    Critical,
}

impl Incident {
    /// Whether the incident is valid at `now_ms` (milliseconds since the Unix epoch): not
    /// before its start, not after its end. An unknown bound does not limit it.
    pub fn is_active_at(&self, now_ms: i64) -> bool {
        self.start_ms.is_none_or(|start| start <= now_ms) && self.end_ms.is_none_or(|end| now_ms <= end)
    }
}
