package dev.nercone.myap.car

import android.Manifest
import android.text.SpannableString
import android.text.Spanned
import androidx.car.app.CarContext
import androidx.car.app.Screen
import androidx.car.app.constraints.ConstraintManager
import androidx.car.app.model.Action
import androidx.car.app.model.ActionStrip
import androidx.car.app.model.CarColor
import androidx.car.app.model.CarIcon
import androidx.car.app.model.DateTimeWithZone
import androidx.car.app.model.Distance
import androidx.car.app.model.DistanceSpan
import androidx.car.app.model.DurationSpan
import androidx.car.app.model.ItemList
import androidx.car.app.model.MessageTemplate
import androidx.car.app.model.Row
import androidx.car.app.model.SearchTemplate
import androidx.car.app.model.Template
import androidx.car.app.navigation.model.Maneuver
import androidx.car.app.navigation.model.MessageInfo
import androidx.car.app.navigation.model.NavigationTemplate
import androidx.car.app.navigation.model.PlaceListNavigationTemplate
import androidx.car.app.navigation.model.RoutePreviewNavigationTemplate
import androidx.car.app.navigation.model.RoutingInfo
import androidx.car.app.navigation.model.Step
import androidx.car.app.navigation.model.TravelEstimate
import androidx.core.graphics.drawable.IconCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.coroutineScope
import androidx.lifecycle.repeatOnLifecycle
import dev.nercone.myap.R
import dev.nercone.myap.core.DrivingSide
import dev.nercone.myap.core.Endpoint
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.NavigationState
import dev.nercone.myap.core.NavigationView
import dev.nercone.myap.core.Place
import dev.nercone.myap.core.RouterChoice
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.core.TravelMode
import dev.nercone.myap.service.Host
import dev.nercone.myap.service.Locator
import java.util.TimeZone
import kotlin.math.roundToInt
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * A screen of the car drawn from the snapshot of the core: it is drawn again (`invalidate`) only
 * when what it shows (`key`) changes, so that the host's limits on new templates are never
 * reached by updates that change nothing on the screen.
 */
abstract class CoreScreen(carContext: CarContext, protected val host: Host) : Screen(carContext) {
    protected val snapshot: Snapshot? get() = host.snapshot.value

    /** What this screen shows of `snapshot`. */
    protected abstract fun key(snapshot: Snapshot?): Any?

    init {
        lifecycle.coroutineScope.launch {
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
                host.snapshot.map(::key).distinctUntilChanged().collect { invalidate() }
            }
        }
    }

    protected fun input(make: (Long) -> Input) = host.input(make(host.now()))

    protected fun icon(resource: Int): CarIcon = CarIcon.Builder(IconCompat.createWithResource(carContext, resource)).build()

    protected fun action(resource: Int, onClick: () -> Unit): Action = Action.Builder().setIcon(icon(resource)).setOnClickListener(onClick).build()

    /** How many rows a list may have in this car (6 where the host cannot say). */
    protected fun listLimit(type: Int = ConstraintManager.CONTENT_LIMIT_TYPE_LIST): Int = if (carContext.carAppApiLevel >= 2) {
        carContext.getCarService(ConstraintManager::class.java).getContentLimit(type)
    } else {
        6
    }

    /** Plans the trip from the traveller's position to `place`, and shows its routes. */
    protected fun directionsTo(place: Place) {
        val routing = snapshot?.settings?.routing
        val mode = routing?.mode?.takeIf { it != TravelMode.TRANSIT } ?: TravelMode.CAR
        input { now -> Input.Plan(now, Endpoint.MyLocation, Endpoint.Of(place), emptyList(), mode, routing?.avoid ?: dev.nercone.myap.core.Avoid(), RouterChoice.AUTOMATIC) }
        screenManager.push(RouteCarScreen(carContext, host))
    }

    /** A text whose first character the host replaces by `distance` in its own words and units. */
    protected fun withDistance(distance: Distance, text: String?): CharSequence {
        val s = SpannableString(if (text.isNullOrEmpty()) " " else "  · $text")
        s.setSpan(DistanceSpan.create(distance), 0, 1, Spanned.SPAN_INCLUSIVE_INCLUSIVE)
        return s
    }

    companion object {
        /** A distance in the units of the core's words (metric, or imperial), rounded as signs round it. */
        fun distance(metres: Double, imperial: Boolean): Distance {
            val m = metres.coerceAtLeast(0.0)
            return if (imperial) {
                val miles = m / 1_609.344
                when {
                    miles >= 10 -> Distance.create(miles.roundToInt().toDouble(), Distance.UNIT_MILES)
                    miles >= 0.1 -> Distance.create((miles * 10).roundToInt() / 10.0, Distance.UNIT_MILES_P1)
                    else -> Distance.create(((m * 3.28084) / 50).roundToInt() * 50.0, Distance.UNIT_FEET)
                }
            } else {
                when {
                    m >= 10_000 -> Distance.create((m / 1_000).roundToInt().toDouble(), Distance.UNIT_KILOMETERS)
                    m >= 1_000 -> Distance.create((m / 100).roundToInt() / 10.0, Distance.UNIT_KILOMETERS_P1)
                    m >= 100 -> Distance.create((m / 50).roundToInt() * 50.0, Distance.UNIT_METERS)
                    else -> Distance.create((m / 10).roundToInt() * 10.0, Distance.UNIT_METERS)
                }
            }
        }

        /**
         * The maneuver of the core's icon name (`Words::icon`), with the way U-turns and
         * roundabouts go on the side traffic drives on.
         */
        fun maneuver(context: CarContext, name: String, side: DrivingSide): Maneuver {
            val left = side == DrivingSide.LEFT
            val (type, drawable) = when (name) {
                "depart" -> Maneuver.TYPE_DEPART to R.drawable.ic_car_straight
                "arrive" -> Maneuver.TYPE_DESTINATION to R.drawable.ic_car_arrive
                "turn_straight", "turn_straight_on" -> Maneuver.TYPE_STRAIGHT to R.drawable.ic_car_straight
                "turn_slight_left" -> Maneuver.TYPE_TURN_SLIGHT_LEFT to R.drawable.ic_car_slight_left
                "turn_left" -> Maneuver.TYPE_TURN_NORMAL_LEFT to R.drawable.ic_car_turn_left
                "turn_sharp_left" -> Maneuver.TYPE_TURN_SHARP_LEFT to R.drawable.ic_car_sharp_left
                "turn_slight_right" -> Maneuver.TYPE_TURN_SLIGHT_RIGHT to R.drawable.ic_car_slight_right
                "turn_right" -> Maneuver.TYPE_TURN_NORMAL_RIGHT to R.drawable.ic_car_turn_right
                "turn_sharp_right" -> Maneuver.TYPE_TURN_SHARP_RIGHT to R.drawable.ic_car_sharp_right
                "turn_uturn" -> if (left) Maneuver.TYPE_U_TURN_RIGHT to R.drawable.ic_car_uturn_right else Maneuver.TYPE_U_TURN_LEFT to R.drawable.ic_car_uturn_left
                // Clockwise where traffic drives on the left.
                "roundabout" -> if (left) Maneuver.TYPE_ROUNDABOUT_ENTER_CW to R.drawable.ic_car_roundabout else Maneuver.TYPE_ROUNDABOUT_ENTER_CCW to R.drawable.ic_car_roundabout_ccw
                "merge_left" -> Maneuver.TYPE_MERGE_LEFT to R.drawable.ic_car_merge
                "merge_right" -> Maneuver.TYPE_MERGE_RIGHT to R.drawable.ic_car_merge
                "fork_left" -> Maneuver.TYPE_FORK_LEFT to R.drawable.ic_car_fork_left
                "fork_right" -> Maneuver.TYPE_FORK_RIGHT to R.drawable.ic_car_fork_right
                "ferry" -> Maneuver.TYPE_FERRY_BOAT to R.drawable.ic_car_ferry
                else -> Maneuver.TYPE_UNKNOWN to R.drawable.ic_car_straight
            }
            return Maneuver.Builder(type).setIcon(CarIcon.Builder(IconCompat.createWithResource(context, drawable)).build()).build()
        }

        /** The time and distance left, and the arrival, as the host shows them. */
        fun estimate(nav: NavigationView, imperial: Boolean): TravelEstimate =
            TravelEstimate.Builder(distance(nav.remainingM, imperial), DateTimeWithZone.create(nav.arrivalMs, TimeZone.getDefault()))
                .setRemainingTimeSeconds(nav.remainingS.coerceAtLeast(0.0).roundToInt().toLong())
                .setRemainingTimeColor(if ((nav.delayMinutes ?: 0) >= 5) CarColor.RED else CarColor.GREEN)
                .build()

        /** The step of the next maneuver. */
        fun step(context: CarContext, nav: NavigationView): Step {
            val builder = Step.Builder(nav.instruction).setManeuver(maneuver(context, nav.icon, nav.drivingSide))
            nav.road?.let(builder::setRoad)
            return builder.build()
        }
    }
}

/**
 * The map: browsing (search, saved places, the map's buttons), or the guidance (the next
 * maneuver, the one after it, the time and distance left). The guidance is the same as on the
 * phone: starting it on either shows it on both.
 */
class MapCarScreen(carContext: CarContext, host: Host, private val map: CarMap, private val guidance: CarGuidance) : CoreScreen(carContext, host) {
    override fun key(snapshot: Snapshot?): Any? {
        val nav = snapshot?.navigation
        return listOf(
            nav?.state, nav?.instruction, nav?.icon, nav?.road, nav?.distance, nav?.thenIcon, nav?.upcoming?.firstOrNull()?.instruction,
            nav?.remainingTime, nav?.remainingDistance, nav?.arrival, snapshot?.settings?.voice, Locator(carContext) {}.permitted(),
        )
    }

    override fun onGetTemplate(): Template {
        if (!Locator(carContext) {}.permitted()) return permission()
        val s = snapshot
        val nav = s?.navigation
        val imperial = s?.imperial ?: false
        val builder = NavigationTemplate.Builder()
        val actions = ActionStrip.Builder()
        if (nav == null) {
            actions.addAction(action(R.drawable.ic_car_search) { screenManager.push(SearchCarScreen(carContext, host)) })
            actions.addAction(action(R.drawable.ic_car_star) { screenManager.push(PlacesCarScreen(carContext, host, PlacesCarScreen.Kind.SAVED)) })
        } else {
            val voice = s.settings.voice
            actions.addAction(action(if (voice) R.drawable.ic_car_volume else R.drawable.ic_car_volume_off) { host.input(Input.Configure(host.now(), s.settings.copy(voice = !voice))) })
            actions.addAction(
                Action.Builder().setTitle(carContext.getString(R.string.end_navigation)).setOnClickListener { input { now -> Input.StopNavigation(now) } }.build(),
            )
        }
        builder.setActionStrip(actions.build())
        // The buttons of the map and panning came with the second level of the cars' API.
        if (carContext.carAppApiLevel >= 2) {
            builder.setMapActionStrip(
                ActionStrip.Builder()
                    .addAction(Action.PAN)
                    .addAction(action(R.drawable.ic_car_recenter) { map.recenter() })
                    .addAction(action(R.drawable.ic_car_zoom_in) { map.zoom(1.0) })
                    .addAction(action(R.drawable.ic_car_zoom_out) { map.zoom(-1.0) })
                    .build(),
            )
            builder.setPanModeListener { panning -> if (panning) map.panned() }
        }
        if (nav != null) {
            when (nav.state) {
                NavigationState.ARRIVED -> builder.setNavigationInfo(MessageInfo.Builder(nav.instruction).setImage(icon(R.drawable.ic_car_arrive)).build())
                NavigationState.REROUTING -> builder.setNavigationInfo(RoutingInfo.Builder().setLoading(true).build())
                NavigationState.NAVIGATING -> {
                    val routing = RoutingInfo.Builder().setCurrentStep(step(carContext, nav), distance(nav.distanceM, imperial))
                    // The maneuver after it, when it follows closely.
                    val then = nav.upcoming.firstOrNull()
                    if (nav.thenIcon != null && then != null) {
                        routing.setNextStep(Step.Builder(then.instruction).setManeuver(maneuver(carContext, then.icon, nav.drivingSide)).build())
                    }
                    builder.setNavigationInfo(routing.build())
                }
            }
            builder.setDestinationTravelEstimate(estimate(nav, imperial))
        }
        return builder.build()
    }

    /** The location is needed and not allowed: it is asked for on the phone. */
    private fun permission(): Template = MessageTemplate.Builder(carContext.getString(R.string.car_permission))
        .setHeaderAction(Action.APP_ICON)
        .addAction(
            Action.Builder().setTitle(carContext.getString(R.string.car_permission_allow)).setOnClickListener {
                carContext.requestPermissions(listOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)) { granted, _ ->
                    if (granted.isNotEmpty()) {
                        host.locationPermissionChanged()
                        guidance.locate()
                    }
                    invalidate()
                }
            }.build(),
        )
        .build()
}

/** Search: suggestions while typing, then the results. */
class SearchCarScreen(carContext: CarContext, host: Host) : CoreScreen(carContext, host) {
    private var text = ""
    private var typing: Job? = null

    override fun key(snapshot: Snapshot?): Any? = snapshot?.search?.let { listOf(it.query, it.suggestions, it.loading) }

    override fun onGetTemplate(): Template {
        val search = snapshot?.search
        val builder = SearchTemplate.Builder(object : SearchTemplate.SearchCallback {
            override fun onSearchTextChanged(searchText: String) {
                text = searchText
                // Asked after a pause in typing, not at every key.
                typing?.cancel()
                if (searchText.isBlank()) return
                typing = lifecycle.coroutineScope.launch {
                    delay(400)
                    input { now -> Input.Suggest(now, searchText) }
                }
            }

            override fun onSearchSubmitted(searchText: String) = submit(searchText)
        })
            .setHeaderAction(Action.BACK)
            .setSearchHint(carContext.getString(R.string.search_hint))
            .setShowKeyboardByDefault(true)
        if (text.isNotEmpty()) builder.setInitialSearchText(text)
        val suggestions = search?.suggestions.orEmpty().takeIf { search?.query == text && text.isNotBlank() }.orEmpty()
        if (search?.loading == true && suggestions.isEmpty() && text.isNotBlank()) {
            builder.setLoading(true)
        } else {
            val list = ItemList.Builder()
            for (suggestion in suggestions.take(listLimit())) {
                val row = Row.Builder().setTitle(suggestion.text)
                suggestion.secondary?.let(row::addText)
                val place = suggestion.place
                row.setOnClickListener { if (place != null) directionsTo(place) else submit(suggestion.text) }
                list.addItem(row.build())
            }
            builder.setItemList(list.build())
        }
        return builder.build()
    }

    private fun submit(query: String) {
        if (query.isBlank()) return
        typing?.cancel()
        input { now -> Input.Search(now, query) }
        screenManager.push(PlacesCarScreen(carContext, host, PlacesCarScreen.Kind.RESULTS))
    }
}

/** A list of places on the map: the results of the search, or the saved and recent places. */
class PlacesCarScreen(carContext: CarContext, host: Host, private val kind: Kind) : CoreScreen(carContext, host) {
    enum class Kind { RESULTS, SAVED }

    override fun key(snapshot: Snapshot?): Any? = when (kind) {
        Kind.RESULTS -> snapshot?.search?.let { listOf(it.query, it.results.map { p -> p.id }, it.loading, it.error, snapshot.selected?.place?.id, snapshot.location != null) }
        Kind.SAVED -> snapshot?.let { listOf(it.saved.map { s -> s.place.id }, it.recent.map { p -> p.id }, it.location != null) }
    }

    private fun places(s: Snapshot?): List<Place> = when (kind) {
        Kind.RESULTS -> s?.search?.results?.ifEmpty { listOfNotNull(s.selected?.place) }.orEmpty()
        Kind.SAVED -> s?.let { (it.saved.map { saved -> saved.place } + it.recent).distinctBy { p -> p.id } }.orEmpty()
    }

    override fun onGetTemplate(): Template {
        val s = snapshot
        val builder = PlaceListNavigationTemplate.Builder()
            .setTitle(if (kind == Kind.RESULTS) s?.search?.query.orEmpty().ifEmpty { carContext.getString(R.string.search_hint) } else carContext.getString(R.string.saved_places))
            .setHeaderAction(Action.BACK)
            .setActionStrip(ActionStrip.Builder().addAction(action(R.drawable.ic_car_search) { screenManager.push(SearchCarScreen(carContext, host)) }).build())
        if (kind == Kind.RESULTS && s?.search?.loading == true) return builder.setLoading(true).build()
        // Every row of this template must have its distance (the library refuses the list
        // otherwise), and the routes start from the position: without a fix, the screen waits.
        if (!Locator(carContext) {}.permitted()) {
            return MessageTemplate.Builder(carContext.getString(R.string.car_permission)).setHeaderAction(Action.BACK).build()
        }
        val here = s?.location?.coordinate
            ?: return MessageTemplate.Builder(carContext.getString(R.string.locating)).setHeaderAction(Action.BACK).setLoading(true).build()
        val list = ItemList.Builder().setNoItemsMessage(s?.search?.error ?: carContext.getString(R.string.car_no_places))
        for (place in places(s).take(listLimit())) {
            val row = Row.Builder().setTitle(place.name)
            val detail = place.address ?: place.category
            val metres = FloatArray(1)
            android.location.Location.distanceBetween(here.latitude, here.longitude, place.coordinate.latitude, place.coordinate.longitude, metres)
            row.addText(withDistance(distance(metres[0].toDouble(), s.imperial), detail))
            row.setOnClickListener { directionsTo(place) }
            list.addItem(row.build())
        }
        return builder.setItemList(list.build()).build()
    }
}

/** The routes to a place, to choose one and start. */
class RouteCarScreen(carContext: CarContext, host: Host) : CoreScreen(carContext, host) {
    private var started = false

    init {
        lifecycle.addObserver(object : androidx.lifecycle.DefaultLifecycleObserver {
            override fun onDestroy(owner: androidx.lifecycle.LifecycleOwner) {
                // Left without starting: the directions are closed, as on the phone.
                if (!started) input { now -> Input.CloseDirections(now) }
            }
        })
    }

    override fun key(snapshot: Snapshot?): Any? = snapshot?.directions?.let { d -> listOf(d.loading, d.error, d.selected, d.routes.map { r -> r.duration to r.distance }, d.notice) }

    override fun onGetTemplate(): Template {
        val s = snapshot
        val d = s?.directions
        if (d != null && !d.loading && d.routes.isEmpty()) {
            return MessageTemplate.Builder(d.error ?: carContext.getString(R.string.car_no_route)).setHeaderAction(Action.BACK).build()
        }
        val builder = RoutePreviewNavigationTemplate.Builder().setTitle(d?.destination.orEmpty()).setHeaderAction(Action.BACK)
        if (d == null || d.loading) return builder.setLoading(true).build()
        val list = ItemList.Builder()
        val shown = d.routes.take(listLimit(ConstraintManager.CONTENT_LIMIT_TYPE_ROUTE_LIST))
        for (route in shown) {
            val title = SpannableString(" ")
            title.setSpan(DurationSpan.create(route.durationS.roundToInt().toLong()), 0, 1, Spanned.SPAN_INCLUSIVE_INCLUSIVE)
            val row = Row.Builder().setTitle(title)
            row.addText(withDistance(distance(route.distanceM, s.imperial), route.summary.ifEmpty { null }))
            list.addItem(row.build())
        }
        // Within the rows shown: a route chosen on the phone may lie beyond the car's limit.
        list.setSelectedIndex(d.selected.coerceIn(0, (shown.size - 1).coerceAtLeast(0)))
        list.setOnSelectedListener { index -> input { now -> Input.ChooseRoute(now, index) } }
        builder.setItemList(list.build())
        builder.setNavigateAction(
            Action.Builder().setTitle(carContext.getString(R.string.start)).setIcon(icon(R.drawable.ic_car_straight)).setOnClickListener {
                started = true
                host.locate(Host.Locating.CAR, precise = true)
                input { now -> Input.StartNavigation(now) }
                screenManager.popToRoot()
            }.build(),
        )
        return builder.build()
    }
}
