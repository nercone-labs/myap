//! What the platform and the core say to each other. Every type here is serialised as JSON with
//! a `type` tag, which is how the C interface carries it.

use crate::navigation::{LocalPoint, Location, NavigationState};
use crate::regions::Region;
use crate::settings::{GoogleUse, RouterKind, Settings};
use crate::store::{GoogleRecord, SavedLabel, SavedPlace};
use crate::style::MapStyle;
use geo::{Bounds, Coordinate};
use geocoding::{Place, Suggestion};
use routing::{Avoid, Lane, Optimize, TravelMode};
use serde::{Deserialize, Serialize};

/// Where a trip begins or passes: a place, or the device's location when it starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Endpoint {
    MyLocation,
    Place { place: Box<Place> },
}

/// The router to use: automatic (by the order of the settings) or one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "router", rename_all = "snake_case")]
pub enum RouterChoice {
    Automatic,
    Only(RouterKind),
}

/// An input of the core. Every input carries the time of the platform (`now`, ms since the
/// epoch), the only clock the core knows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    /// The first input: where the core keeps its files (`None` keeps nothing), the locale of the
    /// system (a BCP 47 tag) and the offset of its time zone.
    Start {
        now: i64,
        data_dir: Option<String>,
        locale: String,
        utc_offset_minutes: i32,
    },
    /// The locale or the time zone of the system changed.
    Locale {
        now: i64,
        locale: String,
        utc_offset_minutes: i32,
    },
    /// Whether the screens are dark (for the map style).
    Appearance {
        now: i64,
        dark: bool,
    },
    Configure {
        now: i64,
        settings: Settings,
    },
    /// The answer to `Effect::Fetch`.
    Fetched {
        now: i64,
        id: u64,
        response: fetch::Response,
    },
    /// The answer to `Effect::BuildGraph`: the statistics, or the error.
    GraphBuilt {
        now: i64,
        id: u64,
        result: Result<GraphStats, String>,
    },
    /// The device's location.
    Located {
        now: i64,
        location: Location,
    },
    /// The map moved: its centre and bounds, for searching near what is shown.
    Viewport {
        now: i64,
        center: Coordinate,
        bounds: Bounds,
        zoom: f64,
    },
    /// Suggestions for what is being typed (the platform waits for a pause in typing).
    Suggest {
        now: i64,
        query: String,
    },
    Search {
        now: i64,
        query: String,
    },
    /// Closes the search, its results and the selected place.
    ClearSearch {
        now: i64,
    },
    /// Shows a place (from the results, a suggestion, the saved places, or the map).
    Select {
        now: i64,
        place: Place,
    },
    /// A point held on the map: a dropped pin, named by a reverse lookup.
    Pin {
        now: i64,
        coordinate: Coordinate,
    },
    Deselect {
        now: i64,
    },
    /// Directions to `destination`, from `origin`, through `stops`.
    Plan {
        now: i64,
        origin: Endpoint,
        destination: Endpoint,
        stops: Vec<Endpoint>,
        mode: TravelMode,
        avoid: Avoid,
        router: RouterChoice,
    },
    ChooseRoute {
        now: i64,
        index: usize,
    },
    CloseDirections {
        now: i64,
    },
    StartNavigation {
        now: i64,
    },
    StopNavigation {
        now: i64,
    },
    Save {
        now: i64,
        place: Place,
        label: SavedLabel,
    },
    Unsave {
        now: i64,
        id: String,
    },
    ClearHistory {
        now: i64,
    },
    /// Forgets the record of what was sent to Google, and starts a new Google session.
    ClearGoogleRecord {
        now: i64,
    },
    /// Downloads the roads of `bounds` (an area of the map) as a region of Myap Router.
    AddRegionArea {
        now: i64,
        name: String,
        bounds: Bounds,
    },
    /// Downloads a PBF extract from `url` as a region.
    AddRegionUrl {
        now: i64,
        name: String,
        url: String,
    },
    /// Builds a region from a PBF file the platform copied to `path`.
    AddRegionFile {
        now: i64,
        name: String,
        path: String,
    },
    RemoveRegion {
        now: i64,
        id: String,
    },
    /// Takes the faster route offered during guidance.
    AcceptFasterRoute {
        now: i64,
    },
    DismissFasterRoute {
        now: i64,
    },
    /// Avoids the area of `radius_m` round `center` in routes from now on, for `hours` (until
    /// removed when `None`); planned again when directions are shown.
    AvoidArea {
        now: i64,
        name: String,
        center: Coordinate,
        radius_m: f64,
        #[serde(default)]
        hours: Option<u32>,
    },
    RemoveAvoidArea {
        now: i64,
        id: String,
    },
    /// During guidance: avoids the road just ahead (a closure, a jam seen) for a few hours, and
    /// finds a new route.
    AvoidAhead {
        now: i64,
    },
    /// Time passed: the platform sends this at the time of `Effect::Wake`, or whenever it likes.
    Tick {
        now: i64,
    },
    Snapshot {
        now: i64,
    },
}

impl Input {
    pub fn now(&self) -> i64 {
        match self {
            Input::Start { now, .. }
            | Input::Locale { now, .. }
            | Input::Appearance { now, .. }
            | Input::Configure { now, .. }
            | Input::Fetched { now, .. }
            | Input::GraphBuilt { now, .. }
            | Input::Located { now, .. }
            | Input::Viewport { now, .. }
            | Input::Suggest { now, .. }
            | Input::Search { now, .. }
            | Input::ClearSearch { now }
            | Input::Select { now, .. }
            | Input::Pin { now, .. }
            | Input::Deselect { now }
            | Input::Plan { now, .. }
            | Input::ChooseRoute { now, .. }
            | Input::CloseDirections { now }
            | Input::StartNavigation { now }
            | Input::StopNavigation { now }
            | Input::Save { now, .. }
            | Input::Unsave { now, .. }
            | Input::ClearHistory { now }
            | Input::ClearGoogleRecord { now }
            | Input::AddRegionArea { now, .. }
            | Input::AddRegionUrl { now, .. }
            | Input::AddRegionFile { now, .. }
            | Input::RemoveRegion { now, .. }
            | Input::AcceptFasterRoute { now }
            | Input::DismissFasterRoute { now }
            | Input::AvoidArea { now, .. }
            | Input::RemoveAvoidArea { now, .. }
            | Input::AvoidAhead { now }
            | Input::Tick { now }
            | Input::Snapshot { now } => *now,
        }
    }
}

/// What a graph build made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphStats {
    pub nodes: usize,
    pub edges: usize,
    pub bounds: Bounds,
    pub size_bytes: u64,
    /// How many points of the shapes got an elevation.
    #[serde(default)]
    pub elevations: usize,
}

/// The input format of a graph build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphFormat {
    Pbf,
    OverpassJson,
    /// A graph already built, to which elevations are added.
    Graph,
}

/// The elevations to add to a graph: from the terrain tiles saved in `directory`
/// (`elevation::Source::file_name`), of `zoom`, in `encoding`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElevationInput {
    pub directory: String,
    pub zoom: u8,
    pub encoding: elevation::Encoding,
}

/// Something the platform is to do, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Fetch, and answer with `Input::Fetched` of the same `id`.
    Fetch {
        id: u64,
        request: fetch::Request,
    },
    /// The answer of `id` is no longer wanted.
    Cancel {
        id: u64,
    },
    /// Build a graph of Myap Router from `input` into `output` away from the thread of the core
    /// (it takes long), with the library's `build_graph`, and answer with `Input::GraphBuilt`.
    BuildGraph {
        id: u64,
        input: String,
        format: GraphFormat,
        output: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        elevation: Option<ElevationInput>,
    },
    /// Say `text` in `language` (a BCP 47 tag), cutting off what is being said.
    Speak {
        text: String,
        language: String,
    },
    /// Send `Input::Tick` at `at`, or earlier. A later `Wake` replaces an earlier one.
    Wake {
        at: i64,
    },
    /// Navigation began or ended: keep the device locating in the background while it runs.
    Navigating {
        active: bool,
    },
    Log {
        level: LogLevel,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

/// A step of a route, as the screens show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepView {
    pub instruction: String,
    pub distance: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
}

/// Something to warn of ahead, as the screens show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertView {
    pub icon: String,
    pub name: String,
    pub distance: String,
    pub distance_m: f64,
}

/// A faster route found during guidance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FasterRouteView {
    pub text: String,
    pub saving_minutes: u32,
    pub summary: String,
    pub router: RouterKind,
}

/// What the camera view of the guidance draws: the route ahead around the traveller, and where
/// the next maneuver is on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArView {
    /// The route ahead, metres east, north and up of the traveller, a few metres apart.
    pub points: Vec<LocalPoint>,
    /// The index in `points` of the next maneuver, when it is within them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maneuver: Option<usize>,
    /// The height of the camera above the road assumed, in metres.
    pub eye_height_m: f64,
}

/// A route, as the screens show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteView {
    pub router: RouterKind,
    pub duration: String,
    pub distance: String,
    /// The time of arrival, at the local time of the device.
    pub arrival: String,
    pub summary: String,
    /// Whether the duration counts the traffic of now.
    pub traffic: bool,
    /// How much longer the traffic makes it, in minutes, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_minutes: Option<u32>,
    pub has_tolls: bool,
    pub has_ferries: bool,
    /// Warnings: what could not be avoided.
    pub notes: Vec<String>,
    pub steps: Vec<StepView>,
    pub distance_m: f64,
    pub duration_s: f64,
    /// The climb and descent, when elevations are known ("↑ 120 m").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descent: Option<String>,
    /// The elevation along the route, at most `Core::PROFILE_POINTS` evenly along it, in metres.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profile: Vec<f32>,
    /// What is on the route to warn of: how many of each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<AlertView>,
}

/// The directions being planned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectionsView {
    /// The trip as planned: the origin, the stops, the destination last, for planning it again.
    pub endpoints: Vec<Endpoint>,
    pub origin: String,
    pub destination: String,
    pub stops: Vec<String>,
    pub mode: TravelMode,
    pub avoid: Avoid,
    pub optimize: Optimize,
    pub router: RouterChoice,
    pub routes: Vec<RouteView>,
    pub selected: usize,
    pub loading: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Which router answered, when another than the first choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// The routers that can answer this trip, for choosing one.
    pub available: Vec<RouterKind>,
}

/// The guidance, as the screens show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationView {
    pub state: NavigationState,
    pub instruction: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    /// The distance to the next maneuver.
    pub distance: String,
    pub distance_m: f64,
    /// The maneuver after it, when it follows closely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then_icon: Option<String>,
    pub remaining_distance: String,
    pub remaining_time: String,
    pub arrival: String,
    /// The distance and time left, in metres and seconds, and the time of arrival in ms since the
    /// epoch: for screens that word them themselves (a car's).
    pub remaining_m: f64,
    pub remaining_s: f64,
    pub arrival_ms: i64,
    /// The side traffic drives on (which way a U-turn or a roundabout goes).
    pub driving_side: routing::DrivingSide,
    pub progress: f64,
    pub router: RouterKind,
    /// The position on the route and its direction, for the camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Coordinate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearing: Option<f64>,
    pub mode: TravelMode,
    /// The lanes before the next maneuver, from the left one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
    /// The name of the junction of the next maneuver, the destinations of its signs and the
    /// number of its exit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub junction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destinations: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_number: Option<String>,
    /// The speed limit where the traveller is, in the units shown, and the speed now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_limit: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u16>,
    /// Whether the speed is above the limit.
    pub speeding: bool,
    pub alerts: Vec<AlertView>,
    /// The steps after the next one, with their distance from here.
    pub upcoming: Vec<StepView>,
    /// The congestion ahead, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub congestion: Option<String>,
    /// How much longer than usual the traffic makes the rest, in minutes, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faster_route: Option<FasterRouteView>,
    /// Whether the position is estimated, without fixes (a tunnel).
    pub estimated: bool,
    /// The zoom and tilt the camera should take.
    pub zoom: f64,
    pub tilt: f64,
    pub ar: ArView,
}

/// The search, as the screens show it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SearchView {
    pub query: String,
    pub suggestions: Vec<Suggestion>,
    pub results: Vec<Place>,
    pub loading: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Who answered: "OpenStreetMap", "Google".
    pub sources: Vec<String>,
    /// The distance from the traveller to each result, worded (empty while the location is not
    /// known), at the same indices as `results`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub distances: Vec<String>,
}

/// The place shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceView {
    pub place: Place,
    pub saved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<String>,
    pub loading: bool,
}

/// Where the camera should go. `id` changes with each new command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<Coordinate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
}

/// How many requests of one use of Google the record holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleCount {
    pub purpose: GoogleUse,
    pub count: usize,
}

/// A message to show once, with an id to tell a new one from the same one shown again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    pub text: String,
}

/// What the screens show of the use of Google.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleView {
    pub records: Vec<GoogleRecord>,
    /// The number of requests of each use in the record.
    pub counts: Vec<GoogleCount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Everything the screens show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub settings: Settings,
    pub language: String,
    pub imperial: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    pub search: SearchView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<PlaceView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directions: Option<DirectionsView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<NavigationView>,
    pub saved: Vec<SavedPlace>,
    pub recent: Vec<Place>,
    pub recent_searches: Vec<String>,
    pub regions: Vec<Region>,
    pub google: GoogleView,
    pub style: MapStyle,
    /// The features on the map, as a GeoJSON FeatureCollection.
    pub overlay: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<Camera>,
    /// A message to show once (an error of something not on a screen), with its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Notice>,
}

/// The answer to every input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub effects: Vec<Effect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Box<Snapshot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
