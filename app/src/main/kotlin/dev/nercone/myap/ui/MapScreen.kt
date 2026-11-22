package dev.nercone.myap.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.MyLocation
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Work
import androidx.compose.material3.BottomSheetScaffold
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.rememberBottomSheetScaffoldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.nercone.myap.R
import dev.nercone.myap.core.Bounds
import dev.nercone.myap.core.Coordinate
import dev.nercone.myap.core.Endpoint
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.Place
import dev.nercone.myap.core.RouterChoice
import dev.nercone.myap.core.SavedLabel
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.core.Suggestion
import dev.nercone.myap.service.Host
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The map screen, as map applications lay it out: the map under everything, a search bar floating
 * at the top, buttons at the side, and a sheet at the bottom with what is being looked at (the
 * places around, the results, a place, directions). During guidance the guidance takes the screen.
 */
object MapScreen {
    @OptIn(ExperimentalMaterial3Api::class)
    @Composable
    fun Content(snapshot: Snapshot, model: AppViewModel, onSettings: (SettingsScreen.Page?) -> Unit) {
        val context = LocalContext.current
        val controller = remember { MapPane.Controller() }
        var following by rememberSaveable { mutableStateOf(false) }
        var searching by rememberSaveable { mutableStateOf(false) }
        var overview by rememberSaveable { mutableStateOf(false) }
        var camera by rememberSaveable { mutableStateOf(false) }
        val navigating = snapshot.navigation != null
        LaunchedEffect(navigating) {
            following = navigating
            overview = false
            if (!navigating) camera = false
        }

        val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { granted ->
            if (granted.values.any { it }) {
                model.host.locationPermissionChanged()
                model.host.locate(Host.Locating.SCREEN, precise = navigating)
            }
        }
        fun permitted() = context.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
        fun askLocation() = permission.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
        // Asked before the location when guidance starts: a second request made while one is
        // open is answered at once with nothing, so the location is asked once this one is done.
        val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { if (!permitted()) askLocation() }

        val listener = remember(model) {
            object : MapPane.Listener {
                override fun onResult(index: Int) {
                    model.snapshot.value?.search?.results?.getOrNull(index)?.let { place -> model.input { now -> Input.Select(now, place) } }
                }

                override fun onAlternative(index: Int) {
                    model.input { now -> Input.ChooseRoute(now, index) }
                }

                override fun onLongPress(coordinate: Coordinate) {
                    if (model.snapshot.value?.navigation == null) model.input { now -> Input.Pin(now, coordinate) }
                }

                override fun onViewport(center: Coordinate, bounds: Bounds, zoom: Double) {
                    model.visibleBounds = bounds
                    model.input { now -> Input.Viewport(now, center, bounds, zoom) }
                }

                override fun onUserMoved() {
                    following = false
                }
            }
        }

        // "Start" on a place plans the trip, then starts the guidance once its route came: only
        // for the plan it was asked for (its destination), and never once those directions are
        // closed, so that guidance never starts later for directions asked without it.
        var startFor by rememberSaveable { mutableStateOf<String?>(null) }
        var startSeen by rememberSaveable { mutableStateOf(false) }
        fun plan(place: Place) {
            if (!permitted()) askLocation()
            val settings = snapshot.settings.routing
            model.input { now -> Input.Plan(now, Endpoint.MyLocation, Endpoint.Of(place), emptyList(), settings.mode, settings.avoid, RouterChoice.AUTOMATIC) }
        }
        val actions = Actions(
            directionsTo = { place ->
                startFor = null
                plan(place)
            },
            navigateTo = { place ->
                startFor = place.id
                startSeen = false
                plan(place)
            },
            search = { searching = true },
            start = {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                    notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                } else if (!permitted()) {
                    askLocation()
                }
                model.host.locate(Host.Locating.SCREEN, precise = true)
                model.input { now -> Input.StartNavigation(now) }
            },
        )

        val directions = snapshot.directions
        val destination = (directions?.endpoints?.lastOrNull() as? Endpoint.Of)?.place?.id
        LaunchedEffect(directions == null, destination, directions?.loading, directions?.routes?.size) {
            val target = startFor ?: return@LaunchedEffect
            when {
                // Closed after they were shown: not to be started any more.
                directions == null -> if (startSeen) startFor = null
                destination != target -> startFor = null
                else -> {
                    startSeen = true
                    if (!directions.loading) {
                        startFor = null
                        if (directions.routes.isNotEmpty()) actions.start()
                    }
                }
            }
        }

        val navigation = snapshot.navigation
        val sheet = rememberBottomSheetScaffoldState()
        // One map in one place for every mode, so that it is never made twice; guidance is drawn
        // over it, with no sheet.
        val peek = when {
            navigation != null -> 0.dp
            snapshot.directions != null -> 300.dp
            snapshot.selected != null -> 240.dp
            snapshot.search.results.isNotEmpty() -> 260.dp
            else -> 140.dp
        }
        val snackbars = remember { SnackbarHostState() }
        val scope = rememberCoroutineScope()
        val locating = stringResource(R.string.locating)
        LaunchedEffect(snapshot.message?.id) {
            val message = snapshot.message ?: return@LaunchedEffect
            if (model.shownMessage == message.id) return@LaunchedEffect
            model.shownMessage = message.id
            snackbars.showSnackbar(message.text)
        }
        BottomSheetScaffold(
            scaffoldState = sheet,
            sheetPeekHeight = if (navigation != null) 0.dp else peek + WindowInsetsBottom.height(),
            sheetDragHandle = if (navigation != null) null else ({ androidx.compose.material3.BottomSheetDefaults.DragHandle() }),
            sheetSwipeEnabled = navigation == null,
            snackbarHost = { SnackbarHost(snackbars) },
            sheetContent = {
                Column(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.navigationBars)) {
                    when {
                        navigation != null -> {}
                        snapshot.directions != null -> Sheets.Directions(snapshot, snapshot.directions, model, actions)
                        snapshot.selected != null -> Sheets.Place(snapshot, snapshot.selected, model, actions)
                        snapshot.search.results.isNotEmpty() || snapshot.search.loading || snapshot.search.error != null -> Sheets.Results(snapshot, model)
                        else -> Sheets.Explore(snapshot, model, actions)
                    }
                }
            },
        ) { padding ->
            Box(Modifier.fillMaxSize()) {
                val mapPadding = if (navigation != null) PaddingValues(top = 180.dp, bottom = 120.dp) else PaddingValues(top = 96.dp, bottom = padding.calculateBottomPadding())
                MapPane.Content(controller, snapshot, following && !overview, mapPadding, listener, Modifier.fillMaxSize())
                if (navigation != null) {
                    LaunchedEffect(overview) { if (overview) MapPane.overview(controller, snapshot) }
                    val guidance = Guidance.Actions(
                        recenter = {
                            overview = false
                            following = true
                        },
                        stop = { model.input { now -> Input.StopNavigation(now) } },
                        overview = { overview = !overview },
                        camera = { camera = true },
                        mute = { model.configure { it.copy(voice = !it.voice) } },
                        avoidAhead = { model.input { now -> Input.AvoidAhead(now) } },
                        acceptFaster = { model.input { now -> Input.AcceptFasterRoute(now) } },
                        dismissFaster = { model.input { now -> Input.DismissFasterRoute(now) } },
                    )
                    // The camera view covers the screen and has a banner of its own: the guidance under
                    // it is not drawn, nor read out twice.
                    if (camera) {
                        ArCamera.Content(navigation, onClose = { camera = false })
                    } else {
                        Guidance.Content(snapshot, navigation, following, overview, guidance)
                    }
                    return@Box
                }
                Column(Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(horizontal = 16.dp, vertical = 8.dp).widthIn(max = 720.dp).fillMaxWidth()) {
                    SearchBar(snapshot, onOpen = { searching = true }, onClear = { model.input { now -> Input.ClearSearch(now) } }, onSettings = { onSettings(null) })
                    if (snapshot.style.google) GoogleMark(Modifier.padding(top = 8.dp))
                }
                Column(
                    Modifier.align(Alignment.BottomEnd).padding(end = 16.dp, bottom = padding.calculateBottomPadding() + 16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    SmallFloatingActionButton(onClick = { onSettings(SettingsScreen.Page.MAP) }) { Icon(Icons.Default.Layers, stringResource(R.string.settings_map)) }
                    SmallFloatingActionButton(onClick = {
                        val location = snapshot.location
                        when {
                            !permitted() -> askLocation()
                            location != null -> MapPane.center(controller, location.coordinate)
                            else -> {
                                model.host.locate(Host.Locating.SCREEN, precise = false)
                                scope.launch { snackbars.showSnackbar(locating) }
                            }
                        }
                    }) { Icon(Icons.Default.MyLocation, stringResource(R.string.my_location)) }
                }
                // The attribution the licences of the map ask for, readable on light and dark maps.
                Text(
                    snapshot.style.attribution,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.align(Alignment.BottomStart).padding(start = 8.dp, bottom = padding.calculateBottomPadding() + 4.dp)
                        .background(MaterialTheme.colorScheme.surface.copy(alpha = 0.75f), RoundedCornerShape(6.dp)).padding(horizontal = 6.dp, vertical = 1.dp),
                )
            }
        }
        if (searching) {
            SearchPanel(snapshot, model, onClose = { searching = false }, onPlace = { place ->
                searching = false
                model.input { now -> Input.Select(now, place) }
            })
        }
    }

    /** What the sheets can start. */
    class Actions(val directionsTo: (Place) -> Unit, val navigateTo: (Place) -> Unit, val start: () -> Unit, val search: () -> Unit)

    /** The height of the navigation bar, which the sheet's peek adds to. */
    private object WindowInsetsBottom {
        @Composable
        fun height() = with(LocalDensity.current) { WindowInsets.navigationBars.getBottom(this).toDp() }
    }

    /** The search bar of the map: opens the search; shows the search done, to clear it. */
    @Composable
    fun SearchBar(snapshot: Snapshot, onOpen: () -> Unit, onClear: () -> Unit, onSettings: () -> Unit) {
        Surface(shape = CircleShape, shadowElevation = 4.dp, tonalElevation = 2.dp, modifier = Modifier.fillMaxWidth().height(56.dp).clickable(onClick = onOpen)) {
            Row(Modifier.padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Default.Search, null, Modifier.padding(horizontal = 8.dp))
                val query = snapshot.search.query
                Text(
                    query.ifEmpty { stringResource(R.string.search_hint) },
                    style = MaterialTheme.typography.bodyLarge,
                    color = if (query.isEmpty()) Widgets.secondary else MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (query.isNotEmpty() || snapshot.selected != null) {
                    IconButton(onClick = onClear) { Icon(Icons.Default.Close, stringResource(R.string.clear)) }
                }
                IconButton(onClick = onSettings) { Icon(Icons.Default.Settings, stringResource(R.string.settings)) }
            }
        }
    }

    /** Said on the map while it is Google's. */
    @Composable
    fun GoogleMark(modifier: Modifier = Modifier) {
        Surface(shape = CircleShape, color = MaterialTheme.colorScheme.tertiaryContainer, modifier = modifier) {
            Text(stringResource(R.string.google_map_in_use), style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp))
        }
    }

    /** The search while typing: the field, then the suggestions, the recent searches and the saved places. */
    @Composable
    fun SearchPanel(snapshot: Snapshot, model: AppViewModel, onClose: () -> Unit, onPlace: (Place) -> Unit) {
        var query by rememberSaveable { mutableStateOf(snapshot.search.query) }
        val focus = LocalFocusManager.current
        val requester = remember { FocusRequester() }
        LaunchedEffect(Unit) { requester.requestFocus() }
        // The suggestions are asked for after a pause in typing, not at every key.
        LaunchedEffect(query) {
            if (query.isBlank()) return@LaunchedEffect
            delay(300)
            model.input { now -> Input.Suggest(now, query) }
        }
        fun submit(text: String) {
            if (text.isBlank()) return
            focus.clearFocus()
            onClose()
            model.input { now -> Input.Search(now, text) }
        }
        androidx.activity.compose.BackHandler(onBack = onClose)
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.statusBarsPadding().imePadding()) {
                TextField(
                    value = query,
                    onValueChange = { query = it },
                    placeholder = { Text(stringResource(R.string.search_hint)) },
                    leadingIcon = { IconButton(onClick = onClose) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) } },
                    trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Default.Close, stringResource(R.string.clear)) } },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { submit(query) }),
                    colors = TextFieldDefaults.colors(focusedContainerColor = Color.Transparent, unfocusedContainerColor = Color.Transparent),
                    modifier = Modifier.fillMaxWidth().padding(8.dp).focusRequester(requester),
                )
                HorizontalDivider()
                LazyColumn(Modifier.fillMaxSize()) {
                    val suggestions = if (query.isBlank()) emptyList() else snapshot.search.suggestions.takeIf { snapshot.search.query == query }.orEmpty()
                    // By position: two saved places may have the same name and no address.
                    items(suggestions) { suggestion -> SuggestionRow(suggestion, onPlace = onPlace, onQuery = ::submit) }
                    if (query.isBlank()) {
                        val saved = snapshot.saved
                        if (saved.isNotEmpty()) item(key = "saved") { Widgets.Header(stringResource(R.string.saved_places)) }
                        items(saved, key = { "v" + it.place.id + it.label }) { s ->
                            Row(
                                place = s.place,
                                icon = when (s.label) {
                                    SavedLabel.HOME -> Icons.Default.Home
                                    SavedLabel.WORK -> Icons.Default.Work
                                    SavedLabel.FAVORITE -> Icons.Default.Star
                                },
                                onClick = { onPlace(s.place) },
                            )
                        }
                        if (snapshot.recentSearches.isNotEmpty()) item(key = "searches") { Widgets.Header(stringResource(R.string.recent_searches)) }
                        items(snapshot.recentSearches, key = { "q$it" }) { text ->
                            ListItem(leadingContent = { Icon(Icons.Default.History, null) }, modifier = Modifier.clickable { submit(text) }, colors = ListItemDefaults.colors(containerColor = Color.Transparent)) { Text(text) }
                        }
                        if (snapshot.recent.isNotEmpty()) item(key = "recent") { Widgets.Header(stringResource(R.string.recent_places)) }
                        items(snapshot.recent, key = { "r" + it.id }) { place -> Row(place, Icons.Default.History) { onPlace(place) } }
                    }
                }
            }
        }
    }

    @Composable
    fun SuggestionRow(suggestion: Suggestion, onPlace: (Place) -> Unit, onQuery: (String) -> Unit) {
        val place = suggestion.place
        ListItem(
            supportingContent = suggestion.secondary?.let { { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis) } },
            leadingContent = { Icon(if (place != null) Icons.Default.Place else Icons.Default.Search, null) },
            colors = ListItemDefaults.colors(containerColor = Color.Transparent),
            modifier = Modifier.clickable { if (place != null) onPlace(place) else onQuery(suggestion.text) },
        ) { Text(suggestion.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    }

    @Composable
    fun Row(place: Place, icon: androidx.compose.ui.graphics.vector.ImageVector, onClick: () -> Unit) {
        ListItem(
            supportingContent = place.address?.let { { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis) } },
            leadingContent = { Icon(icon, null, Modifier.size(24.dp)) },
            colors = ListItemDefaults.colors(containerColor = Color.Transparent),
            modifier = Modifier.clickable(onClick = onClick),
        ) { Text(place.name, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    }
}
