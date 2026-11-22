use geo::{Coordinate, Polygon};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelMode {
    Car,
    Bicycle,
    Foot,
    Transit,
}

impl TravelMode {
    pub const ALL: [TravelMode; 4] = [TravelMode::Car, TravelMode::Bicycle, TravelMode::Foot, TravelMode::Transit];
}

/// What a route should avoid. Each is avoided wherever another way exists: a router that cannot
/// avoid something (no other way, or it does not know the feature) uses it, and the route then
/// says so (`Route::ignored_avoid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Avoid {
    pub tolls: bool,
    /// Motorways and their ramps.
    pub highways: bool,
    pub ferries: bool,
    /// Roads without a paved surface (gravel, dirt, grass…).
    pub unpaved: bool,
    /// Narrow roads: those tagged narrower than two cars can pass, and single-lane side streets.
    pub narrow: bool,
    /// Stairs, for walking and cycling.
    pub steps: bool,
    /// Climbs: routes with less ascent are preferred, and steep climbs are avoided.
    pub hills: bool,
    /// Railway level crossings (踏切).
    pub level_crossings: bool,
    /// Turns across oncoming traffic: right turns where traffic drives on the left (Japan),
    /// left turns where it drives on the right.
    pub crossing_turns: bool,
    /// Roads without street lights, for walking and cycling at night.
    pub unlit: bool,
}

impl Avoid {
    /// Every field, in order, with its name: for code that treats them alike.
    pub fn fields(&self) -> [(&'static str, bool); 10] {
        [
            ("tolls", self.tolls),
            ("highways", self.highways),
            ("ferries", self.ferries),
            ("unpaved", self.unpaved),
            ("narrow", self.narrow),
            ("steps", self.steps),
            ("hills", self.hills),
            ("level_crossings", self.level_crossings),
            ("crossing_turns", self.crossing_turns),
            ("unlit", self.unlit),
        ]
    }

    pub fn any(&self) -> bool {
        self.fields().iter().any(|(_, on)| *on)
    }

    /// The rules among these (what a road has or not), without the preferences (narrow and unlit
    /// roads, hills, turns across traffic): those a route can be said to break.
    pub fn strict(&self) -> Avoid {
        Avoid { narrow: false, hills: false, crossing_turns: false, unlit: false, ..*self }
    }

    /// What this asks to avoid that `other` does not.
    pub fn without(&self, other: &Avoid) -> Avoid {
        Avoid {
            tolls: self.tolls && !other.tolls,
            highways: self.highways && !other.highways,
            ferries: self.ferries && !other.ferries,
            unpaved: self.unpaved && !other.unpaved,
            narrow: self.narrow && !other.narrow,
            steps: self.steps && !other.steps,
            hills: self.hills && !other.hills,
            level_crossings: self.level_crossings && !other.level_crossings,
            crossing_turns: self.crossing_turns && !other.crossing_turns,
            unlit: self.unlit && !other.unlit,
        }
    }

    /// What either asks to avoid.
    pub fn or(&self, other: &Avoid) -> Avoid {
        Avoid {
            tolls: self.tolls || other.tolls,
            highways: self.highways || other.highways,
            ferries: self.ferries || other.ferries,
            unpaved: self.unpaved || other.unpaved,
            narrow: self.narrow || other.narrow,
            steps: self.steps || other.steps,
            hills: self.hills || other.hills,
            level_crossings: self.level_crossings || other.level_crossings,
            crossing_turns: self.crossing_turns || other.crossing_turns,
            unlit: self.unlit || other.unlit,
        }
    }

    /// What both ask to avoid.
    pub fn and(&self, other: &Avoid) -> Avoid {
        Avoid {
            tolls: self.tolls && other.tolls,
            highways: self.highways && other.highways,
            ferries: self.ferries && other.ferries,
            unpaved: self.unpaved && other.unpaved,
            narrow: self.narrow && other.narrow,
            steps: self.steps && other.steps,
            hills: self.hills && other.hills,
            level_crossings: self.level_crossings && other.level_crossings,
            crossing_turns: self.crossing_turns && other.crossing_turns,
            unlit: self.unlit && other.unlit,
        }
    }
}

/// What a route is chosen for, among those that avoid what was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optimize {
    /// The least time, with a preference for main roads.
    #[default]
    Fastest,
    /// The least distance on roads the mode may use.
    Shortest,
    /// The least energy: few stops, starts and climbs, steady speeds.
    Eco,
    /// Few maneuvers and main roads, even if it takes a little longer.
    Simple,
}

/// The side of the road traffic drives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrivingSide {
    #[default]
    Right,
    Left,
}

impl DrivingSide {
    /// The countries and territories where traffic drives on the left (ISO 3166-1 alpha-2), from
    /// the table of Wikipedia's "Left- and right-hand traffic" (checked 2026-11). Territories it
    /// did not list clearly are left out (they drive on the right here).
    pub const LEFT: &'static [&'static str] = &[
        "AG", "AI", "AU", "BB", "BD", "BM", "BN", "BS", "BT", "BW", "CY", "DM", "FJ", "FK", "GB", "GD", "GG", "GS", "GY", "HK", "ID", "IE", "IM", "IN", "JE",
        "JM", "JP", "KE", "KI", "KN", "KY", "LC", "LK", "LS", "MO", "MS", "MT", "MU", "MV", "MW", "MY", "MZ", "NA", "NP", "NR", "NZ", "PG", "PK", "PN", "SB",
        "SC", "SG", "SH", "SR", "SZ", "TC", "TH", "TL", "TO", "TT", "TV", "TZ", "UG", "VC", "VG", "VI", "WS", "ZA", "ZM", "ZW",
    ];

    /// The side of a country (ISO 3166-1 alpha-2, any case); the right for an unknown one.
    pub fn of_country(code: &str) -> DrivingSide {
        let code = code.to_ascii_uppercase();
        if Self::LEFT.contains(&code.as_str()) { DrivingSide::Left } else { DrivingSide::Right }
    }

    /// Whether a turn to the right (`true`) or left crosses oncoming traffic on this side.
    pub fn crosses(&self, right: bool) -> bool {
        match self {
            DrivingSide::Left => right,
            DrivingSide::Right => !right,
        }
    }
}

/// A request for routes through two or more waypoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteRequest {
    pub waypoints: Vec<Coordinate>,
    pub mode: TravelMode,
    #[serde(default)]
    pub avoid: Avoid,
    #[serde(default)]
    pub optimize: Optimize,
    /// Areas not to pass through (a closure, a district to stay out of). Like `avoid`, they are
    /// left out wherever another way exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avoid_areas: Vec<Polygon>,
    /// The side traffic drives on, for the turns that cross it.
    #[serde(default)]
    pub driving_side: DrivingSide,
    /// Whether to ask for alternatives as well as the best route.
    #[serde(default)]
    pub alternatives: bool,
    /// The heading of the traveller at the first waypoint, in degrees, when known: a new route
    /// during navigation should not begin with a U-turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<f64>,
    /// A BCP 47 language tag for the names and texts of the router.
    #[serde(default = "RouteRequest::default_language")]
    pub language: String,
    /// The time of departure, in milliseconds since the epoch, for routers that know traffic or
    /// timetables. `None` is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub departure_ms: Option<i64>,
}

impl RouteRequest {
    /// The most areas to avoid a request may carry.
    pub const MAX_AVOID_AREAS: usize = 50;

    pub fn default_language() -> String {
        "en".to_string()
    }

    pub fn new(waypoints: Vec<Coordinate>, mode: TravelMode) -> RouteRequest {
        RouteRequest {
            waypoints,
            mode,
            avoid: Avoid::default(),
            optimize: Optimize::default(),
            avoid_areas: Vec::new(),
            driving_side: DrivingSide::default(),
            alternatives: false,
            heading: None,
            language: Self::default_language(),
            departure_ms: None,
        }
    }

    pub fn origin(&self) -> Option<&Coordinate> {
        self.waypoints.first()
    }

    pub fn destination(&self) -> Option<&Coordinate> {
        self.waypoints.last()
    }

    /// Whether the request can be routed at all: two or more valid waypoints, and valid areas.
    pub fn is_valid(&self) -> bool {
        self.waypoints.len() >= 2
            && self.waypoints.iter().all(Coordinate::is_valid)
            && self.avoid_areas.len() <= Self::MAX_AVOID_AREAS
            && self.avoid_areas.iter().all(Polygon::is_valid)
    }

    /// The areas to avoid that hold no waypoint: an area round the start or the end cannot be
    /// left out, and is not asked of the router.
    pub fn effective_avoid_areas(&self) -> Vec<&Polygon> {
        self.avoid_areas.iter().filter(|a| !self.waypoints.iter().any(|w| a.contains(w))).collect()
    }
}
