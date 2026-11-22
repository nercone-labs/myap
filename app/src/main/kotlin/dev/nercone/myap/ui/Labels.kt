package dev.nercone.myap.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.DirectionsBike
import androidx.compose.material.icons.automirrored.filled.DirectionsWalk
import androidx.compose.material.icons.filled.TurnLeft
import androidx.compose.material.icons.filled.TurnRight
import androidx.compose.material.icons.filled.TurnSharpLeft
import androidx.compose.material.icons.filled.TurnSharpRight
import androidx.compose.material.icons.filled.TurnSlightLeft
import androidx.compose.material.icons.filled.TurnSlightRight
import androidx.compose.material.icons.filled.DirectionsBoat
import androidx.compose.material.icons.filled.DirectionsBus
import androidx.compose.material.icons.filled.DirectionsCar
import androidx.compose.material.icons.filled.Flag
import androidx.compose.material.icons.filled.ForkLeft
import androidx.compose.material.icons.filled.ForkRight
import androidx.compose.material.icons.filled.Merge
import androidx.compose.material.icons.filled.Navigation
import androidx.compose.material.icons.filled.RoundaboutLeft
import androidx.compose.material.icons.filled.RoundaboutRight
import androidx.compose.material.icons.filled.Straight
import androidx.compose.material.icons.filled.UTurnLeft
import androidx.compose.material.icons.filled.UTurnRight
import androidx.compose.material.icons.filled.Train
import androidx.compose.material.icons.filled.PhotoCamera
import androidx.compose.material.icons.filled.PanTool
import androidx.compose.material.icons.filled.Toll
import androidx.compose.material.icons.filled.Landscape
import androidx.compose.material.icons.filled.Warning
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import dev.nercone.myap.R
import dev.nercone.myap.core.DrivingSide
import dev.nercone.myap.core.DrivingSideSetting
import dev.nercone.myap.core.ElevationSource
import dev.nercone.myap.core.GoogleUse
import dev.nercone.myap.core.NightMode
import dev.nercone.myap.core.Optimize
import dev.nercone.myap.core.Language
import dev.nercone.myap.core.MapSource
import dev.nercone.myap.core.RegionStatus
import dev.nercone.myap.core.PlaceField
import dev.nercone.myap.core.PlaceSource
import dev.nercone.myap.core.Wheelchair
import dev.nercone.myap.core.RouterKind
import dev.nercone.myap.core.TravelMode
import dev.nercone.myap.core.Units

/** The names and icons of the values of the core, for the screens. */
object Labels {
    fun source(source: PlaceSource): String = when (source) {
        PlaceSource.OPEN_STREET_MAP -> "OpenStreetMap"
        PlaceSource.GOOGLE -> "Google"
        PlaceSource.COORDINATE -> "—"
    }

    @Composable
    fun field(field: PlaceField): String = stringResource(
        when (field) {
            PlaceField.ADDRESS -> R.string.field_address
            PlaceField.CATEGORY -> R.string.field_category
            PlaceField.RATING -> R.string.field_rating
            PlaceField.PHONE -> R.string.field_phone
            PlaceField.WEBSITE -> R.string.field_website
            PlaceField.HOURS -> R.string.field_hours
            PlaceField.STATUS -> R.string.field_status
            PlaceField.WHEELCHAIR -> R.string.field_wheelchair
            PlaceField.BOUNDS -> R.string.field_bounds
        },
    )

    @Composable
    fun wheelchair(value: Wheelchair): String = stringResource(
        when (value) {
            Wheelchair.YES -> R.string.wheelchair_yes
            Wheelchair.LIMITED -> R.string.wheelchair_limited
            Wheelchair.NO -> R.string.wheelchair_no
        },
    )

    fun router(kind: RouterKind): String = when (kind) {
        RouterKind.MYAP -> "Myap Router"
        RouterKind.OSRM -> "OSRM"
        RouterKind.VALHALLA -> "Valhalla"
        RouterKind.GOOGLE -> "Google"
    }

    @Composable
    fun routerHelp(kind: RouterKind): String = stringResource(
        when (kind) {
            RouterKind.MYAP -> R.string.router_myap_help
            RouterKind.OSRM -> R.string.router_osrm_help
            RouterKind.VALHALLA -> R.string.router_valhalla_help
            RouterKind.GOOGLE -> R.string.router_google_help
        },
    )

    @Composable
    fun mode(mode: TravelMode): String = stringResource(
        when (mode) {
            TravelMode.CAR -> R.string.mode_car
            TravelMode.BICYCLE -> R.string.mode_bicycle
            TravelMode.FOOT -> R.string.mode_foot
            TravelMode.TRANSIT -> R.string.mode_transit
        },
    )

    fun modeIcon(mode: TravelMode): ImageVector = when (mode) {
        TravelMode.CAR -> Icons.Default.DirectionsCar
        TravelMode.BICYCLE -> Icons.AutoMirrored.Filled.DirectionsBike
        TravelMode.FOOT -> Icons.AutoMirrored.Filled.DirectionsWalk
        TravelMode.TRANSIT -> Icons.Default.DirectionsBus
    }

    @Composable
    fun language(language: Language): String = stringResource(
        when (language) {
            Language.SYSTEM -> R.string.language_system
            Language.JAPANESE -> R.string.language_japanese
            Language.ENGLISH -> R.string.language_english
        },
    )

    @Composable
    fun units(units: Units): String = stringResource(
        when (units) {
            Units.AUTOMATIC -> R.string.units_automatic
            Units.METRIC -> R.string.units_metric
            Units.IMPERIAL -> R.string.units_imperial
        },
    )

    @Composable
    fun map(source: MapSource): String = stringResource(
        when (source) {
            MapSource.OPEN_FREE_MAP -> R.string.map_openfreemap
            MapSource.OPEN_STREET_MAP_CARTO -> R.string.map_osm_carto
            MapSource.GOOGLE_ROADMAP -> R.string.map_google_roadmap
            MapSource.GOOGLE_SATELLITE -> R.string.map_google_satellite
            MapSource.GOOGLE_HYBRID -> R.string.map_google_hybrid
            MapSource.GOOGLE_TERRAIN -> R.string.map_google_terrain
        },
    )

    @Composable
    fun googleUse(use: GoogleUse): String = stringResource(
        when (use) {
            GoogleUse.SUGGESTIONS -> R.string.google_suggestions
            GoogleUse.SEARCH -> R.string.google_search
            GoogleUse.PLACE_DETAILS -> R.string.google_place_details
            GoogleUse.DIRECTIONS -> R.string.google_directions
            GoogleUse.MAP_TILES -> R.string.google_map_tiles
            GoogleUse.TRAFFIC_LAYER -> R.string.google_traffic
            GoogleUse.NAVIGATION_TRAFFIC -> R.string.google_navigation_traffic
        },
    )

    /** What a use of Google sends to Google: shown with each switch. */
    @Composable
    fun googleSends(use: GoogleUse): String = stringResource(
        when (use) {
            GoogleUse.SUGGESTIONS -> R.string.google_suggestions_sends
            GoogleUse.SEARCH -> R.string.google_search_sends
            GoogleUse.PLACE_DETAILS -> R.string.google_place_details_sends
            GoogleUse.DIRECTIONS -> R.string.google_directions_sends
            GoogleUse.MAP_TILES -> R.string.google_map_tiles_sends
            GoogleUse.TRAFFIC_LAYER -> R.string.google_traffic_sends
            GoogleUse.NAVIGATION_TRAFFIC -> R.string.google_navigation_traffic_sends
        },
    )

    @Composable
    fun regionStatus(status: RegionStatus): String = stringResource(
        when (status) {
            RegionStatus.DOWNLOADING -> R.string.region_downloading
            RegionStatus.BUILDING -> R.string.region_building
            RegionStatus.ELEVATING -> R.string.region_elevating
            RegionStatus.READY -> R.string.region_ready
            RegionStatus.FAILED -> R.string.region_failed
        },
    )

    /** The icon of a maneuver, by the name the core gives it (`myap::words::Words::icon`). */
    /** The icon of a maneuver; U-turns and roundabouts go round on the `side` traffic drives on. */
    fun maneuver(icon: String, side: DrivingSide = DrivingSide.RIGHT): ImageVector = when (icon) {
        "turn_left" -> Icons.Default.TurnLeft
        "turn_right" -> Icons.Default.TurnRight
        "turn_slight_left" -> Icons.Default.TurnSlightLeft
        "turn_slight_right" -> Icons.Default.TurnSlightRight
        "turn_sharp_left" -> Icons.Default.TurnSharpLeft
        "turn_sharp_right" -> Icons.Default.TurnSharpRight
        "turn_uturn" -> if (side == DrivingSide.LEFT) Icons.Default.UTurnRight else Icons.Default.UTurnLeft
        "fork_left" -> Icons.Default.ForkLeft
        "fork_right" -> Icons.Default.ForkRight
        "merge_left", "merge_right" -> Icons.Default.Merge
        "roundabout" -> if (side == DrivingSide.LEFT) Icons.Default.RoundaboutLeft else Icons.Default.RoundaboutRight
        "ferry" -> Icons.Default.DirectionsBoat
        "transit" -> Icons.Default.DirectionsBus
        "arrive" -> Icons.Default.Flag
        "depart" -> Icons.Default.Navigation
        else -> Icons.Default.Straight
    }

    /** The icon of an alert, by the name the core gives it (`myap::words::Words::alert_icon`). */
    fun alert(icon: String): ImageVector = when (icon) {
        "level_crossing" -> Icons.Default.Train
        "speed_camera" -> Icons.Default.PhotoCamera
        "stop" -> Icons.Default.PanTool
        "toll_booth" -> Icons.Default.Toll
        "steep" -> Icons.Default.Landscape
        else -> Icons.Default.Warning
    }

    @Composable
    fun optimize(optimize: Optimize): String = stringResource(
        when (optimize) {
            Optimize.FASTEST -> R.string.optimize_fastest
            Optimize.SHORTEST -> R.string.optimize_shortest
            Optimize.ECO -> R.string.optimize_eco
            Optimize.SIMPLE -> R.string.optimize_simple
        },
    )

    /** The name of an avoidance, by its key in `Avoid.fields`. */
    @Composable
    fun avoid(key: String): String = stringResource(
        when (key) {
            "tolls" -> R.string.avoid_tolls
            "highways" -> R.string.avoid_highways
            "ferries" -> R.string.avoid_ferries
            "unpaved" -> R.string.avoid_unpaved
            "narrow" -> R.string.avoid_narrow
            "steps" -> R.string.avoid_steps
            "hills" -> R.string.avoid_hills
            "level_crossings" -> R.string.avoid_level_crossings
            "crossing_turns" -> R.string.avoid_crossing_turns
            else -> R.string.avoid_unlit
        },
    )

    @Composable
    fun nightMode(mode: NightMode): String = stringResource(
        when (mode) {
            NightMode.SYSTEM -> R.string.night_system
            NightMode.SUN -> R.string.night_sun
            NightMode.ALWAYS -> R.string.night_always
            NightMode.NEVER -> R.string.night_never
        },
    )

    @Composable
    fun elevation(source: ElevationSource): String = stringResource(
        when (source) {
            ElevationSource.OFF -> R.string.elevation_off
            ElevationSource.TERRARIUM -> R.string.elevation_terrarium
            ElevationSource.GSI -> R.string.elevation_gsi
        },
    )

    @Composable
    fun drivingSide(side: DrivingSideSetting): String = stringResource(
        when (side) {
            DrivingSideSetting.AUTOMATIC -> R.string.driving_side_automatic
            DrivingSideSetting.LEFT -> R.string.driving_side_left
            DrivingSideSetting.RIGHT -> R.string.driving_side_right
        },
    )

    /** A size in bytes, as "12.3 MB". */
    fun size(bytes: Long): String = when {
        bytes >= 1_000_000_000 -> "%.1f GB".format(bytes / 1e9)
        bytes >= 1_000_000 -> "%.1f MB".format(bytes / 1e6)
        bytes >= 1_000 -> "%.0f kB".format(bytes / 1e3)
        else -> "$bytes B"
    }
}
