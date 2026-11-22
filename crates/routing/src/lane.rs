use serde::{Deserialize, Serialize};

/// Where a lane leads, as its arrows on the road show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneIndication {
    SharpLeft,
    Left,
    SlightLeft,
    Through,
    SlightRight,
    Right,
    SharpRight,
    UTurn,
    MergeToLeft,
    MergeToRight,
    /// No arrow.
    None,
}

impl LaneIndication {
    /// The value of OpenStreetMap's `turn:lanes` (`left`, `slight_right`, `through`, `reverse`,
    /// `merge_to_left`, `none`…), or `None` for a value it does not define.
    pub fn from_osm(value: &str) -> Option<LaneIndication> {
        use LaneIndication::*;
        Some(match value.trim() {
            "sharp_left" => SharpLeft,
            "left" => Left,
            "slight_left" => SlightLeft,
            "through" => Through,
            "slight_right" => SlightRight,
            "right" => Right,
            "sharp_right" => SharpRight,
            "reverse" => UTurn,
            "merge_to_left" => MergeToLeft,
            "merge_to_right" => MergeToRight,
            "none" | "" => None,
            _ => return Option::None,
        })
    }

    /// The indication of OSRM's lanes (`left`, `slight right`, `straight`, `uturn`, `none`…).
    pub fn from_osrm(value: &str) -> Option<LaneIndication> {
        use LaneIndication::*;
        Some(match value {
            "sharp left" => SharpLeft,
            "left" => Left,
            "slight left" => SlightLeft,
            "straight" => Through,
            "slight right" => SlightRight,
            "right" => Right,
            "sharp right" => SharpRight,
            "uturn" => UTurn,
            "none" => None,
            _ => return Option::None,
        })
    }

    /// The angle the indication points to, positive to the right; `None` for no arrow.
    pub fn angle(&self) -> Option<f64> {
        use LaneIndication::*;
        Some(match self {
            SharpLeft => -150.0,
            Left => -90.0,
            SlightLeft => -40.0,
            Through | MergeToLeft | MergeToRight => 0.0,
            SlightRight => 40.0,
            Right => 90.0,
            SharpRight => 150.0,
            UTurn => 180.0,
            None => return Option::None,
        })
    }
}

/// A lane of the road before a maneuver: where it leads, and whether it is one to take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lane {
    pub indications: Vec<LaneIndication>,
    /// Whether the lane leads where the route goes.
    pub valid: bool,
    /// Whether it is the lane to be in, among the valid ones, when the router says so.
    #[serde(default)]
    pub preferred: bool,
}

impl Lane {
    /// The lanes of a value of `turn:lanes`, from the left lane to the right one, none of them
    /// valid yet. `None` when the value is malformed (an indication it does not know).
    pub fn parse_osm(value: &str) -> Option<Vec<Lane>> {
        if value.trim().is_empty() {
            return None;
        }
        value
            .split('|')
            .map(|lane| {
                let indications = lane.split(';').map(LaneIndication::from_osm).collect::<Option<Vec<_>>>()?;
                Some(Lane { indications, valid: false, preferred: false })
            })
            .collect()
    }

    /// The widest gap between a turn and the arrow of a lane for the lane to lead there.
    const MAX_GAP_DEGREES: f64 = 60.0;

    /// Marks valid the lanes that lead towards a turn of `angle` degrees (positive to the right):
    /// those whose arrow is the closest to it. Going straight on where no lane has an arrow
    /// straight ahead, the lanes without an arrow are the valid ones. Returns whether any lane was
    /// marked: when no arrow is within `MAX_GAP_DEGREES` of the turn, the lanes say nothing of it.
    pub fn mark(lanes: &mut [Lane], angle: f64) -> bool {
        for lane in lanes.iter_mut() {
            lane.valid = false;
        }
        let plain = |l: &Lane| l.indications.iter().all(|i| *i == LaneIndication::None);
        let straight = angle.abs() < 30.0;
        let any_through = lanes.iter().any(|l| l.indications.contains(&LaneIndication::Through));
        if straight && !any_through && lanes.iter().any(plain) {
            for lane in lanes.iter_mut() {
                lane.valid = plain(lane);
            }
            return true;
        }
        let distance = |i: &LaneIndication| i.angle().map(|a| (a - angle).abs());
        let best = lanes.iter().flat_map(|l| l.indications.iter()).filter_map(distance).fold(f64::INFINITY, f64::min);
        if best > Self::MAX_GAP_DEGREES {
            return false;
        }
        for lane in lanes.iter_mut() {
            lane.valid = lane.indications.iter().filter_map(distance).any(|d| d <= best + 1e-9);
        }
        true
    }
}
