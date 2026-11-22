package dev.nercone.myap.ui

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.Report
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.filled.Route
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeFlexibleTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.nercone.myap.BuildConfig
import dev.nercone.myap.R
import dev.nercone.myap.core.DrivingSideSetting
import dev.nercone.myap.core.ElevationSource
import dev.nercone.myap.core.GuidanceSettings
import dev.nercone.myap.core.NightMode
import dev.nercone.myap.core.Optimize
import androidx.compose.material.icons.filled.Block
import androidx.compose.material.icons.filled.Navigation
import androidx.compose.material.icons.filled.Traffic
import androidx.compose.material.icons.filled.Place
import dev.nercone.myap.core.GoogleUse
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.Language
import dev.nercone.myap.core.MapSource
import dev.nercone.myap.core.RegionStatus
import dev.nercone.myap.core.RouterKind
import dev.nercone.myap.core.Servers
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.core.TravelMode
import dev.nercone.myap.core.Units
import java.io.File
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.DateFormat
import java.util.Date
import android.provider.OpenableColumns
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.runtime.remember

/** The settings: a list of pages, each of one concern. */
object SettingsScreen {
    const val SOURCE = "https://github.com/nercone-labs/myap"

    enum class Page(val title: Int) {
        GENERAL(R.string.settings_general),
        MAP(R.string.settings_map),
        PLACES(R.string.settings_places),
        ROUTING(R.string.settings_routing),
        GUIDANCE(R.string.settings_guidance),
        TRAFFIC(R.string.settings_traffic),
        OFFLINE(R.string.settings_offline),
        GOOGLE(R.string.settings_google),
        SAVED(R.string.settings_saved),
        ABOUT(R.string.settings_about),
    }

    @OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun Content(snapshot: Snapshot, model: AppViewModel, page: Page?, onOpen: (Page) -> Unit, onBack: () -> Unit) {
        val behavior = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
        Scaffold(
            modifier = Modifier.fillMaxSize().nestedScroll(behavior.nestedScrollConnection),
            containerColor = MaterialTheme.colorScheme.surfaceContainer,
            topBar = {
                LargeFlexibleTopAppBar(
                    title = { Text(stringResource(page?.title ?: R.string.settings)) },
                    navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) } },
                    scrollBehavior = behavior,
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surfaceContainer, scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer),
                )
            },
        ) { padding ->
            Column(Modifier.padding(padding)) {
                Widgets.Screen {
                    when (page) {
                        null -> item(key = "pages") { Pages(snapshot, onOpen) }
                        Page.GENERAL -> item(key = "general") { General(snapshot, model) }
                        Page.MAP -> item(key = "map") { MapPage(snapshot, model, onOpen) }
                        Page.PLACES -> item(key = "places") { PlacesPage(snapshot, model, onOpen) }
                        Page.ROUTING -> item(key = "routing") { Routing(snapshot, model) }
                        Page.GUIDANCE -> item(key = "guidance") { GuidancePage(snapshot, model) }
                        Page.TRAFFIC -> item(key = "traffic") { TrafficPage(snapshot, model) }
                        Page.OFFLINE -> item(key = "offline") { Offline(snapshot, model) }
                        Page.GOOGLE -> item(key = "google") { Google(snapshot, model) }
                        Page.SAVED -> item(key = "saved") { Saved(snapshot, model) }
                        Page.ABOUT -> item(key = "about") { About() }
                    }
                }
            }
        }
    }

    @Composable
    fun Pages(snapshot: Snapshot, onOpen: (Page) -> Unit) {
        val s = snapshot.settings
        val google = if (!s.google.enabled) stringResource(R.string.off) else GoogleUse.entries.filter { s.google.allows(it) }.size.let { stringResource(R.string.google_uses_on, it) }
        val pages = listOf(
            Triple(Page.GENERAL, Icons.Default.Tune, "${Labels.language(s.language)} · ${Labels.units(s.units)}"),
            Triple(Page.MAP, Icons.Default.Map, Labels.map(s.map)),
            Triple(Page.PLACES, Icons.Default.Place, stringResource(if (s.places.combine) R.string.places_combine else R.string.places_details)),
            Triple(Page.ROUTING, Icons.Default.Route, s.routing.order.joinToString(" → ") { Labels.router(it) }),
            Triple(Page.GUIDANCE, Icons.Default.Navigation, Labels.nightMode(s.guidance.nightMode)),
            Triple(Page.TRAFFIC, Icons.Default.Traffic, stringResource(if (s.traffic.hereApiKey.isBlank()) R.string.traffic_off else R.string.traffic_on)),
            Triple(Page.OFFLINE, Icons.Default.Download, stringResource(R.string.regions_count, snapshot.regions.count { it.status == RegionStatus.READY || it.status == RegionStatus.ELEVATING })),
            Triple(Page.GOOGLE, Icons.Default.Shield, google),
            Triple(Page.SAVED, Icons.Default.Bookmark, stringResource(R.string.saved_count, snapshot.saved.size)),
            Triple(Page.ABOUT, Icons.Default.Info, stringResource(R.string.settings_about_summary)),
        )
        Widgets.Group {
            pages.forEachIndexed { index, (page, icon, summary) -> Widgets.Item(index, pages.size, stringResource(page.title), summary, icon, onClick = { onOpen(page) }) }
        }
    }

    @Composable
    fun General(snapshot: Snapshot, model: AppViewModel) {
        val s = snapshot.settings
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Section(null) {
                Widgets.ChoiceItem(0, 3, stringResource(R.string.language), Language.entries.toList(), s.language, { Labels.language(it) }, { v -> model.configure { it.copy(language = v) } })
                Widgets.ChoiceItem(1, 3, stringResource(R.string.units), Units.entries.toList(), s.units, { Labels.units(it) }, { v -> model.configure { it.copy(units = v) } })
                Widgets.SwitchItem(2, 3, stringResource(R.string.voice), stringResource(R.string.voice_help), s.voice) { v -> model.configure { it.copy(voice = v) } }
            }
        }
    }

    @Composable
    fun MapPage(snapshot: Snapshot, model: AppViewModel, onOpen: (Page) -> Unit) {
        val s = snapshot.settings
        val tiles = s.google.allows(GoogleUse.MAP_TILES)
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            val open = listOf(MapSource.OPEN_FREE_MAP, MapSource.OPEN_STREET_MAP_CARTO)
            Widgets.Section(stringResource(R.string.map_open), stringResource(R.string.map_open_help)) {
                open.forEachIndexed { i, source -> Widgets.RadioItem(i, open.size, Labels.map(source), null, s.map == source) { model.configure { it.copy(map = source) } } }
            }
            val google = listOf(MapSource.GOOGLE_ROADMAP, MapSource.GOOGLE_SATELLITE, MapSource.GOOGLE_HYBRID, MapSource.GOOGLE_TERRAIN)
            Widgets.Section(stringResource(R.string.map_google), stringResource(if (tiles) R.string.map_google_help else R.string.map_google_off)) {
                google.forEachIndexed { i, source -> Widgets.RadioItem(i, google.size, Labels.map(source), null, s.map == source, enabled = tiles) { model.configure { it.copy(map = source) } } }
            }
            if (!tiles) Widgets.Group { Widgets.Item(0, 1, stringResource(R.string.settings_google), stringResource(R.string.open_google_settings), Icons.Default.Shield, onClick = { onOpen(Page.GOOGLE) }) }
            Widgets.Section(null) {
                Widgets.SwitchItem(0, 1, stringResource(R.string.google_traffic), stringResource(R.string.google_traffic_sends), s.google.allows(GoogleUse.TRAFFIC_LAYER), enabled = s.google.enabled) { v ->
                    model.configure { it.copy(google = it.google.copy(trafficLayer = v)) }
                }
            }
        }
    }

    /** What is looked up about places, and OpenStreetMap and Google combined. */
    @Composable
    fun PlacesPage(snapshot: Snapshot, model: AppViewModel, onOpen: (Page) -> Unit) {
        val p = snapshot.settings.places
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Section(null, stringResource(R.string.places_help)) {
                Widgets.SwitchItem(0, 2, stringResource(R.string.places_details), stringResource(R.string.places_details_help), p.details) { v -> model.configure { it.copy(places = it.places.copy(details = v)) } }
                Widgets.SwitchItem(1, 2, stringResource(R.string.places_combine), stringResource(R.string.places_combine_help), p.combine) { v -> model.configure { it.copy(places = it.places.copy(combine = v)) } }
            }
            Widgets.Group { Widgets.Item(0, 1, stringResource(R.string.settings_google), stringResource(R.string.open_google_settings), Icons.Default.Shield, onClick = { onOpen(Page.GOOGLE) }) }
        }
    }

    @Composable
    fun Routing(snapshot: Snapshot, model: AppViewModel) {
        val s = snapshot.settings
        val r = s.routing
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Section(stringResource(R.string.routing_default_mode)) {
                Widgets.Segments(listOf(TravelMode.CAR, TravelMode.BICYCLE, TravelMode.FOOT), r.mode, { Labels.mode(it) }, { v -> model.configure { it.copy(routing = it.routing.copy(mode = v)) } }, Modifier.padding(horizontal = 4.dp))
            }
            Widgets.Section(stringResource(R.string.routing_order), stringResource(R.string.routing_order_help)) {
                r.order.forEachIndexed { index, kind ->
                    Widgets.Item(index, r.order.size, "${index + 1}. ${Labels.router(kind)}", Labels.routerHelp(kind), trailing = {
                        Row {
                            IconButton(enabled = index > 0, onClick = { model.configure { it.copy(routing = it.routing.copy(order = r.order.moved(index, index - 1))) } }) { Icon(Icons.Default.ArrowUpward, stringResource(R.string.move_up)) }
                            IconButton(enabled = index < r.order.size - 1, onClick = { model.configure { it.copy(routing = it.routing.copy(order = r.order.moved(index, index + 1))) } }) { Icon(Icons.Default.ArrowDownward, stringResource(R.string.move_down)) }
                        }
                    })
                }
            }
            Widgets.Section(stringResource(R.string.routing_optimize), stringResource(R.string.routing_optimize_help)) {
                Widgets.Segments(Optimize.entries.toList(), r.optimize, { Labels.optimize(it) }, { v -> model.configure { it.copy(routing = it.routing.copy(optimize = v)) } }, Modifier.padding(horizontal = 4.dp))
            }
            Widgets.Section(stringResource(R.string.routing_avoid), stringResource(R.string.routing_avoid_help)) {
                val keys = r.avoid.fields()
                keys.forEachIndexed { index, (key, on) ->
                    Widgets.SwitchItem(index, keys.size, Labels.avoid(key), null, on) { v -> model.configure { it.copy(routing = it.routing.copy(avoid = it.routing.avoid.with(key, v))) } }
                }
            }
            Widgets.Section(stringResource(R.string.avoid_areas), stringResource(R.string.avoid_areas_help)) {
                if (r.avoidAreas.isEmpty()) Widgets.Item(0, 1, stringResource(R.string.avoid_areas_none))
                r.avoidAreas.forEachIndexed { index, area ->
                    Widgets.Item(index, r.avoidAreas.size, area.name, null, Icons.Default.Block, trailing = {
                        IconButton(onClick = { model.input { now -> Input.RemoveAvoidArea(now, area.id) } }) { Icon(Icons.Default.Delete, stringResource(R.string.remove)) }
                    })
                }
            }
            Widgets.Section(null) {
                Widgets.ChoiceItem(0, 1, stringResource(R.string.driving_side), DrivingSideSetting.entries.toList(), r.drivingSide, { Labels.drivingSide(it) }, { v -> model.configure { it.copy(routing = it.routing.copy(drivingSide = v)) } }, supporting = stringResource(R.string.driving_side_help))
            }
            Widgets.Section(null) {
                Widgets.SwitchItem(0, 3, stringResource(R.string.prefer_offline), stringResource(R.string.prefer_offline_help), r.preferOffline) { v -> model.configure { it.copy(routing = it.routing.copy(preferOffline = v)) } }
                Widgets.SwitchItem(1, 3, stringResource(R.string.routing_annotate), stringResource(R.string.routing_annotate_help), r.annotate) { v -> model.configure { it.copy(routing = it.routing.copy(annotate = v)) } }
                Widgets.SwitchItem(2, 3, stringResource(R.string.alternatives), null, r.alternatives) { v -> model.configure { it.copy(routing = it.routing.copy(alternatives = v)) } }
            }
            Widgets.Section(stringResource(R.string.servers), stringResource(R.string.servers_help)) {
                val url: (String) -> Boolean = { it.isEmpty() || (it.startsWith("https://") && Uri.parse(it).host != null) }
                val fields: List<Triple<Int, (Servers) -> String, (Servers, String) -> Servers>> = listOf(
                    Triple(R.string.server_osrm_car, { it.osrmCar }, { sv, v -> sv.copy(osrmCar = v) }),
                    Triple(R.string.server_osrm_bicycle, { it.osrmBicycle }, { sv, v -> sv.copy(osrmBicycle = v) }),
                    Triple(R.string.server_osrm_foot, { it.osrmFoot }, { sv, v -> sv.copy(osrmFoot = v) }),
                    Triple(R.string.server_valhalla, { it.valhalla }, { sv, v -> sv.copy(valhalla = v) }),
                    Triple(R.string.server_photon, { it.photon }, { sv, v -> sv.copy(photon = v) }),
                    Triple(R.string.server_nominatim, { it.nominatim }, { sv, v -> sv.copy(nominatim = v) }),
                    Triple(R.string.server_overpass, { it.overpass }, { sv, v -> sv.copy(overpass = v) }),
                )
                fields.forEachIndexed { i, (title, get, set) ->
                    Widgets.TextItem(i, fields.size, stringResource(title), get(s.servers), url, { v -> model.configure { it.copy(servers = set(it.servers, v)) } }, keyboardType = KeyboardType.Uri)
                }
            }
        }
    }

    private fun <T> List<T>.moved(from: Int, to: Int): List<T> = toMutableList().apply { add(to, removeAt(from)) }

    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun GuidancePage(snapshot: Snapshot, model: AppViewModel) {
        val g = snapshot.settings.guidance
        fun set(change: (GuidanceSettings) -> GuidanceSettings) = model.configure { it.copy(guidance = change(it.guidance)) }
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Section(null) {
                Widgets.SwitchItem(0, 4, stringResource(R.string.guidance_alerts), stringResource(R.string.guidance_alerts_help), g.alerts) { v -> set { it.copy(alerts = v) } }
                Widgets.SwitchItem(1, 4, stringResource(R.string.guidance_speed_limit), stringResource(R.string.guidance_speed_limit_help), g.speedLimit) { v -> set { it.copy(speedLimit = v) } }
                Widgets.ChoiceItem(2, 4, stringResource(R.string.night_mode), NightMode.entries.toList(), g.nightMode, { Labels.nightMode(it) }, { v -> set { it.copy(nightMode = v) } })
                Widgets.ChoiceItem(
                    3, 4, stringResource(R.string.end_after_arrival), listOf(0, 1, 2, 5, 10), g.endAfterArrivalMinutes,
                    { if (it == 0) stringResource(R.string.end_after_arrival_never) else stringResource(R.string.minutes, it) },
                    { v -> set { it.copy(endAfterArrivalMinutes = v) } },
                    description = stringResource(R.string.end_after_arrival_help),
                )
            }
            Widgets.Section(stringResource(R.string.faster_routes), stringResource(R.string.faster_routes_help)) {
                Widgets.SwitchItem(0, 3, stringResource(R.string.faster_routes_look), null, g.fasterRoutes) { v -> set { it.copy(fasterRoutes = v) } }
                Widgets.SwitchItem(1, 3, stringResource(R.string.faster_routes_accept), stringResource(R.string.faster_routes_accept_help), g.acceptFasterRoutes, enabled = g.fasterRoutes) { v -> set { it.copy(acceptFasterRoutes = v) } }
                Widgets.ChoiceItem(2, 3, stringResource(R.string.faster_routes_minutes), listOf(1, 2, 3, 5, 10, 15), g.fasterRouteMinutes, { stringResource(R.string.minutes, it) }, { v -> set { it.copy(fasterRouteMinutes = v) } }, enabled = g.fasterRoutes)
            }
        }
    }

    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun TrafficPage(snapshot: Snapshot, model: AppViewModel) {
        val t = snapshot.settings.traffic
        var editing by rememberSaveable { mutableStateOf(false) }
        if (editing) {
            TextDialog(stringResource(R.string.traffic_key), "", onDone = { v ->
                editing = false
                model.configure { it.copy(traffic = it.traffic.copy(hereApiKey = v.trim())) }
            }, onDismiss = { editing = false }, keyboardType = KeyboardType.Password)
        }
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Notice(Widgets.Tone.INFORMATION, stringResource(R.string.traffic_here), stringResource(R.string.traffic_here_help))
            Widgets.Section(null) {
                // The key is never shown whole; editing starts empty (empty turns traffic off).
                Widgets.Item(0, 2, stringResource(R.string.traffic_key), if (t.hereApiKey.isBlank()) stringResource(R.string.traffic_key_none) else "••••" + t.hereApiKey.takeLast(4), Icons.Default.Traffic, onClick = { editing = true })
                Widgets.ChoiceItem(1, 2, stringResource(R.string.traffic_refresh), listOf(2, 5, 10, 15), t.refreshMinutes, { stringResource(R.string.minutes, it) }, { v -> model.configure { it.copy(traffic = it.traffic.copy(refreshMinutes = v)) } })
            }
            Widgets.Help(stringResource(R.string.traffic_google_help))
        }
    }

    @Composable
    fun Offline(snapshot: Snapshot, model: AppViewModel) {
        val context = LocalContext.current
        var naming by rememberSaveable { mutableStateOf<String?>(null) }
        var url by rememberSaveable { mutableStateOf(false) }
        var removing by rememberSaveable { mutableStateOf<String?>(null) }
        val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri == null) return@rememberLauncherForActivityResult
            // The name the provider shows (the path of a document's URI is an opaque id).
            val shown = runCatching {
                context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
            }.getOrNull()
            val name = (shown ?: uri.lastPathSegment?.substringAfterLast('/'))?.substringBefore('.')?.takeIf { it.isNotBlank() } ?: "PBF"
            // Copied by the host, which outlives this page: an extract can be hundreds of megabytes.
            model.host.importRegion(uri, name)
        }
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Section(stringResource(R.string.regions), stringResource(R.string.regions_help)) {
                if (snapshot.regions.isEmpty()) Widgets.Item(0, 1, stringResource(R.string.regions_none))
                snapshot.regions.forEachIndexed { index, region ->
                    val progress = region.progress?.let { "${(it * 100).toInt()} %" }
                    val elevations = region.elevations?.takeIf { it != ElevationSource.OFF }?.let { stringResource(R.string.region_has_elevations) }
                    val detail = listOfNotNull(Labels.regionStatus(region.status), progress, elevations, region.sizeBytes.takeIf { it > 0 }?.let(Labels::size), region.error).joinToString(" · ")
                    Widgets.Item(index, snapshot.regions.size, region.name, detail, Icons.Default.Layers, trailing = {
                        IconButton(onClick = { removing = region.id }) { Icon(Icons.Default.Delete, stringResource(R.string.remove)) }
                    })
                }
            }
            Widgets.Section(stringResource(R.string.elevation), stringResource(R.string.elevation_help)) {
                Widgets.ChoiceItem(0, 1, stringResource(R.string.elevation_source), ElevationSource.entries.toList(), snapshot.settings.elevation, { Labels.elevation(it) }, { v -> model.configure { it.copy(elevation = v) } })
            }
            Widgets.Section(stringResource(R.string.regions_add)) {
                Widgets.Item(0, 3, stringResource(R.string.region_add_view), stringResource(R.string.region_add_view_help), onClick = { naming = "" }, enabled = model.visibleBounds != null)
                Widgets.Item(1, 3, stringResource(R.string.region_add_url), stringResource(R.string.region_add_url_help), onClick = { url = true })
                Widgets.Item(2, 3, stringResource(R.string.region_add_file), stringResource(R.string.region_add_file_help), onClick = { picker.launch(arrayOf("application/octet-stream", "application/x-protobuf", "*/*")) })
            }
        }
        naming?.let { current ->
            TextDialog(stringResource(R.string.region_name), current, onDone = { name ->
                naming = null
                val bounds = model.visibleBounds ?: return@TextDialog
                model.input { now -> Input.AddRegionArea(now, name.ifBlank { DateFormat.getDateInstance().format(Date()) }, bounds) }
            }, onDismiss = { naming = null })
        }
        if (url) {
            TextDialog(stringResource(R.string.region_add_url), "https://download.geofabrik.de/", onDone = { link ->
                url = false
                if (link.startsWith("https://") && link.toHttpUrlOrNull() != null) model.input { now -> Input.AddRegionUrl(now, link.substringAfterLast('/').substringBefore('.').ifBlank { "PBF" }, link) }
            }, onDismiss = { url = false }, keyboardType = KeyboardType.Uri)
        }
        removing?.let { id ->
            Widgets.Confirm(stringResource(R.string.remove_region), stringResource(R.string.remove_region_text), stringResource(R.string.remove), onConfirm = {
                removing = null
                model.input { now -> Input.RemoveRegion(now, id) }
            }, onDismiss = { removing = null })
        }
    }

    @Composable
    fun TextDialog(title: String, initial: String, onDone: (String) -> Unit, onDismiss: () -> Unit, keyboardType: KeyboardType = KeyboardType.Text) {
        // A secret is neither shown while typed nor kept in the saved state of the activity.
        val secret = keyboardType == KeyboardType.Password
        var text by if (secret) remember { mutableStateOf(initial) } else rememberSaveable { mutableStateOf(initial) }
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(title) },
            text = {
                OutlinedTextField(
                    text,
                    { text = it },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
                    visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = { TextButton(onClick = { onDone(text.trim()) }) { Text(stringResource(R.string.ok)) } },
            dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
        )
    }

    /**
     * Google, in one place: whether Myap uses it at all, each use with what it sends, and the
     * record of what was sent. Every use is off until the user turns it on.
     */
    @Composable
    fun Google(snapshot: Snapshot, model: AppViewModel) {
        val g = snapshot.settings.google
        var clearing by rememberSaveable { mutableStateOf(false) }
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Notice(Widgets.Tone.WARNING, stringResource(R.string.google_warning_title), stringResource(R.string.google_warning))
            Widgets.Section(null) {
                Widgets.SwitchItem(0, 1, stringResource(R.string.google_enabled), stringResource(if (g.enabled) R.string.google_enabled_on else R.string.google_enabled_off), g.enabled) { v ->
                    model.configure { it.copy(google = it.google.copy(enabled = v)) }
                }
            }
            val uses = GoogleUse.entries
            Widgets.Section(stringResource(R.string.google_uses), stringResource(R.string.google_uses_help)) {
                uses.forEachIndexed { index, use ->
                    Widgets.SwitchItem(index, uses.size, Labels.googleUse(use), Labels.googleSends(use), g.chosen(use), enabled = g.enabled) { v ->
                        model.configure { it.copy(google = it.google.with(use, v)) }
                    }
                }
            }
            snapshot.google.lastError?.let { Widgets.Notice(Widgets.Tone.ERROR, stringResource(R.string.google_last_error), it) }
            Widgets.Section(stringResource(R.string.google_record), stringResource(R.string.google_record_help)) {
                Widgets.SwitchItem(0, 2, stringResource(R.string.google_keep_record), null, g.keepRecord) { v -> model.configure { it.copy(google = it.google.copy(keepRecord = v)) } }
                Widgets.Item(1, 2, stringResource(R.string.google_clear), stringResource(R.string.google_clear_help), Icons.Default.Delete, onClick = { clearing = true })
            }
            if (snapshot.google.counts.isNotEmpty()) {
                Widgets.Section(stringResource(R.string.google_sent_counts)) {
                    snapshot.google.counts.forEachIndexed { i, c -> Widgets.Item(i, snapshot.google.counts.size, Labels.googleUse(c.purpose), stringResource(R.string.times, c.count)) }
                }
            }
            if (snapshot.google.records.isNotEmpty()) {
                val format = DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT)
                Widgets.Section(stringResource(R.string.google_sent)) {
                    val records = snapshot.google.records.take(50)
                    records.forEachIndexed { i, r -> Widgets.Item(i, records.size, r.sent, "${format.format(Date(r.timeMs))} · ${Labels.googleUse(r.purpose)}") }
                }
            }
        }
        if (clearing) {
            Widgets.Confirm(stringResource(R.string.google_clear), stringResource(R.string.google_clear_help), stringResource(R.string.clear), onConfirm = {
                clearing = false
                model.input { now -> Input.ClearGoogleRecord(now) }
            }, onDismiss = { clearing = false })
        }
    }

    @Composable
    fun Saved(snapshot: Snapshot, model: AppViewModel) {
        var clearing by rememberSaveable { mutableStateOf(false) }
        Column(verticalArrangement = Arrangement.spacedBy(Widgets.SPACING)) {
            Widgets.Section(stringResource(R.string.saved_places)) {
                if (snapshot.saved.isEmpty()) Widgets.Item(0, 1, stringResource(R.string.saved_none))
                snapshot.saved.forEachIndexed { i, s ->
                    Widgets.Item(i, snapshot.saved.size, s.place.name, "${stringResource(Sheets.label(s.label))} · ${s.place.address.orEmpty()}", trailing = {
                        IconButton(onClick = { model.input { now -> Input.Unsave(now, s.place.id) } }) { Icon(Icons.Default.Delete, stringResource(R.string.remove)) }
                    })
                }
            }
            Widgets.Section(null) { Widgets.Item(0, 1, stringResource(R.string.clear_history), stringResource(R.string.clear_history_help), Icons.Default.Delete, onClick = { clearing = true }) }
        }
        if (clearing) {
            Widgets.Confirm(stringResource(R.string.clear_history), stringResource(R.string.clear_history_help), stringResource(R.string.clear), onConfirm = {
                clearing = false
                model.input { now -> Input.ClearHistory(now) }
            }, onDismiss = { clearing = false })
        }
    }

    @Composable
    fun About() {
        val uri = LocalUriHandler.current
        Widgets.About(
            identity = stringResource(R.string.about_version, Widgets.version()),
            credits = stringResource(R.string.about_credits_text),
            creditsSummary = stringResource(R.string.about_credits_summary),
            source = SOURCE,
            items = listOf(
                Widgets.AboutItem("OpenStreetMap", stringResource(R.string.about_osm), Icons.Default.Map) { Widgets.open(uri, "https://www.openstreetmap.org/copyright") },
                Widgets.AboutItem("OpenFreeMap · OpenMapTiles", stringResource(R.string.about_openfreemap), Icons.Default.Layers) { Widgets.open(uri, "https://openfreemap.org") },
                Widgets.AboutItem(stringResource(R.string.about_fix_map), null, Icons.Default.Report) { Widgets.open(uri, "https://www.openstreetmap.org/fixthemap") },
            ),
            notice = stringResource(R.string.about_notice_title) to stringResource(R.string.about_notice),
        )
    }
}
