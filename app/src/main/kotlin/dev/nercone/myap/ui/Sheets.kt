package dev.nercone.myap.ui

import android.content.Intent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.height
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import dev.nercone.myap.core.Optimize
import kotlin.math.roundToInt
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.automirrored.filled.Accessible
import androidx.compose.material.icons.filled.Merge
import androidx.compose.material.icons.filled.Place
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.NotAccessible
import androidx.compose.material3.Surface
import dev.nercone.myap.core.PlaceField
import dev.nercone.myap.core.Wheelchair
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Directions
import androidx.compose.material.icons.filled.Block
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkBorder
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.Navigation
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Work
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.nercone.myap.R
import dev.nercone.myap.core.DirectionsView
import dev.nercone.myap.core.Endpoint
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.Place
import dev.nercone.myap.core.PlaceSource
import dev.nercone.myap.core.PlaceView
import dev.nercone.myap.core.RouteView
import dev.nercone.myap.core.RouterChoice
import dev.nercone.myap.core.RouterKind
import dev.nercone.myap.core.SavedLabel
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.core.TravelMode

/** The contents of the bottom sheet of the map. */
object Sheets {
    private val PADDING = Modifier.padding(horizontal = 16.dp)

    /** With nothing looked at: home, work, and the places looked at recently. */
    @Composable
    fun Explore(snapshot: Snapshot, model: AppViewModel, actions: MapScreen.Actions) {
        Column(PADDING.padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (label in listOf(SavedLabel.HOME, SavedLabel.WORK)) {
                    val saved = snapshot.saved.firstOrNull { it.label == label }
                    AssistChip(
                        onClick = { saved?.let { actions.directionsTo(it.place) } ?: actions.search() },
                        label = { Text(saved?.place?.name ?: stringResource(if (label == SavedLabel.HOME) R.string.home_unset else R.string.work_unset)) },
                        leadingIcon = { Icon(if (label == SavedLabel.HOME) Icons.Default.Home else Icons.Default.Work, null) },
                    )
                }
                for (favorite in snapshot.saved.filter { it.label == SavedLabel.FAVORITE }) {
                    AssistChip(onClick = { model.input { now -> Input.Select(now, favorite.place) } }, label = { Text(favorite.place.name) }, leadingIcon = { Icon(Icons.Default.Star, null) })
                }
            }
            if (snapshot.recent.isNotEmpty()) {
                Widgets.Header(stringResource(R.string.recent_places))
                snapshot.recent.take(5).forEach { place -> MapScreen.Row(place, Icons.Default.Schedule) { model.input { now -> Input.Select(now, place) } } }
            } else {
                Text(stringResource(R.string.explore_help), style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary)
            }
        }
    }

    /** The results of a search. */
    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun Results(snapshot: Snapshot, model: AppViewModel) {
        val search = snapshot.search
        Column(PADDING.padding(bottom = 16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(search.query, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                IconButton(onClick = { model.input { now -> Input.ClearSearch(now) } }) { Icon(Icons.Default.Close, stringResource(R.string.clear)) }
            }
            if (search.sources.isNotEmpty()) Text(stringResource(R.string.results_from, search.sources.joinToString(" · ")), style = MaterialTheme.typography.labelMedium, color = Widgets.secondary)
            if (search.loading) LoadingIndicator(Modifier.align(Alignment.CenterHorizontally).padding(16.dp))
            search.error?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(vertical = 8.dp)) }
            Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                search.results.forEachIndexed { index, place ->
                    ResultRow(place, search.distances.getOrNull(index)?.takeIf { it.isNotEmpty() }) { model.input { now -> Input.Select(now, place) } }
                }
            }
        }
    }

    /**
     * A result: its name, what it is and whether it is open, how far and where; marked when what
     * is shown of it was combined from OpenStreetMap and Google.
     */
    @Composable
    fun ResultRow(place: Place, distance: String?, onClick: () -> Unit) {
        Surface(onClick = onClick, shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
            Row(Modifier.padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Surface(shape = CircleShape, color = MaterialTheme.colorScheme.secondaryContainer, modifier = Modifier.size(40.dp)) {
                    Box(contentAlignment = Alignment.Center) { Icon(Icons.Default.Place, null, tint = MaterialTheme.colorScheme.onSecondaryContainer) }
                }
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(place.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Summary(place)
                    val where = listOfNotNull(distance, place.address).joinToString(" · ")
                    if (where.isNotEmpty()) Text(where, style = MaterialTheme.typography.bodySmall, color = Widgets.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (place.sources().size > 1) {
                    Surface(shape = RoundedCornerShape(8.dp), color = MaterialTheme.colorScheme.tertiaryContainer, modifier = Modifier.padding(start = 8.dp)) {
                        Icon(Icons.Default.Merge, stringResource(R.string.place_sources), Modifier.padding(4.dp).size(16.dp), tint = MaterialTheme.colorScheme.onTertiaryContainer)
                    }
                }
            }
        }
    }

    /** Rating, category and open status on one line. */
    @Composable
    fun Summary(place: Place) {
        val parts = listOfNotNull(
            place.rating?.let { r -> "★ %.1f".format(r) + (place.reviewCount?.let { " ($it)" } ?: "") },
            place.category,
            place.status,
        )
        if (parts.isNotEmpty()) Text(parts.joinToString(" · "), style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }

    /** A place: what it is, the ways to go there, and what is known of it. */
    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun Place(snapshot: Snapshot, view: PlaceView, model: AppViewModel, actions: MapScreen.Actions) {
        val place = view.place
        val context = LocalContext.current
        val uri = LocalUriHandler.current
        var saving by rememberSaveable { mutableStateOf(false) }
        Column(PADDING.padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.Top) {
                Column(Modifier.weight(1f)) {
                    Text(place.name, style = MaterialTheme.typography.headlineSmall)
                    Summary(place)
                    listOfNotNull(view.distance, place.address).takeIf { it.isNotEmpty() }?.let { Text(it.joinToString(" · "), style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary) }
                }
                IconButton(onClick = { model.input { now -> Input.Deselect(now) } }) { Icon(Icons.Default.Close, stringResource(R.string.close)) }
            }
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { actions.directionsTo(place) }) {
                    Icon(Icons.Default.Directions, null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.directions))
                }
                FilledTonalButton(onClick = { actions.navigateTo(place) }) {
                    Icon(Icons.Default.Navigation, null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.start))
                }
                OutlinedButton(onClick = { if (view.saved) model.input { now -> Input.Unsave(now, place.id) } else saving = true }) {
                    Icon(if (view.saved) Icons.Default.Bookmark else Icons.Default.BookmarkBorder, null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(if (view.saved) R.string.saved else R.string.save))
                }
                OutlinedButton(onClick = {
                    val c = place.coordinate
                    val text = listOfNotNull(place.name, place.address, "https://www.openstreetmap.org/?mlat=${c.latitude}&mlon=${c.longitude}#map=17/${c.latitude}/${c.longitude}").joinToString("\n")
                    context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null))
                }) {
                    Icon(Icons.Default.Share, null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.share))
                }
                // Routes stay out of 150 m round here (a closure the user knows), until removed.
                OutlinedButton(onClick = { model.input { now -> Input.AvoidAreaAt(now, place.name, place.coordinate, 150.0) } }) {
                    Icon(Icons.Default.Block, null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.avoid_area_here))
                }
            }
            if (view.loading) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    LoadingIndicator(Modifier.size(32.dp))
                    Text(stringResource(R.string.place_looking_up), style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary)
                }
            }
            val hasDetails = place.phone != null || place.website != null || place.hours.isNotEmpty() || place.wheelchair != null
            if (hasDetails) {
                Surface(color = MaterialTheme.colorScheme.surfaceContainerLow, shape = RoundedCornerShape(20.dp), modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(vertical = 4.dp)) {
                        place.phone?.let { phone -> Detail(Icons.Default.Phone, phone, stringResource(R.string.call)) { Widgets.open(uri, "tel:${phone.filter { c -> c.isDigit() || c == '+' }}") } }
                        place.website?.takeIf { it.startsWith("https://") || it.startsWith("http://") }?.let { site ->
                            Detail(Icons.Default.Language, site.removePrefix("https://").removePrefix("http://").trimEnd('/'), stringResource(R.string.website)) { Widgets.open(uri, site) }
                        }
                        place.wheelchair?.let { w ->
                            Detail(if (w == Wheelchair.NO) Icons.Default.NotAccessible else Icons.AutoMirrored.Filled.Accessible, Labels.wheelchair(w), null, onClick = null)
                        }
                        if (place.hours.isNotEmpty()) Hours(place)
                    }
                }
            }
            Sources(place)
        }
        if (saving) {
            Widgets.ChoiceDialog<SavedLabel?>(
                title = stringResource(R.string.save_as),
                description = null,
                options = SavedLabel.entries.toList(),
                selected = null,
                describe = { option -> option?.let { stringResource(label(it)) }.orEmpty() },
                help = null,
                onSelect = { label ->
                    saving = false
                    if (label != null) model.input { now -> Input.Save(now, place, label) }
                },
                onDismiss = { saving = false },
            )
        }
    }

    fun label(label: SavedLabel): Int = when (label) {
        SavedLabel.HOME -> R.string.label_home
        SavedLabel.WORK -> R.string.label_work
        SavedLabel.FAVORITE -> R.string.label_favorite
    }

    /** A line of what is known of a place; one that can be acted on (call, open) is in the colour of actions. */
    @Composable
    private fun Detail(icon: androidx.compose.ui.graphics.vector.ImageVector, text: String, action: String?, onClick: (() -> Unit)?) {
        val clickable = if (onClick != null) Modifier.clickable(onClickLabel = action, onClick = onClick) else Modifier
        Row(Modifier.fillMaxWidth().then(clickable).padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, null, Modifier.size(20.dp), tint = if (onClick != null) MaterialTheme.colorScheme.primary else Widgets.secondary)
            Spacer(Modifier.width(16.dp))
            Text(
                text,
                style = MaterialTheme.typography.bodyLarge,
                color = if (onClick != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }

    /** The opening hours: the first line, and all of them when opened. */
    @Composable
    private fun Hours(place: Place) {
        var open by rememberSaveable(place.id) { mutableStateOf(false) }
        Column(Modifier.fillMaxWidth().clickable { open = !open }.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Default.Schedule, null, Modifier.size(20.dp), tint = Widgets.secondary)
                Spacer(Modifier.width(16.dp))
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.hours), style = MaterialTheme.typography.titleSmall)
                    place.status?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary) }
                }
                Icon(if (open) Icons.Default.ExpandLess else Icons.Default.ExpandMore, null, tint = Widgets.secondary)
            }
            val lines = if (open) place.hours else place.hours.take(1)
            lines.forEach { Text(it, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(start = 36.dp, top = 2.dp)) }
        }
    }

    /** The fields of a place that are known. */
    private fun known(place: Place): List<PlaceField> = PlaceField.entries.filter { field ->
        when (field) {
            PlaceField.ADDRESS -> place.address != null
            PlaceField.CATEGORY -> place.category != null
            PlaceField.RATING -> place.rating != null
            PlaceField.PHONE -> place.phone != null
            PlaceField.WEBSITE -> place.website != null
            PlaceField.HOURS -> place.hours.isNotEmpty()
            PlaceField.STATUS -> place.status != null
            PlaceField.WHEELCHAIR -> place.wheelchair != null
            PlaceField.BOUNDS -> false
        }
    }

    /**
     * Where what is shown came from: the place's own source, and, when another filled some of
     * it, each source with the parts it gave. Required by the licences of the data, and so that
     * nobody takes the hours of one source for those of another.
     */
    @Composable
    private fun Sources(place: Place) {
        val sources = place.sources()
        if (sources.size <= 1) {
            Text(
                stringResource(
                    when (place.source) {
                        PlaceSource.GOOGLE -> R.string.place_from_google
                        PlaceSource.OPEN_STREET_MAP -> R.string.place_from_osm
                        PlaceSource.COORDINATE -> R.string.place_from_point
                    },
                ),
                style = MaterialTheme.typography.labelSmall,
                color = Widgets.secondary,
            )
            return
        }
        val fields = known(place)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(stringResource(R.string.place_sources), style = MaterialTheme.typography.labelMedium, color = Widgets.secondary)
            sources.forEach { source ->
                val parts = fields.filter { place.sourceOf(it) == source }.map { Labels.field(it) }
                if (parts.isNotEmpty() || source == place.source) {
                    val name = if (source == PlaceSource.OPEN_STREET_MAP) "© OpenStreetMap" else Labels.source(source)
                    Text(
                        if (parts.isEmpty()) name else stringResource(R.string.place_source_parts, name, parts.joinToString(", ")),
                        style = MaterialTheme.typography.labelSmall,
                        color = Widgets.secondary,
                    )
                }
            }
        }
    }

    /** Directions: the trip, the mode, the router, what to avoid, the routes, and the steps. */
    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    @Composable
    fun Directions(snapshot: Snapshot, d: DirectionsView, model: AppViewModel, actions: MapScreen.Actions) {
        fun replan(mode: TravelMode = d.mode, avoid: dev.nercone.myap.core.Avoid = d.avoid, router: RouterChoice = d.router) {
            if (d.endpoints.size < 2) return
            model.input { now -> Input.Plan(now, d.endpoints.first(), d.endpoints.last(), d.endpoints.subList(1, d.endpoints.size - 1), mode, avoid, router) }
        }
        var steps by rememberSaveable { mutableStateOf(false) }
        Column(PADDING.padding(bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(d.destination, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(stringResource(R.string.directions_from, d.origin), style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary)
                }
                IconButton(onClick = { model.input { now -> Input.CloseDirections(now) } }) { Icon(Icons.Default.Close, stringResource(R.string.close)) }
            }
            Widgets.Segments(listOf(TravelMode.CAR, TravelMode.BICYCLE, TravelMode.FOOT), d.mode, { Labels.mode(it) }, { replan(mode = it) })
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(selected = d.router == RouterChoice.AUTOMATIC, onClick = { replan(router = RouterChoice.AUTOMATIC) }, label = { Text(stringResource(R.string.router_automatic)) })
                for (kind in RouterKind.entries.filter { it in d.available || d.router.router == it }) {
                    FilterChip(selected = d.router.router == kind, onClick = { replan(router = RouterChoice.only(kind)) }, label = { Text(Labels.router(kind)) })
                }
            }
            // What to avoid, the ones that matter for the mode, and what to optimise for.
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val fields = d.avoid.fields().toMap()
                for (key in avoidances(d.mode)) {
                    val on = fields[key] == true
                    FilterChip(selected = on, onClick = { replan(avoid = d.avoid.with(key, !on)) }, label = { Text(Labels.avoid(key)) })
                }
            }
            Widgets.Segments(Optimize.entries.toList(), d.optimize, { Labels.optimize(it) }, { optimize ->
                model.configure { it.copy(routing = it.routing.copy(optimize = optimize)) }
                replan()
            })
            val areas = snapshot.settings.routing.avoidAreas
            if (areas.isNotEmpty()) {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    areas.forEach { area ->
                        AssistChip(
                            onClick = { model.input { now -> Input.RemoveAvoidArea(now, area.id) } },
                            label = { Text(stringResource(R.string.avoid_area_remove, area.name)) },
                            leadingIcon = { Icon(Icons.Default.Close, null, Modifier.size(18.dp)) },
                        )
                    }
                }
            }
            if (d.loading) LoadingIndicator(Modifier.align(Alignment.CenterHorizontally))
            d.error?.let { Widgets.Notice(Widgets.Tone.ERROR, it, null) }
            d.notice?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = Widgets.secondary) }
            d.routes.forEachIndexed { index, route -> RouteCard(route, index == d.selected) { model.input { now -> Input.ChooseRoute(now, index) } } }
            d.routes.getOrNull(d.selected)?.let { route ->
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = actions.start, modifier = Modifier.weight(1f)) {
                        Icon(Icons.Default.Navigation, null)
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.start))
                    }
                    OutlinedButton(onClick = { steps = !steps }) { Text(stringResource(if (steps) R.string.hide_steps else R.string.show_steps)) }
                }
                if (steps) {
                    Column(Modifier.heightIn(max = 480.dp).verticalScroll(rememberScrollState())) {
                        route.steps.forEach { step ->
                            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                                Icon(Labels.maneuver(step.icon), null, Modifier.size(28.dp))
                                Spacer(Modifier.width(16.dp))
                                Column(Modifier.weight(1f)) {
                                    Text(step.instruction, style = MaterialTheme.typography.bodyLarge)
                                    if (step.distance.isNotEmpty()) Text(step.distance, style = MaterialTheme.typography.bodySmall, color = Widgets.secondary)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    @Composable
    fun RouteCard(route: RouteView, selected: Boolean, onClick: () -> Unit) {
        val colors = MaterialTheme.colorScheme
        Card(
            onClick = onClick,
            colors = CardDefaults.cardColors(containerColor = if (selected) colors.primaryContainer else colors.surfaceContainerHigh),
            border = if (selected) BorderStroke(2.dp, colors.primary) else null,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(Modifier.padding(16.dp)) {
                Row(verticalAlignment = Alignment.Bottom) {
                    Text(route.duration, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, color = if (route.traffic && (route.delayMinutes ?: 0) >= 5) colors.error else colors.onSurface)
                    Spacer(Modifier.width(12.dp))
                    Text("${route.distance} · ${stringResource(R.string.arrive_at, route.arrival)}", style = MaterialTheme.typography.bodyMedium, color = Widgets.secondary)
                }
                if (route.summary.isNotEmpty()) Text(route.summary, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val facts = listOfNotNull(
                    Labels.router(route.router),
                    if (route.traffic) stringResource(R.string.with_traffic, route.delayMinutes ?: 0) else stringResource(R.string.without_traffic),
                    stringResource(R.string.has_tolls).takeIf { route.hasTolls },
                    stringResource(R.string.has_ferries).takeIf { route.hasFerries },
                )
                Text(facts.joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = Widgets.secondary)
                route.notes.forEach { Text(it, style = MaterialTheme.typography.labelMedium, color = colors.error) }
                val terrain = listOfNotNull(route.climb, route.descent).joinToString("  ")
                if (terrain.isNotEmpty()) Text(terrain, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 4.dp))
                if (route.profile.size >= 2) Profile(route.profile, Modifier.fillMaxWidth().height(56.dp).padding(top = 4.dp))
                if (route.alerts.isNotEmpty()) {
                    Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        route.alerts.forEach { alert ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(Labels.alert(alert.icon), alert.name, Modifier.size(18.dp), tint = Widgets.secondary)
                                Text(alert.distance, style = MaterialTheme.typography.labelMedium, color = Widgets.secondary)
                            }
                        }
                    }
                }
            }
        }
    }

    /** The avoidances that matter for a mode of travel, in the order shown. */
    fun avoidances(mode: TravelMode): List<String> = when (mode) {
        TravelMode.CAR -> listOf("tolls", "highways", "ferries", "unpaved", "narrow", "level_crossings", "crossing_turns")
        TravelMode.BICYCLE -> listOf("hills", "unpaved", "steps", "narrow", "unlit", "level_crossings", "ferries")
        else -> listOf("hills", "steps", "unlit", "unpaved", "level_crossings", "ferries")
    }

    /** The elevation along a route, as a filled line from its lowest to its highest point. */
    @Composable
    fun Profile(heights: List<Float>, modifier: Modifier = Modifier) {
        val color = MaterialTheme.colorScheme.primary
        val low = heights.min()
        val high = heights.max()
        val description = "${low.roundToInt()}–${high.roundToInt()} m"
        Canvas(modifier.semantics { contentDescription = description }) {
            val span = (high - low).coerceAtLeast(10f)
            val step = size.width / (heights.size - 1)
            val path = Path().apply {
                moveTo(0f, size.height)
                heights.forEachIndexed { i, h -> lineTo(i * step, size.height - (h - low) / span * size.height) }
                lineTo(size.width, size.height)
                close()
            }
            drawPath(path, color.copy(alpha = 0.25f))
            val top = Path().apply { heights.forEachIndexed { i, h -> if (i == 0) moveTo(0f, size.height - (h - low) / span * size.height) else lineTo(i * step, size.height - (h - low) / span * size.height) } }
            drawPath(top, color, style = Stroke(width = 2.dp.toPx()))
        }
    }
}
