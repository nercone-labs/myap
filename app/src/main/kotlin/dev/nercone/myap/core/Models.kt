package dev.nercone.myap.core

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonClassDiscriminator
import kotlinx.serialization.json.JsonElement

// The types of `myap::protocol` as the core writes them in JSON. Times are milliseconds since the
// epoch. The core and the application are always built together, but unknown keys are ignored all
// the same, so that a field added to the core never breaks the screens.

object CoreJson {
    val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = false
        classDiscriminator = "type"
    }
}

@Serializable
data class Coordinate(val latitude: Double, val longitude: Double)

@Serializable
data class Bounds(val south: Double, val west: Double, val north: Double, val east: Double)

@Serializable
enum class TravelMode {
    @SerialName("car") CAR,
    @SerialName("bicycle") BICYCLE,
    @SerialName("foot") FOOT,
    @SerialName("transit") TRANSIT,
}

@Serializable
data class Avoid(
    val tolls: Boolean = false,
    val highways: Boolean = false,
    val ferries: Boolean = false,
    val unpaved: Boolean = false,
    val narrow: Boolean = false,
    val steps: Boolean = false,
    val hills: Boolean = false,
    @SerialName("level_crossings") val levelCrossings: Boolean = false,
    @SerialName("crossing_turns") val crossingTurns: Boolean = false,
    val unlit: Boolean = false,
) {
    /** Each avoidance with its key, in the order of the core. */
    fun fields(): List<Pair<String, Boolean>> = listOf(
        "tolls" to tolls, "highways" to highways, "ferries" to ferries, "unpaved" to unpaved, "narrow" to narrow,
        "steps" to steps, "hills" to hills, "level_crossings" to levelCrossings, "crossing_turns" to crossingTurns, "unlit" to unlit,
    )

    fun with(key: String, on: Boolean): Avoid = when (key) {
        "tolls" -> copy(tolls = on)
        "highways" -> copy(highways = on)
        "ferries" -> copy(ferries = on)
        "unpaved" -> copy(unpaved = on)
        "narrow" -> copy(narrow = on)
        "steps" -> copy(steps = on)
        "hills" -> copy(hills = on)
        "level_crossings" -> copy(levelCrossings = on)
        "crossing_turns" -> copy(crossingTurns = on)
        "unlit" -> copy(unlit = on)
        else -> this
    }

    fun count(): Int = fields().count { it.second }
}

@Serializable
enum class Optimize {
    @SerialName("fastest") FASTEST,
    @SerialName("shortest") SHORTEST,
    @SerialName("eco") ECO,
    @SerialName("simple") SIMPLE,
}

@Serializable
data class Polygon(val ring: List<Coordinate>)

@Serializable
enum class PlaceSource {
    @SerialName("open_street_map") OPEN_STREET_MAP,
    @SerialName("google") GOOGLE,
    @SerialName("coordinate") COORDINATE,
}

@Serializable
enum class Wheelchair {
    @SerialName("yes") YES,
    @SerialName("limited") LIMITED,
    @SerialName("no") NO,
}

/** A part of a place that one source may know and another not (`geocoding::PlaceField`). */
@Serializable
enum class PlaceField {
    @SerialName("address") ADDRESS,
    @SerialName("category") CATEGORY,
    @SerialName("rating") RATING,
    @SerialName("phone") PHONE,
    @SerialName("website") WEBSITE,
    @SerialName("hours") HOURS,
    @SerialName("status") STATUS,
    @SerialName("wheelchair") WHEELCHAIR,
    @SerialName("bounds") BOUNDS,
}

/** A field of a place filled from another source than the place's own. */
@Serializable
data class Filled(val field: PlaceField, val source: PlaceSource)

@Serializable
data class Place(
    val id: String,
    val source: PlaceSource,
    /** What the place is called, with its branch. */
    val name: String,
    val coordinate: Coordinate,
    val address: String? = null,
    val category: String? = null,
    val rating: Double? = null,
    @SerialName("review_count") val reviewCount: Long? = null,
    val phone: String? = null,
    val website: String? = null,
    val hours: List<String> = emptyList(),
    val status: String? = null,
    val bounds: Bounds? = null,
    val wheelchair: Wheelchair? = null,
    /** The ids of the same place in other sources. */
    val links: List<String> = emptyList(),
    val filled: List<Filled> = emptyList(),
    /** The branch given apart from the name (OpenStreetMap's `branch` tag), already part of `name`. */
    val branch: String? = null,
) {
    /** The source `field` came from. */
    fun sourceOf(field: PlaceField): PlaceSource = filled.firstOrNull { it.field == field }?.source ?: source

    /** Every source something of the place came from, its own first. */
    fun sources(): List<PlaceSource> = (listOf(source) + filled.map { it.source }).distinct()
}

@Serializable
data class Suggestion(val text: String, val secondary: String? = null, val place: Place? = null)

@Serializable
data class Location(
    val coordinate: Coordinate,
    @SerialName("accuracy_m") val accuracyM: Double? = null,
    val bearing: Double? = null,
    @SerialName("speed_mps") val speedMps: Double? = null,
    @SerialName("time_ms") val timeMs: Long,
)

// ---- Settings ----

@Serializable
enum class Language {
    @SerialName("system") SYSTEM,
    @SerialName("japanese") JAPANESE,
    @SerialName("english") ENGLISH,
}

@Serializable
enum class Units {
    @SerialName("automatic") AUTOMATIC,
    @SerialName("metric") METRIC,
    @SerialName("imperial") IMPERIAL,
}

@Serializable
enum class MapSource {
    @SerialName("open_free_map") OPEN_FREE_MAP,
    @SerialName("open_street_map_carto") OPEN_STREET_MAP_CARTO,
    @SerialName("google_roadmap") GOOGLE_ROADMAP,
    @SerialName("google_satellite") GOOGLE_SATELLITE,
    @SerialName("google_hybrid") GOOGLE_HYBRID,
    @SerialName("google_terrain") GOOGLE_TERRAIN,
}

@Serializable
enum class RouterKind {
    @SerialName("myap") MYAP,
    @SerialName("osrm") OSRM,
    @SerialName("valhalla") VALHALLA,
    @SerialName("google") GOOGLE,
}

@Serializable
enum class GoogleUse {
    @SerialName("suggestions") SUGGESTIONS,
    @SerialName("search") SEARCH,
    @SerialName("place_details") PLACE_DETAILS,
    @SerialName("directions") DIRECTIONS,
    @SerialName("map_tiles") MAP_TILES,
    @SerialName("traffic_layer") TRAFFIC_LAYER,
    @SerialName("navigation_traffic") NAVIGATION_TRAFFIC,
}

@Serializable
data class GoogleSettings(
    val enabled: Boolean = false,
    val suggestions: Boolean = false,
    val search: Boolean = false,
    @SerialName("place_details") val placeDetails: Boolean = false,
    val directions: Boolean = false,
    @SerialName("map_tiles") val mapTiles: Boolean = false,
    @SerialName("traffic_layer") val trafficLayer: Boolean = false,
    @SerialName("navigation_traffic") val navigationTraffic: Boolean = false,
    @SerialName("keep_record") val keepRecord: Boolean = true,
) {
    fun allows(use: GoogleUse): Boolean = enabled && when (use) {
        GoogleUse.SUGGESTIONS -> suggestions
        GoogleUse.SEARCH -> search
        GoogleUse.PLACE_DETAILS -> placeDetails
        GoogleUse.DIRECTIONS -> directions
        GoogleUse.MAP_TILES -> mapTiles
        GoogleUse.TRAFFIC_LAYER -> trafficLayer
        GoogleUse.NAVIGATION_TRAFFIC -> navigationTraffic
    }

    fun with(use: GoogleUse, on: Boolean): GoogleSettings = when (use) {
        GoogleUse.SUGGESTIONS -> copy(suggestions = on)
        GoogleUse.SEARCH -> copy(search = on)
        GoogleUse.PLACE_DETAILS -> copy(placeDetails = on)
        GoogleUse.DIRECTIONS -> copy(directions = on)
        GoogleUse.MAP_TILES -> copy(mapTiles = on)
        GoogleUse.TRAFFIC_LAYER -> copy(trafficLayer = on)
        GoogleUse.NAVIGATION_TRAFFIC -> copy(navigationTraffic = on)
    }

    fun chosen(use: GoogleUse): Boolean = with(use, true) == this
}

@Serializable
data class Servers(
    @SerialName("osrm_car") val osrmCar: String = "",
    @SerialName("osrm_bicycle") val osrmBicycle: String = "",
    @SerialName("osrm_foot") val osrmFoot: String = "",
    @SerialName("osrm_profile") val osrmProfile: String = "driving",
    val valhalla: String = "",
    val photon: String = "",
    val nominatim: String = "",
    val overpass: String = "",
)

@Serializable
enum class DrivingSide {
    @SerialName("right") RIGHT,
    @SerialName("left") LEFT,
}

@Serializable
enum class DrivingSideSetting {
    @SerialName("automatic") AUTOMATIC,
    @SerialName("left") LEFT,
    @SerialName("right") RIGHT,
}

@Serializable
data class AvoidArea(val id: String, val name: String, val area: Polygon, @SerialName("until_ms") val untilMs: Long? = null)

@Serializable
data class RoutingSettings(
    val mode: TravelMode = TravelMode.CAR,
    val avoid: Avoid = Avoid(),
    val optimize: Optimize = Optimize.FASTEST,
    @SerialName("driving_side") val drivingSide: DrivingSideSetting = DrivingSideSetting.AUTOMATIC,
    @SerialName("avoid_areas") val avoidAreas: List<AvoidArea> = emptyList(),
    val order: List<RouterKind> = emptyList(),
    @SerialName("prefer_offline") val preferOffline: Boolean = false,
    val alternatives: Boolean = true,
    val annotate: Boolean = true,
)

@Serializable
data class PlaceSettings(val details: Boolean = true, val combine: Boolean = true)

@Serializable
data class TrafficSettings(@SerialName("here_api_key") val hereApiKey: String = "", @SerialName("refresh_minutes") val refreshMinutes: Int = 5)

@Serializable
enum class NightMode {
    @SerialName("system") SYSTEM,
    @SerialName("sun") SUN,
    @SerialName("always") ALWAYS,
    @SerialName("never") NEVER,
}

@Serializable
data class GuidanceSettings(
    val alerts: Boolean = true,
    @SerialName("speed_limit") val speedLimit: Boolean = true,
    @SerialName("faster_routes") val fasterRoutes: Boolean = true,
    @SerialName("accept_faster_routes") val acceptFasterRoutes: Boolean = false,
    @SerialName("faster_route_minutes") val fasterRouteMinutes: Int = 3,
    @SerialName("night_mode") val nightMode: NightMode = NightMode.SYSTEM,
    @SerialName("end_after_arrival_minutes") val endAfterArrivalMinutes: Int = 2,
)

@Serializable
enum class ElevationSource {
    @SerialName("off") OFF,
    @SerialName("terrarium") TERRARIUM,
    @SerialName("gsi") GSI,
}

@Serializable
data class Settings(
    val version: Int = 1,
    val language: Language = Language.SYSTEM,
    val units: Units = Units.AUTOMATIC,
    val map: MapSource = MapSource.OPEN_FREE_MAP,
    val voice: Boolean = true,
    val routing: RoutingSettings = RoutingSettings(),
    val places: PlaceSettings = PlaceSettings(),
    val google: GoogleSettings = GoogleSettings(),
    val servers: Servers = Servers(),
    val traffic: TrafficSettings = TrafficSettings(),
    val guidance: GuidanceSettings = GuidanceSettings(),
    val elevation: ElevationSource = ElevationSource.TERRARIUM,
    val onboarded: Boolean = false,
)

// ---- Inputs ----

/** Tagged by `kind`, as the core tags it (the inputs are tagged by `type`). */
@OptIn(ExperimentalSerializationApi::class)
@JsonClassDiscriminator("kind")
@Serializable
sealed interface Endpoint {
    @Serializable @SerialName("my_location") data object MyLocation : Endpoint
    @Serializable @SerialName("place") data class Of(val place: Place) : Endpoint
}

@Serializable
data class RouterChoice(val kind: String, val router: RouterKind? = null) {
    companion object {
        val AUTOMATIC = RouterChoice("automatic")
        fun only(router: RouterKind) = RouterChoice("only", router)
    }
}

@Serializable
data class HttpResponse(val status: Int, val body: String = "", val headers: List<List<String>> = emptyList(), val error: String? = null)

@Serializable
enum class SavedLabel {
    @SerialName("home") HOME,
    @SerialName("work") WORK,
    @SerialName("favorite") FAVORITE,
}

@Serializable
sealed interface Input {
    val now: Long

    @Serializable @SerialName("start")
    data class Start(override val now: Long, @SerialName("data_dir") val dataDir: String?, val locale: String, @SerialName("utc_offset_minutes") val utcOffsetMinutes: Int) : Input

    @Serializable @SerialName("locale")
    data class Locale(override val now: Long, val locale: String, @SerialName("utc_offset_minutes") val utcOffsetMinutes: Int) : Input

    @Serializable @SerialName("appearance")
    data class Appearance(override val now: Long, val dark: Boolean) : Input

    @Serializable @SerialName("configure")
    data class Configure(override val now: Long, val settings: Settings) : Input

    @Serializable @SerialName("fetched")
    data class Fetched(override val now: Long, val id: Long, val response: HttpResponse) : Input

    @Serializable @SerialName("graph_built")
    data class GraphBuilt(override val now: Long, val id: Long, val result: JsonElement) : Input

    @Serializable @SerialName("located")
    data class Located(override val now: Long, val location: Location) : Input

    @Serializable @SerialName("viewport")
    data class Viewport(override val now: Long, val center: Coordinate, val bounds: Bounds, val zoom: Double) : Input

    @Serializable @SerialName("suggest")
    data class Suggest(override val now: Long, val query: String) : Input

    @Serializable @SerialName("search")
    data class Search(override val now: Long, val query: String) : Input

    @Serializable @SerialName("clear_search")
    data class ClearSearch(override val now: Long) : Input

    @Serializable @SerialName("select")
    data class Select(override val now: Long, val place: Place) : Input

    @Serializable @SerialName("pin")
    data class Pin(override val now: Long, val coordinate: Coordinate) : Input

    @Serializable @SerialName("deselect")
    data class Deselect(override val now: Long) : Input

    @Serializable @SerialName("plan")
    data class Plan(
        override val now: Long,
        val origin: Endpoint,
        val destination: Endpoint,
        val stops: List<Endpoint>,
        val mode: TravelMode,
        val avoid: Avoid,
        val router: RouterChoice,
    ) : Input

    @Serializable @SerialName("choose_route")
    data class ChooseRoute(override val now: Long, val index: Int) : Input

    @Serializable @SerialName("close_directions")
    data class CloseDirections(override val now: Long) : Input

    @Serializable @SerialName("start_navigation")
    data class StartNavigation(override val now: Long) : Input

    @Serializable @SerialName("stop_navigation")
    data class StopNavigation(override val now: Long) : Input

    @Serializable @SerialName("save")
    data class Save(override val now: Long, val place: Place, val label: SavedLabel) : Input

    @Serializable @SerialName("unsave")
    data class Unsave(override val now: Long, val id: String) : Input

    @Serializable @SerialName("clear_history")
    data class ClearHistory(override val now: Long) : Input

    @Serializable @SerialName("clear_google_record")
    data class ClearGoogleRecord(override val now: Long) : Input

    @Serializable @SerialName("add_region_area")
    data class AddRegionArea(override val now: Long, val name: String, val bounds: Bounds) : Input

    @Serializable @SerialName("add_region_url")
    data class AddRegionUrl(override val now: Long, val name: String, val url: String) : Input

    @Serializable @SerialName("add_region_file")
    data class AddRegionFile(override val now: Long, val name: String, val path: String) : Input

    @Serializable @SerialName("remove_region")
    data class RemoveRegion(override val now: Long, val id: String) : Input

    @Serializable @SerialName("accept_faster_route")
    data class AcceptFasterRoute(override val now: Long) : Input

    @Serializable @SerialName("dismiss_faster_route")
    data class DismissFasterRoute(override val now: Long) : Input

    @Serializable @SerialName("avoid_area")
    data class AvoidAreaAt(override val now: Long, val name: String, val center: Coordinate, @SerialName("radius_m") val radiusM: Double, val hours: Int? = null) : Input

    @Serializable @SerialName("remove_avoid_area")
    data class RemoveAvoidArea(override val now: Long, val id: String) : Input

    @Serializable @SerialName("avoid_ahead")
    data class AvoidAhead(override val now: Long) : Input

    @Serializable @SerialName("tick")
    data class Tick(override val now: Long) : Input

    @Serializable @SerialName("snapshot")
    data class Snap(override val now: Long) : Input
}

// ---- Effects ----

@Serializable
data class HttpRequest(
    val method: String,
    val url: String,
    val headers: List<List<String>> = emptyList(),
    val body: String? = null,
    @SerialName("timeout_ms") val timeoutMs: Long = 20_000,
    @SerialName("save_to") val saveTo: String? = null,
)

@Serializable
enum class GraphFormat {
    @SerialName("pbf") PBF,
    @SerialName("overpass_json") OVERPASS_JSON,
    @SerialName("graph") GRAPH,
}

/** The elevations of a build, passed to the core as it gave them. */
@Serializable
data class GraphBuild(val input: String, val format: GraphFormat, val output: String, val elevation: JsonElement? = null)

@Serializable
enum class LogLevel {
    @SerialName("info") INFO,
    @SerialName("warning") WARNING,
    @SerialName("error") ERROR,
}

@Serializable
sealed interface Effect {
    @Serializable @SerialName("fetch") data class Fetch(val id: Long, val request: HttpRequest) : Effect
    @Serializable @SerialName("cancel") data class Cancel(val id: Long) : Effect
    @Serializable @SerialName("build_graph") data class BuildGraph(val id: Long, val input: String, val format: GraphFormat, val output: String, val elevation: JsonElement? = null) : Effect
    @Serializable @SerialName("speak") data class Speak(val text: String, val language: String) : Effect
    @Serializable @SerialName("wake") data class Wake(val at: Long) : Effect
    @Serializable @SerialName("navigating") data class Navigating(val active: Boolean) : Effect
    @Serializable @SerialName("log") data class Log(val level: LogLevel, val message: String) : Effect
}

// ---- Snapshot ----

@Serializable
enum class LaneIndication {
    @SerialName("sharp_left") SHARP_LEFT,
    @SerialName("left") LEFT,
    @SerialName("slight_left") SLIGHT_LEFT,
    @SerialName("through") THROUGH,
    @SerialName("slight_right") SLIGHT_RIGHT,
    @SerialName("right") RIGHT,
    @SerialName("sharp_right") SHARP_RIGHT,
    @SerialName("u_turn") U_TURN,
    @SerialName("merge_to_left") MERGE_TO_LEFT,
    @SerialName("merge_to_right") MERGE_TO_RIGHT,
    @SerialName("none") NONE,
}

@Serializable
data class Lane(val indications: List<LaneIndication> = emptyList(), val valid: Boolean = false, val preferred: Boolean = false)

@Serializable
data class StepView(val instruction: String, val distance: String, val icon: String, val road: String? = null, val lanes: List<Lane> = emptyList())

@Serializable
data class AlertView(val icon: String, val name: String, val distance: String, @SerialName("distance_m") val distanceM: Double = 0.0)

@Serializable
data class FasterRouteView(val text: String, @SerialName("saving_minutes") val savingMinutes: Int, val summary: String = "", val router: RouterKind)

@Serializable
data class LocalPoint(val east: Double, val north: Double, val up: Double = 0.0)

@Serializable
data class ArView(val points: List<LocalPoint> = emptyList(), val maneuver: Int? = null, @SerialName("eye_height_m") val eyeHeightM: Double = 1.4)

@Serializable
data class RouteView(
    val router: RouterKind,
    val duration: String,
    val distance: String,
    val arrival: String,
    val summary: String,
    val traffic: Boolean,
    @SerialName("delay_minutes") val delayMinutes: Int? = null,
    @SerialName("has_tolls") val hasTolls: Boolean,
    @SerialName("has_ferries") val hasFerries: Boolean,
    val notes: List<String> = emptyList(),
    val steps: List<StepView> = emptyList(),
    @SerialName("distance_m") val distanceM: Double,
    @SerialName("duration_s") val durationS: Double,
    val climb: String? = null,
    val descent: String? = null,
    val profile: List<Float> = emptyList(),
    val alerts: List<AlertView> = emptyList(),
)

@Serializable
data class DirectionsView(
    val endpoints: List<Endpoint> = emptyList(),
    val origin: String,
    val destination: String,
    val stops: List<String> = emptyList(),
    val mode: TravelMode,
    val avoid: Avoid,
    val optimize: Optimize = Optimize.FASTEST,
    val router: RouterChoice,
    val routes: List<RouteView> = emptyList(),
    val selected: Int = 0,
    val loading: Boolean,
    val error: String? = null,
    val notice: String? = null,
    val available: List<RouterKind> = emptyList(),
)

@Serializable
enum class NavigationState {
    @SerialName("navigating") NAVIGATING,
    @SerialName("rerouting") REROUTING,
    @SerialName("arrived") ARRIVED,
}

@Serializable
data class NavigationView(
    val state: NavigationState,
    val instruction: String,
    val icon: String,
    val road: String? = null,
    val distance: String,
    @SerialName("distance_m") val distanceM: Double,
    @SerialName("then_icon") val thenIcon: String? = null,
    @SerialName("remaining_distance") val remainingDistance: String,
    @SerialName("remaining_time") val remainingTime: String,
    val arrival: String,
    @SerialName("remaining_m") val remainingM: Double = 0.0,
    @SerialName("remaining_s") val remainingS: Double = 0.0,
    @SerialName("arrival_ms") val arrivalMs: Long = 0,
    @SerialName("driving_side") val drivingSide: DrivingSide = DrivingSide.RIGHT,
    val progress: Double,
    val router: RouterKind,
    val position: Coordinate? = null,
    val bearing: Double? = null,
    val mode: TravelMode = TravelMode.CAR,
    val lanes: List<Lane> = emptyList(),
    val junction: String? = null,
    val destinations: String? = null,
    @SerialName("exit_number") val exitNumber: String? = null,
    @SerialName("speed_limit") val speedLimit: Int? = null,
    val speed: Int? = null,
    val speeding: Boolean = false,
    val alerts: List<AlertView> = emptyList(),
    val upcoming: List<StepView> = emptyList(),
    val congestion: String? = null,
    @SerialName("delay_minutes") val delayMinutes: Int? = null,
    @SerialName("faster_route") val fasterRoute: FasterRouteView? = null,
    val estimated: Boolean = false,
    val zoom: Double = 17.0,
    val tilt: Double = 50.0,
    val ar: ArView = ArView(),
)

@Serializable
data class SearchView(
    val query: String = "",
    val suggestions: List<Suggestion> = emptyList(),
    val results: List<Place> = emptyList(),
    val loading: Boolean = false,
    val error: String? = null,
    val sources: List<String> = emptyList(),
    val distances: List<String> = emptyList(),
)

@Serializable
data class PlaceView(val place: Place, val saved: Boolean, val distance: String? = null, val loading: Boolean = false)

@Serializable
data class SavedPlace(val place: Place, val label: SavedLabel, @SerialName("saved_ms") val savedMs: Long)

@Serializable
data class RegionSource(val kind: String, val bounds: Bounds? = null, val url: String? = null)

@Serializable
enum class RegionStatus {
    @SerialName("downloading") DOWNLOADING,
    @SerialName("building") BUILDING,
    @SerialName("elevating") ELEVATING,
    @SerialName("ready") READY,
    @SerialName("failed") FAILED,
}

@Serializable
data class Region(
    val id: String,
    val name: String,
    val source: RegionSource,
    val status: RegionStatus,
    val bounds: Bounds? = null,
    @SerialName("size_bytes") val sizeBytes: Long = 0,
    @SerialName("built_ms") val builtMs: Long? = null,
    val error: String? = null,
    val elevations: ElevationSource? = null,
    val progress: Double? = null,
)

@Serializable
data class GoogleRecord(@SerialName("time_ms") val timeMs: Long, val purpose: GoogleUse, val sent: String)

@Serializable
data class GoogleCount(val purpose: GoogleUse, val count: Int)

@Serializable
data class GoogleView(val records: List<GoogleRecord> = emptyList(), val counts: List<GoogleCount> = emptyList(), @SerialName("last_error") val lastError: String? = null)

@Serializable
data class RasterOverlay(val id: String, val tiles: List<String>, @SerialName("max_zoom") val maxZoom: Int, val opacity: Double)

@Serializable
data class MapStyle(val url: String? = null, val json: String? = null, val overlays: List<RasterOverlay> = emptyList(), val attribution: String, val google: Boolean)

@Serializable
data class Camera(val id: Long, val bounds: Bounds? = null, val center: Coordinate? = null, val zoom: Double? = null)

@Serializable
data class Notice(val id: Long, val text: String)

@Serializable
data class Snapshot(
    val settings: Settings,
    val language: String,
    val imperial: Boolean,
    val location: Location? = null,
    val search: SearchView = SearchView(),
    val selected: PlaceView? = null,
    val directions: DirectionsView? = null,
    val navigation: NavigationView? = null,
    val saved: List<SavedPlace> = emptyList(),
    val recent: List<Place> = emptyList(),
    @SerialName("recent_searches") val recentSearches: List<String> = emptyList(),
    val regions: List<Region> = emptyList(),
    val google: GoogleView = GoogleView(),
    val style: MapStyle,
    val overlay: String,
    val camera: Camera? = null,
    val message: Notice? = null,
)
