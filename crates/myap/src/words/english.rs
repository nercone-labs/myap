use super::{Message, Phrasebook, StepWords};
use crate::settings::Units;
use routing::{AlertKind, ManeuverKind, Modifier};

pub struct English;

impl English {
    fn direction(modifier: Option<Modifier>) -> &'static str {
        match modifier {
            Some(Modifier::UTurn) => "make a U-turn",
            Some(Modifier::SharpRight) => "turn sharp right",
            Some(Modifier::Right) => "turn right",
            Some(Modifier::SlightRight) => "turn slight right",
            Some(Modifier::SlightLeft) => "turn slight left",
            Some(Modifier::Left) => "turn left",
            Some(Modifier::SharpLeft) => "turn sharp left",
            Some(Modifier::Straight) | None => "continue straight",
        }
    }

    fn side(modifier: Option<Modifier>) -> &'static str {
        if modifier.is_some_and(|m| m.is_left()) { "left" } else { "right" }
    }

    fn ordinal(n: u32) -> String {
        let suffix = match (n % 10, n % 100) {
            (1, 11) | (2, 12) | (3, 13) => "th",
            (1, _) => "st",
            (2, _) => "nd",
            (3, _) => "rd",
            _ => "th",
        };
        format!("{n}{suffix}")
    }

    fn capitalized(text: &str) -> String {
        let mut chars = text.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }

    fn lowered(text: &str) -> String {
        let mut chars = text.chars();
        match chars.next() {
            Some(first) => first.to_lowercase().chain(chars).collect(),
            None => String::new(),
        }
    }
}

impl Phrasebook for English {
    fn tag(&self) -> &'static str {
        "en-US"
    }

    fn instruction(&self, s: &StepWords) -> String {
        let onto = |text: &str| match s.road {
            Some(road) => format!("{text} onto {road}"),
            None => text.to_string(),
        };
        let toward = s.destinations.map(|d| format!(" toward {d}")).unwrap_or_default();
        let exit = s.exit_number.map(|e| format!("exit {e}: ")).unwrap_or_default();
        let text = match s.kind {
            ManeuverKind::Depart => match s.road {
                Some(road) => format!("head on {road}"),
                None => "start".into(),
            },
            ManeuverKind::Arrive if s.last => "you have arrived at your destination".into(),
            ManeuverKind::Arrive => "you have arrived at your stop".into(),
            ManeuverKind::Turn => match s.modifier {
                Some(Modifier::UTurn) => "make a U-turn".into(),
                m => format!("{}{toward}", onto(Self::direction(m))),
            },
            ManeuverKind::Continue => onto("continue straight"),
            ManeuverKind::NewName => onto("continue"),
            ManeuverKind::EndOfRoad => format!("at the end of the road, {}", onto(Self::direction(s.modifier))),
            ManeuverKind::Fork => format!("{}{toward}", onto(&format!("keep {}", Self::side(s.modifier)))),
            ManeuverKind::OnRamp => format!("{}{toward}", onto(&format!("take the ramp on the {}", Self::side(s.modifier)))),
            ManeuverKind::OffRamp => format!("{exit}{}{toward}", onto(&format!("take the exit on the {}", Self::side(s.modifier)))),
            ManeuverKind::Merge => onto("merge"),
            ManeuverKind::Roundabout => match (s.exit, s.road) {
                (Some(n), Some(road)) => format!("at the roundabout, take the {} exit onto {road}", Self::ordinal(n)),
                (Some(n), None) => format!("at the roundabout, take the {} exit", Self::ordinal(n)),
                _ => "enter the roundabout".into(),
            },
            ManeuverKind::ExitRoundabout => onto("exit the roundabout"),
            ManeuverKind::Ferry => "take the ferry".into(),
            ManeuverKind::Transit => s.instruction.map(str::to_string).unwrap_or_else(|| "take public transport".into()),
        };
        // "At Hibiya, turn left": the junction where the maneuver is, when it has a name.
        let text = match (s.junction, s.kind) {
            (Some(j), ManeuverKind::Turn | ManeuverKind::Fork | ManeuverKind::EndOfRoad) => format!("at {j}, {text}"),
            _ => text,
        };
        Self::capitalized(&text)
    }

    fn prepare(&self, distance: &str, instruction: &str) -> String {
        format!("In {distance}, {}", Self::lowered(instruction))
    }

    fn now(&self, instruction: &str, then: Option<&str>) -> String {
        match then {
            Some(then) => format!("{instruction}, then {}", Self::lowered(then)),
            None => instruction.to_string(),
        }
    }

    fn spoken_distance(&self, metres: f64, _units: Units, imperial: bool) -> String {
        if imperial {
            let miles = metres / 1609.344;
            return if miles < 0.1 {
                format!("{} feet", ((metres * 3.28084 / 50.0).round() * 50.0).max(50.0) as u64)
            } else if (miles - 1.0).abs() < 0.05 {
                "1 mile".into()
            } else {
                format!("{miles:.1} miles")
            };
        }
        if metres < 1000.0 {
            let rounded = if metres < 100.0 { (metres / 10.0).round() * 10.0 } else { (metres / 50.0).round() * 50.0 };
            format!("{} meters", rounded.max(10.0) as u64)
        } else {
            let km = metres / 1000.0;
            if (km - 1.0).abs() < 0.05 {
                "1 kilometer".into()
            } else if (km - km.round()).abs() < 0.05 {
                format!("{} kilometers", km.round() as u64)
            } else {
                format!("{km:.1} kilometers")
            }
        }
    }

    fn duration(&self, seconds: f64) -> String {
        let minutes = (seconds / 60.0).round().max(1.0) as u64;
        match (minutes / 60, minutes % 60) {
            (0, m) => format!("{m} min"),
            (h, 0) => format!("{h} h"),
            (h, m) => format!("{h} h {m} min"),
        }
    }

    fn message(&self, message: Message) -> &'static str {
        match message {
            Message::NoRoute => "No route found",
            Message::NoRoad => "No road near the start or the destination",
            Message::OutOfCoverage => "Outside the downloaded regions",
            Message::UnsupportedMode => "This mode of travel is not supported",
            Message::NetworkError => "Cannot connect to the network",
            Message::ServerError => "The server did not answer",
            Message::NoRouter => "No router is available",
            Message::NoResults => "No results",
            Message::SearchFailed => "Search failed",
            Message::RouteFrom => "Start",
            Message::MyLocation => "Your location",
            Message::LocationUnknown => "Your location is unknown",
            Message::Locating => "Finding your location…",
            Message::Rerouting => "Finding a new route",
            Message::Arrived => "You have arrived",
            Message::StartGuidance => "Starting guidance",
            Message::TollsNotAvoided => "Toll roads could not be avoided",
            Message::HighwaysNotAvoided => "Highways could not be avoided",
            Message::FerriesNotAvoided => "Ferries could not be avoided",
            Message::GoogleRefused => "Google refused the request",
            Message::RegionTooLarge => "The area is too large; zoom in",
            Message::RegionDownloading => "Downloading",
            Message::RegionBuilding => "Building routing data",
            Message::RegionReady => "Ready",
            Message::RegionFailed => "Failed",
            Message::DroppedPin => "Dropped pin",
            Message::Home => "Home",
            Message::Work => "Work",
            Message::UnpavedNotAvoided => "Unpaved roads may not have been avoided",
            Message::StepsNotAvoided => "Steps may not have been avoided",
            Message::LevelCrossingsNotAvoided => "Level crossings may not have been avoided",
            Message::AvoidedAreaEntered => "Passes through an area to avoid",
            Message::RegionElevation => "Downloading elevations",
            Message::FasterRouteTaken => "Switched to a faster route",
            Message::Estimated => "No GPS signal (estimated position)",
            Message::TrafficFailed => "Traffic could not be fetched",
        }
    }

    fn via(&self, roads: &str) -> String {
        format!("Via {roads}")
    }

    fn fallback(&self, used: &str, reason: &str) -> String {
        format!("Used {used} ({reason})")
    }

    fn ordinal_exit(&self, exit: u32) -> String {
        format!("{} exit", Self::ordinal(exit))
    }

    fn alert_name(&self, kind: AlertKind) -> &'static str {
        match kind {
            AlertKind::LevelCrossing => "Level crossing",
            AlertKind::SpeedCamera => "Speed camera",
            AlertKind::Stop => "Stop sign",
            AlertKind::TollBooth => "Toll booth",
            AlertKind::Steep => "Steep climb",
            AlertKind::Incident => "Incident",
        }
    }

    fn alert(&self, kind: AlertKind, distance: &str) -> String {
        format!("{} in {distance}", self.alert_name(kind))
    }

    fn faster_route(&self, saving: &str) -> String {
        format!("A route {saving} faster is available")
    }

    fn congestion(&self, distance: &str, length: &str) -> String {
        format!("{length} of traffic in {distance}")
    }
}
