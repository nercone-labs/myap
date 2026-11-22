use geo::Coordinate;
use serde::{Deserialize, Serialize};

/// What the traveller does at the beginning of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManeuverKind {
    /// Leave the first waypoint.
    Depart,
    /// Reach a waypoint (the last one, or an intermediate one).
    Arrive,
    /// Turn at a junction.
    Turn,
    /// Go on along the road, which bends or which others join.
    Continue,
    /// The road changes its name, without a turn.
    NewName,
    /// Join a larger road from a ramp.
    Merge,
    /// Take a ramp onto a motorway or expressway.
    OnRamp,
    /// Take a ramp off a motorway or expressway.
    OffRamp,
    /// Keep to one side where the road splits.
    Fork,
    /// The road ends: turn left or right.
    EndOfRoad,
    /// Enter a roundabout and take an exit (`Maneuver::exit`).
    Roundabout,
    /// Leave a roundabout.
    ExitRoundabout,
    /// Board a ferry.
    Ferry,
    /// Board public transport (`Step::transit`).
    Transit,
}

/// The direction of a maneuver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    UTurn,
    SharpRight,
    Right,
    SlightRight,
    Straight,
    SlightLeft,
    Left,
    SharpLeft,
}

impl Modifier {
    /// The direction of a turn of `angle` degrees (from the bearing before to the bearing after,
    /// positive to the right), by the usual classes: straight within 20°, slight to 50°, a turn to
    /// 135°, sharp to 170°, a U-turn beyond.
    pub fn from_angle(angle: f64) -> Modifier {
        let a = angle.abs();
        let right = angle > 0.0;
        match a {
            a if a < 20.0 => Modifier::Straight,
            a if a < 50.0 => {
                if right {
                    Modifier::SlightRight
                } else {
                    Modifier::SlightLeft
                }
            }
            a if a < 135.0 => {
                if right {
                    Modifier::Right
                } else {
                    Modifier::Left
                }
            }
            a if a < 170.0 => {
                if right {
                    Modifier::SharpRight
                } else {
                    Modifier::SharpLeft
                }
            }
            _ => Modifier::UTurn,
        }
    }

    pub fn is_left(&self) -> bool {
        matches!(self, Modifier::SharpLeft | Modifier::Left | Modifier::SlightLeft)
    }

    pub fn is_right(&self) -> bool {
        matches!(self, Modifier::SharpRight | Modifier::Right | Modifier::SlightRight)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Maneuver {
    pub kind: ManeuverKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<Modifier>,
    pub location: Coordinate,
    /// The bearing before and after the maneuver, in degrees, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearing_before: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearing_after: Option<f64>,
    /// The number of the exit to take from a roundabout (1 is the first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<u32>,
}

impl Maneuver {
    pub fn new(kind: ManeuverKind, modifier: Option<Modifier>, location: Coordinate) -> Maneuver {
        Maneuver { kind, modifier, location, bearing_before: None, bearing_after: None, exit: None }
    }
}
