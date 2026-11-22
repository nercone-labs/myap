use crate::navigation::{Location, NavigationState, Session};
use crate::protocol::*;
use crate::regions::{Region, RegionSource, RegionStatus, Regions};
use crate::settings::{ElevationSource, GoogleUse, NightMode, RouterKind, Settings};
use crate::store::{GoogleLog, GoogleRecord, Places, SavedLabel, Store};
use crate::style::{FeatureKind, Style};
use crate::words::{Message, Phrasebook, StepWords, Words};
use fetch::{Request, Response as HttpResponse};
use geo::{Bounds, Coordinate, Polygon, Sun};
use geocoding::{Nominatim, Photon, SearchRequest};
use google::{Google, GoogleError};
use myap_router::{Graph, GraphError, Router};
use routing::{AlertKind, RemoteRouter, Route, RouteError, RouteRequest, TravelMode};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

mod directions;
mod downloads;
mod elevation;
mod guidance;
mod live;
mod places;
mod view;

use places::{SearchRun, SearchSource};

/// What a fetch in flight was for.
#[derive(Debug, Clone)]
enum Pending {
    GoogleSession,
    Suggest {
        generation: u64,
        google: bool,
    },
    Search {
        generation: u64,
        source: SearchSource,
    },
    /// The branches of the places Photon found for search `generation` (Nominatim).
    Branches {
        generation: u64,
    },
    /// The address of a dropped pin (Nominatim), for selection `selection`.
    Reverse {
        selection: u64,
    },
    /// The details of the place selected, from Google.
    Details {
        selection: u64,
    },
    /// The place of OpenStreetMap that is the place of Google selected (Photon).
    Counterpart {
        selection: u64,
    },
    /// The tags of the place of OpenStreetMap selected or linked (Nominatim).
    OsmDetails {
        selection: u64,
    },
    Route {
        generation: u64,
        router: RouterKind,
        request: RouteRequest,
    },
    Region {
        id: String,
    },
    /// Live traffic of an area, for the directions of `generation` or the check of the guidance.
    Traffic {
        generation: u64,
        incidents: bool,
    },
    /// A terrain tile of a region's elevations, saved to a file.
    ElevationTile {
        region: String,
    },
    /// A route asked during guidance to compare with the one followed.
    Check {
        generation: u64,
        router: RouterKind,
        request: RouteRequest,
    },
}

/// The live traffic last fetched: what, where and when.
#[derive(Debug, Clone)]
struct TrafficData {
    at_ms: i64,
    areas: Vec<Bounds>,
    data: traffic::Traffic,
}

/// The download of the terrain tiles of a region's elevations.
#[derive(Debug, Clone)]
struct ElevationDownload {
    source: ElevationSource,
    zoom: u8,
    directory: String,
    waiting: VecDeque<geo::Tile>,
    in_flight: usize,
    total: usize,
}

/// The directions being planned, and how far the routers have been tried.
#[derive(Debug, Clone)]
struct Directions {
    origin: Endpoint,
    destination: Endpoint,
    stops: Vec<Endpoint>,
    mode: TravelMode,
    avoid: routing::Avoid,
    choice: RouterChoice,
    request: Option<RouteRequest>,
    routes: Vec<(RouterKind, Route)>,
    selected: usize,
    loading: bool,
    error: Option<String>,
    notice: Option<String>,
    /// The routers still to try, in order, and the errors of those tried.
    remaining: VecDeque<RouterKind>,
    failures: Vec<(RouterKind, RouteError)>,
    generation: u64,
    /// Whether this planning is a reroute of the navigation.
    reroute: bool,
    /// Waiting for live traffic before asking Myap Router.
    waiting_for_traffic: bool,
    /// Waiting for the first location, to plan from it.
    waiting_for_location: bool,
}

/// The guidance in progress.
#[derive(Debug, Clone)]
struct Navigation {
    session: Session,
    router: RouterKind,
    /// The places still to reach, the destination last.
    targets: Vec<Coordinate>,
    avoid: routing::Avoid,
    /// A faster route found, offered to the traveller, with the seconds it saves.
    faster: Option<(RouterKind, Route, f64)>,
    /// When the route was last compared with live traffic, and the generation of that check.
    last_check_ms: i64,
    check: u64,
    /// When the destination was reached.
    arrived_ms: Option<i64>,
}

/// The core of Myap: a state machine without network access. The platform hands it inputs
/// ([`Input`]) and carries out the effects of each response ([`Effect`]).
pub struct Core {
    store: Store,
    settings: Settings,
    places: Places,
    google_log: GoogleLog,
    regions: Regions,
    locale: String,
    utc_offset_minutes: i32,
    dark: bool,
    now: i64,
    location: Option<Location>,
    viewport: Option<(Coordinate, Bounds, f64)>,
    next_id: u64,
    pending: HashMap<u64, Pending>,
    search: SearchView,
    search_generation: u64,
    /// The search submitted, while its sources answer.
    search_run: Option<SearchRun>,
    /// The generation of the place selected, and how many lookups of it are still to answer.
    selection: u64,
    selection_waiting: u32,
    /// The generation of directions, reroutes and checks of the guidance: apart from that of the
    /// search, so that planning never makes a search in progress stale.
    plan_generation: u64,
    selected: Option<PlaceView>,
    directions: Option<Directions>,
    navigation: Option<Navigation>,
    google_session: google::Session,
    /// The Google requests waiting for the session to be opened.
    google_queue: Vec<(u64, GoogleUse, Request)>,
    google_session_open: bool,
    google_error: Option<String>,
    suggest_counter: u32,
    nominatim_last: Option<i64>,
    nominatim_queue: VecDeque<(u64, Request)>,
    router: Option<(String, Router)>,
    /// The output of each graph build in progress, to remove it if its region is gone.
    builds: HashMap<u64, String>,
    camera: Option<Camera>,
    camera_id: u64,
    message: Option<Notice>,
    effects: Vec<Effect>,
    traffic: Option<TrafficData>,
    /// The traffic fetches in flight, and for which generation.
    traffic_waiting: HashMap<u64, u64>,
    /// The area of each flow fetch in flight, held once its answer came.
    traffic_pieces: HashMap<u64, Bounds>,
    elevation_downloads: HashMap<String, ElevationDownload>,
}

impl Default for Core {
    fn default() -> Self {
        Core::new()
    }
}

impl Core {
    /// The User-Agent of Myap's requests to open services, as their policies ask.
    pub const USER_AGENT: &'static str = concat!("Myap/", env!("CARGO_PKG_VERSION"), " (+https://github.com/nercone-labs/myap)");
    /// How close to a stop the traveller must come for it to be reached.
    const STOP_REACHED_M: f64 = 40.0;
    /// A fix less precise than this is not used for guidance while a better one is recent.
    const POOR_ACCURACY_M: f64 = 100.0;
    /// The least time between requests to Nominatim (its policy: one a second).
    const NOMINATIM_INTERVAL_MS: i64 = 1_100;
    /// Live traffic older than this is not used, in milliseconds.
    const TRAFFIC_MAX_AGE_MS: i64 = 15 * 60_000;
    /// The most areas of traffic fetched for one trip (each at most a degree wide, for HERE).
    const TRAFFIC_MAX_PIECES: usize = 6;
    /// The points of the elevation profile of a route.
    pub const PROFILE_POINTS: usize = 120;
    /// The most terrain tiles of a region, and how many are fetched at once.
    const ELEVATION_MAX_TILES: usize = 1_500;
    const ELEVATION_PARALLEL: usize = 4;

    pub fn new() -> Core {
        Core {
            store: Store { directory: None },
            settings: Settings::default(),
            places: Places::default(),
            google_log: GoogleLog::default(),
            regions: Regions::default(),
            locale: "en-US".into(),
            utc_offset_minutes: 0,
            dark: false,
            now: 0,
            location: None,
            viewport: None,
            next_id: 1,
            pending: HashMap::new(),
            search: SearchView::default(),
            search_generation: 0,
            search_run: None,
            selection: 0,
            selection_waiting: 0,
            plan_generation: 0,
            selected: None,
            directions: None,
            navigation: None,
            google_session: google::Session::new(),
            google_queue: Vec::new(),
            google_session_open: false,
            google_error: None,
            suggest_counter: 0,
            nominatim_last: None,
            nominatim_queue: VecDeque::new(),
            router: None,
            builds: HashMap::new(),
            camera: None,
            camera_id: 0,
            message: None,
            effects: Vec::new(),
            traffic: None,
            traffic_waiting: HashMap::new(),
            traffic_pieces: HashMap::new(),
            elevation_downloads: HashMap::new(),
        }
    }

    /// Handles one input given as JSON, and answers with JSON: what the C interface calls.
    pub fn handle_json(&mut self, input: &str) -> String {
        let response = match serde_json::from_str::<Input>(input) {
            Ok(input) => self.handle(input),
            Err(e) => Response { effects: Vec::new(), snapshot: None, error: Some(format!("unreadable input: {e}")) },
        };
        serde_json::to_string(&response).unwrap_or_else(|e| format!(r#"{{"effects":[],"error":"unwritable response: {e}"}}"#))
    }

    pub fn handle(&mut self, input: Input) -> Response {
        self.now = input.now().max(self.now);
        let mut error = None;
        match input {
            Input::Start { data_dir, locale, utc_offset_minutes, .. } => {
                self.locale = locale;
                self.utc_offset_minutes = utc_offset_minutes;
                error = self.start(data_dir.map(PathBuf::from));
            }
            Input::Locale { locale, utc_offset_minutes, .. } => {
                self.locale = locale;
                self.utc_offset_minutes = utc_offset_minutes;
            }
            Input::Appearance { dark, .. } => self.dark = dark,
            Input::Configure { settings, .. } => self.configure(settings),
            Input::Fetched { id, response, .. } => self.fetched(id, response),
            Input::GraphBuilt { id, result, .. } => self.graph_built(id, result),
            Input::Located { location, .. } => self.located(location),
            Input::Viewport { center, bounds, zoom, .. } => self.viewport = Some((center, bounds, zoom)),
            Input::Suggest { query, .. } => self.suggest(query),
            Input::Search { query, .. } => {
                self.close_directions();
                self.search(query);
            }
            Input::ClearSearch { .. } => {
                self.search_generation += 1;
                self.search = SearchView::default();
                self.search_run = None;
                self.deselect();
            }
            Input::Select { place, .. } => {
                self.close_directions();
                self.select(place);
            }
            Input::Pin { coordinate, .. } => {
                self.close_directions();
                self.pin(coordinate);
            }
            Input::Deselect { .. } => self.deselect(),
            Input::Plan { origin, destination, stops, mode, avoid, router, .. } => self.plan(origin, destination, stops, mode, avoid, router),
            Input::ChooseRoute { index, .. } => {
                if let Some(d) = &mut self.directions
                    && index < d.routes.len()
                {
                    d.selected = index;
                }
            }
            Input::CloseDirections { .. } => self.close_directions(),
            Input::StartNavigation { .. } => self.start_navigation(),
            Input::StopNavigation { .. } => self.stop_navigation(),
            Input::Save { place, label, .. } => {
                self.places.save(place, label, self.now);
                self.save_places();
            }
            Input::Unsave { id, .. } => {
                self.places.saved.retain(|s| s.place.id != id);
                self.save_places();
            }
            Input::ClearHistory { .. } => {
                self.places.recent.clear();
                self.places.searches.clear();
                self.save_places();
            }
            Input::ClearGoogleRecord { .. } => {
                self.google_log.records.clear();
                self.google_session.clear();
                self.google_session_open = false;
                self.google_error = None;
                self.save_google_log();
            }
            Input::AddRegionArea { name, bounds, .. } => self.add_region(name, RegionSource::Area { bounds }, None),
            Input::AddRegionUrl { name, url, .. } => self.add_region(name, RegionSource::Url { url }, None),
            Input::AddRegionFile { name, path, .. } => self.add_region(name, RegionSource::File { path: Some(path.clone()) }, Some(path)),
            Input::RemoveRegion { id, .. } => self.remove_region(&id),
            Input::AcceptFasterRoute { .. } => self.accept_faster_route(),
            Input::DismissFasterRoute { .. } => {
                if let Some(nav) = self.navigation.as_mut() {
                    nav.faster = None;
                }
            }
            Input::AvoidArea { name, center, radius_m, hours, .. } => {
                let until = hours.map(|h| self.now + h as i64 * 3_600_000);
                self.add_avoid_area(name, Polygon::circle(&center, radius_m.clamp(10.0, 20_000.0), 24), until);
            }
            Input::RemoveAvoidArea { id, .. } => {
                self.settings.routing.avoid_areas.retain(|a| a.id != id);
                self.save_settings();
                self.replan();
            }
            Input::AvoidAhead { .. } => self.avoid_ahead(),
            Input::Tick { .. } => self.tick(),
            Input::Snapshot { .. } => {}
        }
        self.flush_nominatim();
        let mut effects = std::mem::take(&mut self.effects);
        // A later wake replaces an earlier one at the platform: one, at the earliest time asked.
        let wake = effects.iter().filter_map(|e| if let Effect::Wake { at } = e { Some(*at) } else { None }).min();
        effects.retain(|e| !matches!(e, Effect::Wake { .. }));
        if let Some(at) = wake {
            effects.push(Effect::Wake { at });
        }
        Response { effects, snapshot: Some(Box::new(self.snapshot())), error }
    }

    fn book(&self) -> &'static dyn Phrasebook {
        Words::book(self.settings.language, &self.locale)
    }

    fn imperial(&self) -> bool {
        Words::imperial(self.settings.units, &self.locale)
    }

    fn region_code(&self) -> String {
        self.locale.rsplit(['-', '_']).next().filter(|r| r.len() == 2).unwrap_or("US").to_string()
    }

    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.effects.push(Effect::Log { level, message: message.into() });
    }

    fn tell(&mut self, text: impl Into<String>) {
        let id = self.id();
        self.message = Some(Notice { id, text: text.into() });
    }

    fn move_camera(&mut self, bounds: Option<Bounds>, center: Option<Coordinate>, zoom: Option<f64>) {
        self.camera_id += 1;
        self.camera = Some(Camera { id: self.camera_id, bounds, center, zoom });
    }

    // ---- Files ----

    fn start(&mut self, directory: Option<PathBuf>) -> Option<String> {
        self.store = Store { directory };
        let mut problems = Vec::new();
        match self.store.load::<Settings>(Store::SETTINGS) {
            Ok(settings) => self.settings = settings.unwrap_or_default().normalized(),
            Err(e) => problems.push(e),
        }
        match self.store.load::<Places>(Store::PLACES) {
            Ok(places) => self.places = places.unwrap_or_default(),
            Err(e) => problems.push(e),
        }
        match self.store.load::<GoogleLog>(Store::GOOGLE_LOG) {
            Ok(log) => self.google_log = log.unwrap_or_default(),
            Err(e) => problems.push(e),
        }
        match self.store.load::<Regions>(Store::REGIONS) {
            Ok(regions) => self.regions = regions.unwrap_or_default(),
            Err(e) => problems.push(e),
        }
        // A download or build that was running when the application ended did not finish.
        for region in &mut self.regions.regions {
            if matches!(region.status, RegionStatus::Downloading | RegionStatus::Building) {
                region.status = RegionStatus::Failed;
                region.error = Some("interrupted".into());
                region.job = None;
            }
            // The graph is whole; only its elevations were not added.
            if region.status == RegionStatus::Elevating {
                region.status = RegionStatus::Ready;
                region.job = None;
                region.progress = None;
            }
        }
        // Nothing is downloading or building at a start: what is left among the downloads (a file
        // cut off, the tiles of elevations, the copy of an imported file) was interrupted and is
        // not wanted.
        if let Some(downloads) = self.store.path(Store::DOWNLOADS_DIRECTORY)
            && let Ok(entries) = std::fs::read_dir(downloads)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
            }
        }
        for problem in &problems {
            self.effects.push(Effect::Log { level: LogLevel::Warning, message: problem.clone() });
        }
        (!problems.is_empty()).then(|| problems.join("; "))
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.store.save(Store::SETTINGS, &self.settings) {
            self.log(LogLevel::Error, e);
        }
    }

    fn save_places(&mut self) {
        if let Err(e) = self.store.save(Store::PLACES, &self.places) {
            self.log(LogLevel::Error, e);
        }
    }

    fn save_google_log(&mut self) {
        if let Err(e) = self.store.save(Store::GOOGLE_LOG, &self.google_log) {
            self.log(LogLevel::Error, e);
        }
    }

    fn save_regions(&mut self) {
        if let Err(e) = self.store.save(Store::REGIONS, &self.regions) {
            self.log(LogLevel::Error, e);
        }
    }

    fn configure(&mut self, settings: Settings) {
        let google_was = self.settings.google.enabled;
        self.settings = settings.normalized();
        if google_was && !self.settings.google.enabled {
            // Turning Google off ends its session.
            self.google_session.clear();
            self.google_session_open = false;
            self.pending.retain(|_, p| !matches!(p, Pending::GoogleSession));
        }
        self.drop_disallowed_google();
        self.save_settings();
    }

    /// Drops the Google requests waiting for the session whose use is no longer allowed, and
    /// answers each as failed, so that what waited for it goes on without Google (a search falls
    /// back to open data, directions to the next router).
    fn drop_disallowed_google(&mut self) {
        let google = self.settings.google.clone();
        let (keep, drop): (Vec<_>, Vec<_>) = std::mem::take(&mut self.google_queue).into_iter().partition(|(_, purpose, _)| google.allows(*purpose));
        self.google_queue = keep;
        for (id, _, _) in drop {
            self.fetched(id, HttpResponse::failed("Google was turned off"));
        }
    }

    // ---- Fetching ----

    fn fetch(&mut self, request: Request, pending: Pending) -> u64 {
        let id = self.id();
        self.pending.insert(id, pending);
        self.effects.push(Effect::Fetch { id, request });
        id
    }

    fn google(&self) -> Google {
        Google::new(self.book().tag(), &self.region_code())
    }

    /// Sends a request to Google for `purpose`, after opening the session, and records what it
    /// sends. Only called when the settings allow `purpose`.
    fn fetch_google(&mut self, purpose: GoogleUse, sent: String, request: Request, pending: Pending) -> u64 {
        debug_assert!(self.settings.google.allows(purpose));
        if self.settings.google.keep_record {
            self.google_log.add(GoogleRecord { time_ms: self.now, purpose, sent });
            self.save_google_log();
        }
        let id = self.id();
        self.pending.insert(id, pending);
        if self.google_session.is_started() {
            let request = self.google_session.apply(request);
            self.effects.push(Effect::Fetch { id, request });
        } else {
            self.google_queue.push((id, purpose, request));
            if !self.google_session_open {
                self.google_session_open = true;
                if self.settings.google.keep_record {
                    self.google_log.add(GoogleRecord { time_ms: self.now, purpose, sent: "Google Maps (new session)".into() });
                    self.save_google_log();
                }
                let session = self.google().session_request();
                self.fetch(session, Pending::GoogleSession);
            }
        }
        id
    }

    fn fetch_nominatim(&mut self, request: Request, pending: Pending) {
        let id = self.id();
        self.pending.insert(id, pending);
        self.nominatim_queue.push_back((id, request));
    }

    /// Sends the next request to Nominatim when its interval allows, else wakes up for it.
    fn flush_nominatim(&mut self) {
        if self.nominatim_queue.is_empty() {
            return;
        }
        let ready = self.nominatim_last.is_none_or(|t| self.now - t >= Self::NOMINATIM_INTERVAL_MS);
        if ready && let Some((id, request)) = self.nominatim_queue.pop_front() {
            self.nominatim_last = Some(self.now);
            self.effects.push(Effect::Fetch { id, request });
        }
        if let (false, Some(t)) = (self.nominatim_queue.is_empty(), self.nominatim_last) {
            self.effects.push(Effect::Wake { at: t + Self::NOMINATIM_INTERVAL_MS });
        }
    }

    fn fetched(&mut self, id: u64, response: HttpResponse) {
        let Some(pending) = self.pending.remove(&id) else { return };
        match pending {
            Pending::GoogleSession => {
                self.google_session.absorb(&response);
                if let Err(e) = Google::session_check(&response) {
                    self.google_error = Some(e.to_string());
                }
                self.drop_disallowed_google();
                for (id, _, request) in std::mem::take(&mut self.google_queue) {
                    if self.pending.contains_key(&id) {
                        let request = self.google_session.apply(request);
                        self.effects.push(Effect::Fetch { id, request });
                    }
                }
            }
            Pending::Suggest { generation, google } => self.suggestions_arrived(generation, google, &response),
            Pending::Search { generation, source } => self.results_arrived(generation, source, &response),
            Pending::Branches { generation } => self.branches_arrived(generation, &response),
            Pending::Reverse { selection } => self.reverse_arrived(selection, &response),
            Pending::Details { selection } => self.details_arrived(selection, &response),
            Pending::Counterpart { selection } => self.counterpart_arrived(selection, &response),
            Pending::OsmDetails { selection } => self.osm_details_arrived(selection, &response),
            Pending::Route { generation, router, request } => self.route_arrived(generation, router, &request, &response),
            Pending::Region { id } => self.region_downloaded(&id, &response),
            Pending::Traffic { generation, incidents } => self.traffic_arrived(id, generation, incidents, &response),
            Pending::ElevationTile { region } => self.elevation_tile_arrived(&region),
            Pending::Check { generation, router, request } => self.check_arrived(generation, router, &request, &response),
        }
    }

    fn photon(&self) -> Photon {
        Photon { base: self.settings.servers.photon.clone(), user_agent: Self::USER_AGENT.into() }
    }

    fn nominatim(&self) -> Nominatim {
        Nominatim { base: self.settings.servers.nominatim.clone(), user_agent: Self::USER_AGENT.into() }
    }

    fn near(&self) -> Coordinate {
        self.viewport.map(|v| v.0).or(self.location.map(|l| l.coordinate)).unwrap_or(Coordinate::new(35.681236, 139.767125))
    }

    fn search_request(&self, query: &str, limit: u32) -> SearchRequest {
        SearchRequest { query: query.into(), near: Some(self.near()), bounds: self.viewport.map(|v| v.1), bounded: false, language: self.book().tag().into(), limit }
    }

    fn note_google_error(&mut self, error: &GoogleError) {
        if let GoogleError::Blocked(_) = error {
            self.google_error = Some(self.book().message(Message::GoogleRefused).to_string());
        } else {
            self.google_error = Some(error.to_string());
        }
    }
}
