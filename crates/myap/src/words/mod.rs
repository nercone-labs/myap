//! The words of the core, so that every platform says the same: instructions, what the voice
//! says, distances, durations and the messages of the screens. Each language is a
//! [`Phrasebook`].

mod english;
mod japanese;

pub use english::English;
pub use japanese::Japanese;

use crate::settings::{Language, Units};
use routing::{AlertKind, ManeuverKind, Modifier, Step};

/// What a step says: its maneuver, the road it leads onto, and the exit of a roundabout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepWords<'a> {
    pub kind: ManeuverKind,
    pub modifier: Option<Modifier>,
    pub road: Option<&'a str>,
    pub destinations: Option<&'a str>,
    pub exit: Option<u32>,
    /// The name of the junction (交差点名) and the number of the exit, when known.
    pub junction: Option<&'a str>,
    pub exit_number: Option<&'a str>,
    /// The router's own text, for what Myap does not word (public transport).
    pub instruction: Option<&'a str>,
    /// Whether an arrival is at the last waypoint.
    pub last: bool,
}

impl<'a> StepWords<'a> {
    pub fn of(step: &'a Step, last: bool) -> StepWords<'a> {
        StepWords {
            kind: step.maneuver.kind,
            modifier: step.maneuver.modifier,
            road: step.name.as_deref().or(step.road_ref.as_deref()).filter(|r| !r.is_empty()),
            destinations: step.destinations.as_deref(),
            exit: step.maneuver.exit,
            junction: step.junction.as_deref().filter(|j| !j.is_empty()),
            exit_number: step.exit_number.as_deref().filter(|e| !e.is_empty()),
            instruction: step.instruction.as_deref().or(step.transit.as_deref()),
            last,
        }
    }
}

/// Messages of the screens and notices of the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    NoRoute,
    NoRoad,
    OutOfCoverage,
    UnsupportedMode,
    NetworkError,
    ServerError,
    NoRouter,
    NoResults,
    SearchFailed,
    RouteFrom,
    MyLocation,
    LocationUnknown,
    Locating,
    Rerouting,
    Arrived,
    StartGuidance,
    TollsNotAvoided,
    HighwaysNotAvoided,
    FerriesNotAvoided,
    GoogleRefused,
    RegionTooLarge,
    RegionDownloading,
    RegionBuilding,
    RegionReady,
    RegionFailed,
    DroppedPin,
    Home,
    Work,
    UnpavedNotAvoided,
    StepsNotAvoided,
    LevelCrossingsNotAvoided,
    AvoidedAreaEntered,
    RegionElevation,
    FasterRouteTaken,
    Estimated,
    TrafficFailed,
}

pub trait Phrasebook {
    /// A BCP 47 tag of the language, for the voice and the routers.
    fn tag(&self) -> &'static str;
    /// The instruction of a step, written.
    fn instruction(&self, step: &StepWords) -> String;
    /// "In 300 m, turn right onto …": the announcement before a maneuver.
    fn prepare(&self, distance: &str, instruction: &str) -> String;
    /// "Turn right onto …, then turn left": the announcement at a maneuver, with the next one when
    /// it follows closely.
    fn now(&self, instruction: &str, then: Option<&str>) -> String;
    /// A distance for the voice ("300 meters", "300メートル").
    fn spoken_distance(&self, metres: f64, units: Units, imperial: bool) -> String;
    fn duration(&self, seconds: f64) -> String;
    fn message(&self, message: Message) -> &'static str;
    /// "Via 国道1号" for a route's summary.
    fn via(&self, roads: &str) -> String;
    /// "Used OSRM: Google did not answer".
    fn fallback(&self, used: &str, reason: &str) -> String;
    /// "Exit 2", for the badge of a roundabout.
    fn ordinal_exit(&self, exit: u32) -> String;
    /// The name of an alert, for the screen ("踏切").
    fn alert_name(&self, kind: AlertKind) -> &'static str;
    /// The announcement of an alert ahead ("300メートル先、踏切があります").
    fn alert(&self, kind: AlertKind, distance: &str) -> String;
    /// The offer of a faster route ("5 分早いルートがあります").
    fn faster_route(&self, saving: &str) -> String;
    /// Congestion ahead ("2 km 先から 800 m の渋滞").
    fn congestion(&self, distance: &str, length: &str) -> String;
    /// Destinations of a sign, joined for the screen and the voice.
    fn destinations(&self, list: &str) -> String {
        list.to_string()
    }
}

/// The phrasebooks and the formats that do not depend on the language.
pub struct Words;

impl Words {
    /// The phrasebook of `language`, with the system's locale for `Language::System`.
    pub fn book(language: Language, locale: &str) -> &'static dyn Phrasebook {
        match language {
            Language::Japanese => &Japanese,
            Language::English => &English,
            Language::System => {
                if locale.to_ascii_lowercase().starts_with("ja") {
                    &Japanese
                } else {
                    &English
                }
            }
        }
    }

    /// Whether to use imperial units with `units` in the region of `locale`.
    pub fn imperial(units: Units, locale: &str) -> bool {
        match units {
            Units::Metric => false,
            Units::Imperial => true,
            Units::Automatic => {
                let region = locale.rsplit(['-', '_']).next().unwrap_or("").to_ascii_uppercase();
                matches!(region.as_str(), "US" | "GB" | "LR" | "MM")
            }
        }
    }

    /// A distance for the screen: "80 m", "350 m", "1.2 km", "12 km"; "500 ft", "0.3 mi".
    pub fn distance(metres: f64, imperial: bool) -> String {
        let m = metres.max(0.0);
        if imperial {
            let miles = m / 1609.344;
            if miles < 0.1 {
                let feet = (m * 3.28084 / 50.0).round() * 50.0;
                format!("{} ft", feet.max(50.0) as u64)
            } else if miles < 10.0 {
                format!("{miles:.1} mi")
            } else {
                format!("{} mi", miles.round() as u64)
            }
        } else if m < 100.0 {
            format!("{} m", ((m / 10.0).round() * 10.0).max(10.0) as u64)
        } else if m < 1000.0 {
            format!("{} m", ((m / 50.0).round() * 50.0) as u64)
        } else if m < 10_000.0 {
            format!("{:.1} km", m / 1000.0)
        } else {
            format!("{} km", (m / 1000.0).round() as u64)
        }
    }

    /// The name of the icon of an alert, the same on every platform.
    pub fn alert_icon(kind: AlertKind) -> &'static str {
        match kind {
            AlertKind::LevelCrossing => "level_crossing",
            AlertKind::SpeedCamera => "speed_camera",
            AlertKind::Stop => "stop",
            AlertKind::TollBooth => "toll_booth",
            AlertKind::Steep => "steep",
            AlertKind::Incident => "incident",
        }
    }

    /// The local time of `utc_ms` with the offset of the time zone, as "14:05".
    pub fn clock(utc_ms: i64, offset_minutes: i32) -> String {
        let minutes = (utc_ms.div_euclid(60_000) + offset_minutes as i64).rem_euclid(24 * 60);
        format!("{:02}:{:02}", minutes / 60, minutes % 60)
    }

    /// The name of a maneuver's icon, the same on every platform: `turn_right`, `roundabout`…
    pub fn icon(kind: ManeuverKind, modifier: Option<Modifier>) -> String {
        let side = |m: Option<Modifier>| match m {
            Some(Modifier::UTurn) => "uturn",
            Some(Modifier::SharpRight) => "sharp_right",
            Some(Modifier::Right) => "right",
            Some(Modifier::SlightRight) => "slight_right",
            Some(Modifier::SlightLeft) => "slight_left",
            Some(Modifier::Left) => "left",
            Some(Modifier::SharpLeft) => "sharp_left",
            Some(Modifier::Straight) | None => "straight",
        };
        match kind {
            ManeuverKind::Depart => "depart".into(),
            ManeuverKind::Arrive => "arrive".into(),
            ManeuverKind::Continue if modifier.is_none_or(|m| m == Modifier::Straight) => "turn_straight".into(),
            ManeuverKind::Roundabout | ManeuverKind::ExitRoundabout => "roundabout".into(),
            ManeuverKind::Ferry => "ferry".into(),
            ManeuverKind::Transit => "transit".into(),
            ManeuverKind::Merge => format!("merge_{}", if modifier.is_some_and(|m| m.is_left()) { "left" } else { "right" }),
            ManeuverKind::Fork | ManeuverKind::OffRamp | ManeuverKind::OnRamp => {
                format!("fork_{}", if modifier.is_some_and(|m| m.is_left()) { "left" } else { "right" })
            }
            _ => format!("turn_{}", side(modifier)),
        }
    }
}
