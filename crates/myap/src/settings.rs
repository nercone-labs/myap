use geo::Polygon;
use routing::{Avoid, DrivingSide, Optimize, TravelMode};
use serde::{Deserialize, Serialize};

/// The language of the texts of the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// The language of the system, when the core has it, else English.
    System,
    Japanese,
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    /// Metric, except imperial in the United States, the United Kingdom, Liberia and Myanmar.
    Automatic,
    Metric,
    Imperial,
}

/// The base map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapSource {
    /// OpenStreetMap data drawn from vector tiles of OpenFreeMap.
    OpenFreeMap,
    /// The raster tiles of openstreetmap.org (OpenStreetMap Carto).
    OpenStreetMapCarto,
    /// Google's tiles (only while the Google setting allows them).
    GoogleRoadmap,
    GoogleSatellite,
    GoogleHybrid,
    GoogleTerrain,
}

/// The routers Myap can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterKind {
    /// Myap Router, on the phone, on the regions downloaded.
    Myap,
    Osrm,
    Valhalla,
    Google,
}

impl RouterKind {
    pub const ALL: [RouterKind; 4] = [RouterKind::Google, RouterKind::Valhalla, RouterKind::Osrm, RouterKind::Myap];
}

/// What Google is used for. Every use is off until the user turns it on; `enabled` turns them
/// all off at once without forgetting which were chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GoogleSettings {
    pub enabled: bool,
    /// Suggestions while typing: sends what is typed and the area of the map.
    pub suggestions: bool,
    /// Search: sends the search and the area of the map.
    pub search: bool,
    /// Details of places from Google (hours, phone, rating): sends the name, address and position
    /// of a place of Google opened, and of a place of OpenStreetMap while the sources are
    /// combined (`PlaceSettings::combine`).
    pub place_details: bool,
    /// Routes with traffic: sends the waypoints of the trip.
    pub directions: bool,
    /// Google's map tiles (as a base map): Google sees which tiles are shown.
    pub map_tiles: bool,
    /// The traffic layer over the map: Google sees which tiles are shown.
    pub traffic_layer: bool,
    /// Traffic during guidance: the route left is asked of Google again every few minutes, to
    /// find a faster one; sends the position and the places still to reach each time.
    pub navigation_traffic: bool,
    /// Keeps a record on the phone of what was sent to Google, to show in the settings.
    pub keep_record: bool,
}

impl Default for GoogleSettings {
    fn default() -> GoogleSettings {
        GoogleSettings {
            enabled: false,
            suggestions: false,
            search: false,
            place_details: false,
            directions: false,
            map_tiles: false,
            traffic_layer: false,
            navigation_traffic: false,
            keep_record: true,
        }
    }
}

/// The uses of Google, by what they send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoogleUse {
    Suggestions,
    Search,
    PlaceDetails,
    Directions,
    MapTiles,
    TrafficLayer,
    NavigationTraffic,
}

impl GoogleSettings {
    pub fn allows(&self, use_: GoogleUse) -> bool {
        self.enabled
            && match use_ {
                GoogleUse::Suggestions => self.suggestions,
                GoogleUse::Search => self.search,
                GoogleUse::PlaceDetails => self.place_details,
                GoogleUse::Directions => self.directions,
                GoogleUse::MapTiles => self.map_tiles,
                GoogleUse::TrafficLayer => self.traffic_layer,
                GoogleUse::NavigationTraffic => self.navigation_traffic,
            }
    }
}

/// The servers of the routers and geocoders. Every one can be replaced by the user's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Servers {
    pub osrm_car: String,
    pub osrm_bicycle: String,
    pub osrm_foot: String,
    /// The profile name in OSRM's URLs (`driving` on the servers of FOSSGIS).
    pub osrm_profile: String,
    pub valhalla: String,
    pub photon: String,
    pub nominatim: String,
    pub overpass: String,
}

impl Default for Servers {
    fn default() -> Servers {
        Servers {
            osrm_car: "https://routing.openstreetmap.de/routed-car".into(),
            osrm_bicycle: "https://routing.openstreetmap.de/routed-bike".into(),
            osrm_foot: "https://routing.openstreetmap.de/routed-foot".into(),
            osrm_profile: "driving".into(),
            valhalla: "https://valhalla1.openstreetmap.de".into(),
            photon: "https://photon.komoot.io".into(),
            nominatim: "https://nominatim.openstreetmap.org".into(),
            overpass: osm::Overpass::DEFAULT_SERVER.into(),
        }
    }
}

/// The side traffic drives on, for avoiding turns across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrivingSideSetting {
    /// By the region of the device (`DrivingSide::of_country`).
    #[default]
    Automatic,
    Left,
    Right,
}

/// An area the user chose to stay out of (a closure they know, a district).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvoidArea {
    pub id: String,
    pub name: String,
    pub area: Polygon,
    /// When it stops being avoided, in ms since the epoch (`None`: until removed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingSettings {
    pub mode: TravelMode,
    pub avoid: Avoid,
    pub optimize: Optimize,
    pub driving_side: DrivingSideSetting,
    pub avoid_areas: Vec<AvoidArea>,
    /// The routers tried in order when the choice is automatic; one that cannot answer (off, out
    /// of its area, unreachable) is skipped.
    pub order: Vec<RouterKind>,
    /// Myap Router first whenever a downloaded region holds the trip.
    pub prefer_offline: bool,
    pub alternatives: bool,
    /// Adds what a downloaded region knows (speed limits, level crossings, speed cameras, toll
    /// booths, the names of junctions) to the routes of the other routers, on the phone.
    pub annotate: bool,
}

impl Default for RoutingSettings {
    fn default() -> RoutingSettings {
        RoutingSettings {
            mode: TravelMode::Car,
            avoid: Avoid::default(),
            optimize: Optimize::default(),
            driving_side: DrivingSideSetting::default(),
            avoid_areas: Vec::new(),
            order: RouterKind::ALL.to_vec(),
            prefer_offline: false,
            alternatives: true,
            annotate: true,
        }
    }
}

impl RoutingSettings {
    /// The side traffic drives on, by the setting and the region of the device.
    pub fn side(&self, region: &str) -> DrivingSide {
        match self.driving_side {
            DrivingSideSetting::Automatic => DrivingSide::of_country(region),
            DrivingSideSetting::Left => DrivingSide::Left,
            DrivingSideSetting::Right => DrivingSide::Right,
        }
    }

    /// The areas avoided at `now_ms`.
    pub fn areas_at(&self, now_ms: i64) -> Vec<Polygon> {
        self.avoid_areas.iter().filter(|a| a.until_ms.is_none_or(|u| now_ms < u)).map(|a| a.area.clone()).collect()
    }
}

/// Live traffic for Myap Router, from HERE with the user's own key. Off while the key is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrafficSettings {
    /// The key of the user's HERE account (kept on the device only, in the settings).
    pub here_api_key: String,
    /// How often the traffic is fetched again during guidance, in minutes.
    pub refresh_minutes: u32,
}

impl Default for TrafficSettings {
    fn default() -> TrafficSettings {
        TrafficSettings { here_api_key: String::new(), refresh_minutes: 5 }
    }
}

impl TrafficSettings {
    pub fn enabled(&self) -> bool {
        !self.here_api_key.trim().is_empty()
    }
}

/// What is looked up about places, and how the sources are combined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaceSettings {
    /// Looks up the details of a place in OpenStreetMap (opening hours, phone, website,
    /// wheelchair access) when it is opened: sends the place to Nominatim.
    pub details: bool,
    /// Combines OpenStreetMap and Google, so that each fills what the other lacks: a search of
    /// Google is also searched in OpenStreetMap, and a place of either is looked for in the other
    /// (in Google only while *Place details* of Google is allowed).
    pub combine: bool,
}

impl Default for PlaceSettings {
    fn default() -> PlaceSettings {
        PlaceSettings { details: true, combine: true }
    }
}

/// When the map is drawn dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NightMode {
    /// As the screens of the system are.
    #[default]
    System,
    /// From sunset to sunrise where the device is (civil twilight), as car navigation does.
    Sun,
    Always,
    Never,
}

/// The guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuidanceSettings {
    /// Tell level crossings, speed cameras, steep climbs and incidents ahead.
    pub alerts: bool,
    /// Show the speed limit and warn when it is exceeded.
    pub speed_limit: bool,
    /// Look for faster routes during guidance (with live traffic) and offer them.
    pub faster_routes: bool,
    /// Take a faster route at once, instead of offering it.
    pub accept_faster_routes: bool,
    /// The least time a faster route must save to be offered, in minutes.
    pub faster_route_minutes: u32,
    pub night_mode: NightMode,
    /// The guidance ends by itself this many minutes after arriving (0: only when the traveller
    /// ends it), so that the location and the service do not go on in a pocket.
    pub end_after_arrival_minutes: u32,
}

impl Default for GuidanceSettings {
    fn default() -> GuidanceSettings {
        GuidanceSettings {
            alerts: true,
            speed_limit: true,
            faster_routes: true,
            accept_faster_routes: false,
            faster_route_minutes: 3,
            night_mode: NightMode::System,
            end_after_arrival_minutes: 2,
        }
    }
}

/// Elevations for the regions of Myap Router (for avoiding hills and the times of walking and
/// cycling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElevationSource {
    /// None: regions are built without elevations.
    Off,
    /// The terrain tiles of AWS Open Data (Terrarium), worldwide.
    Terrarium,
    /// The elevation tiles of the Geospatial Information Authority of Japan (国土地理院), Japan only.
    Gsi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub language: Language,
    pub units: Units,
    pub map: MapSource,
    /// Spoken guidance.
    pub voice: bool,
    pub routing: RoutingSettings,
    pub places: PlaceSettings,
    pub google: GoogleSettings,
    pub servers: Servers,
    pub traffic: TrafficSettings,
    pub guidance: GuidanceSettings,
    /// Where the elevations of new regions come from.
    pub elevation: ElevationSource,
    /// Whether the first start has asked about Google.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            version: Self::VERSION,
            language: Language::System,
            units: Units::Automatic,
            map: MapSource::OpenFreeMap,
            voice: true,
            routing: RoutingSettings::default(),
            places: PlaceSettings::default(),
            google: GoogleSettings::default(),
            servers: Servers::default(),
            traffic: TrafficSettings::default(),
            guidance: GuidanceSettings::default(),
            elevation: ElevationSource::Terrarium,
            onboarded: false,
        }
    }
}

impl Settings {
    pub const VERSION: u32 = 1;

    /// The settings made consistent: every router once in the order, no Google map without the
    /// Google setting.
    pub fn normalized(mut self) -> Settings {
        let mut order = Vec::new();
        for kind in self.routing.order.iter().chain(RouterKind::ALL.iter()) {
            if !order.contains(kind) {
                order.push(*kind);
            }
        }
        self.routing.order = order;
        self.routing.avoid_areas.truncate(routing::RouteRequest::MAX_AVOID_AREAS);
        self.routing.avoid_areas.retain(|a| a.area.is_valid());
        self.traffic.refresh_minutes = self.traffic.refresh_minutes.clamp(1, 60);
        self.guidance.faster_route_minutes = self.guidance.faster_route_minutes.clamp(1, 60);
        self.guidance.end_after_arrival_minutes = self.guidance.end_after_arrival_minutes.min(60);
        self.version = Self::VERSION;
        self
    }

    /// The map source in use: a Google map only while Google's tiles are allowed.
    pub fn effective_map(&self) -> MapSource {
        match self.map {
            MapSource::GoogleRoadmap | MapSource::GoogleSatellite | MapSource::GoogleHybrid | MapSource::GoogleTerrain
                if !self.google.allows(GoogleUse::MapTiles) =>
            {
                MapSource::OpenFreeMap
            }
            other => other,
        }
    }
}
