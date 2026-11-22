use geo::Coordinate;
use serde::{Deserialize, Serialize};

/// The flow of traffic along one stretch of road, in its direction of travel (the order of
/// `shape`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowSegment {
    /// The line of the road, two points or more.
    pub shape: Vec<Coordinate>,
    /// The length of the stretch in metres, as the provider measured it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_m: Option<f64>,
    /// The speed that traffic is expected to travel at now, in km/h.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_kmh: Option<f64>,
    /// The speed of the road without traffic (the reference speed), in km/h.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_flow_kmh: Option<f64>,
    /// The amount of congestion, from 0 (none) to 10 (blocked), on the scale of HERE that other
    /// providers are mapped to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jam_factor: Option<f64>,
    /// The share of live measurements in the speed, from 0 to 1. Above 0.7 the speed is
    /// measured now; from 0.5 to 0.7 it is historical; at 0.5 and below it is derived from the
    /// speed limit and says nothing about traffic (the scale of HERE Traffic API v7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// The road cannot be driven now.
    #[serde(default)]
    pub closed: bool,
}

/// How congested a road is, for drawing it on a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Congestion {
    Free,
    Light,
    Moderate,
    Heavy,
    /// Traffic barely moves.
    Stopped,
    /// The road cannot be driven.
    Closed,
    /// Nothing is known: no values, or values derived from the speed limit only.
    Unknown,
}

impl FlowSegment {
    /// The confidence at or below which values are not about traffic (HERE: "speed limit").
    pub const MIN_CONFIDENCE: f64 = 0.5;
    /// The bounds of [`FlowSegment::speed_factor`].
    pub const MIN_SPEED_FACTOR: f64 = 0.05;
    pub const MAX_SPEED_FACTOR: f64 = 1.5;
    /// The lowest jam factor of [`Congestion::Light`], [`Congestion::Moderate`],
    /// [`Congestion::Heavy`] and [`Congestion::Stopped`]; 10 is a closure.
    pub const JAM_THRESHOLDS: [f64; 4] = [2.0, 4.0, 7.0, 9.0];
    /// The ratio of the speed to the free-flow speed below which a segment is
    /// [`Congestion::Light`], [`Congestion::Moderate`], [`Congestion::Heavy`] and
    /// [`Congestion::Stopped`], when there is no jam factor.
    pub const RATIO_THRESHOLDS: [f64; 4] = [0.85, 0.65, 0.45, 0.25];

    /// Whether the values are about traffic: a confidence above [`FlowSegment::MIN_CONFIDENCE`],
    /// or none given (a provider without confidences).
    pub fn is_reliable(&self) -> bool {
        self.confidence.is_none_or(|c| c.is_finite() && c > Self::MIN_CONFIDENCE)
    }

    /// How congested the segment is.
    ///
    /// [`Congestion::Closed`] when it is closed or its jam factor is 10. Otherwise, when it is
    /// reliable ([`FlowSegment::is_reliable`]): from the jam factor, by
    /// [`FlowSegment::JAM_THRESHOLDS`] (below 2 free, below 4 light, below 7 moderate, below 9
    /// heavy, else stopped); without one, from the speed over the free-flow speed, by
    /// [`FlowSegment::RATIO_THRESHOLDS`] (at least 0.85 free, 0.65 light, 0.45 moderate, 0.25
    /// heavy, else stopped). [`Congestion::Unknown`] in every other case.
    pub fn congestion(&self) -> Congestion {
        let jam = self.jam_factor.filter(|j| j.is_finite() && *j >= 0.0);
        if self.closed || jam.is_some_and(|j| j >= 10.0) {
            return Congestion::Closed;
        }
        if !self.is_reliable() {
            return Congestion::Unknown;
        }
        const LEVELS: [Congestion; 5] = [Congestion::Free, Congestion::Light, Congestion::Moderate, Congestion::Heavy, Congestion::Stopped];
        if let Some(jam) = jam {
            return LEVELS[Self::JAM_THRESHOLDS.iter().filter(|&&t| jam >= t).count()];
        }
        match self.ratio() {
            Some(ratio) => LEVELS[Self::RATIO_THRESHOLDS.iter().filter(|&&t| ratio < t).count()],
            None => Congestion::Unknown,
        }
    }

    /// The speed now over the free-flow speed, clamped to [`FlowSegment::MIN_SPEED_FACTOR`] to
    /// [`FlowSegment::MAX_SPEED_FACTOR`]: the factor by which the time to drive the road
    /// without traffic divides. `None` when either speed is unknown, the free-flow speed is
    /// not positive, the values are not reliable ([`FlowSegment::is_reliable`]), or the road is
    /// closed (a closure is told by `closed`, not by a factor).
    pub fn speed_factor(&self) -> Option<f64> {
        if self.closed || !self.is_reliable() {
            return None;
        }
        self.ratio().map(|r| r.clamp(Self::MIN_SPEED_FACTOR, Self::MAX_SPEED_FACTOR))
    }

    fn ratio(&self) -> Option<f64> {
        let speed = self.speed_kmh.filter(|s| s.is_finite() && *s >= 0.0)?;
        let free = self.free_flow_kmh.filter(|f| f.is_finite() && *f > 0.0)?;
        Some(speed / free).filter(|r| r.is_finite())
    }
}
