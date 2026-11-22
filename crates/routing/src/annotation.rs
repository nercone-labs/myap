use geo::Coordinate;
use serde::{Deserialize, Serialize};

/// A value over a part of a route: from point `start` to point `end` of its geometry.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Span<T> {
    pub start: usize,
    pub end: usize,
    pub value: T,
}

impl<T: PartialEq + Copy> Span<T> {
    /// Adds `value` over `start..=end` to `spans`, joining it to the last span when it continues
    /// it with the same value.
    pub fn push(spans: &mut Vec<Span<T>>, start: usize, end: usize, value: T) {
        if let Some(last) = spans.last_mut()
            && last.value == value
            && last.end >= start.saturating_sub(1)
        {
            last.end = last.end.max(end);
            return;
        }
        spans.push(Span { start, end, value });
    }

    /// The value at point `index`, if a span holds it.
    pub fn at(spans: &[Span<T>], index: usize) -> Option<T> {
        let i = spans.partition_point(|s| s.end < index);
        spans.get(i).filter(|s| s.start <= index).map(|s| s.value)
    }
}

/// How congested a part of a route is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Congestion {
    Free,
    Light,
    Moderate,
    Heavy,
    Stopped,
    Closed,
    Unknown,
}

impl Congestion {
    /// The congestion of a road where the traffic goes at `factor` of its usual speed.
    pub fn of_speed_factor(factor: f64) -> Congestion {
        match factor {
            f if !f.is_finite() => Congestion::Unknown,
            f if f >= 0.8 => Congestion::Free,
            f if f >= 0.6 => Congestion::Light,
            f if f >= 0.4 => Congestion::Moderate,
            f if f >= 0.15 => Congestion::Heavy,
            _ => Congestion::Stopped,
        }
    }
}

/// Something on the route to warn of before reaching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    /// A railway level crossing (踏切): stop before it.
    LevelCrossing,
    /// A speed camera.
    SpeedCamera,
    /// A stop sign.
    Stop,
    /// A toll booth.
    TollBooth,
    /// A steep climb or descent.
    Steep,
    /// A road closure or an incident on the road (traffic data).
    Incident,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub kind: AlertKind,
    /// The point of the route's geometry where it is.
    pub index: usize,
    pub location: Coordinate,
    /// What it is, in the words of its source, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}
