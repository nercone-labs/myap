package dev.nercone.myap.ui

import android.content.Context
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.nercone.myap.core.Bounds
import dev.nercone.myap.core.Coordinate
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.Location
import dev.nercone.myap.core.MapStyle
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.service.Host
import okhttp3.OkHttpClient
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.module.http.HttpRequestUtil
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.FillExtrusionLayer
import org.maplibre.android.style.layers.Layer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.RasterLayer
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.android.style.sources.RasterSource
import org.maplibre.android.style.sources.TileSet

/**
 * The map: MapLibre Native drawing the style the core chose, with the features of the core
 * (routes, places) laid over it from its GeoJSON, the device's position, and the camera the core
 * or the guidance asks for. Every decision is the core's; this only draws.
 */
object MapPane {
    private const val SOURCE = "myap"
    private const val ME = "myap-me"

    /** What the map tells the screens. */
    interface Listener {
        fun onResult(index: Int)
        fun onAlternative(index: Int)
        fun onLongPress(coordinate: Coordinate)
        fun onViewport(center: Coordinate, bounds: Bounds, zoom: Double)
        /** The user moved the map by hand (which ends following the position). */
        fun onUserMoved()
    }

    /** Sets up MapLibre once per process, with Myap's own identity in its requests. */
    fun initialize(context: Context, http: OkHttpClient) {
        MapLibre.getInstance(context.applicationContext)
        val client = http.newBuilder().addInterceptor { chain -> chain.proceed(chain.request().newBuilder().header("User-Agent", Host.USER_AGENT).build()) }.build()
        HttpRequestUtil.setOkHttpClient(client)
    }

    /**
     * The state of one map: what was last drawn, so that only changes are drawn again. The map and
     * its style are state of Compose, so that what waits for them is drawn as soon as they are
     * ready, not at the next change of the snapshot.
     */
    class Controller {
        var map: MapLibreMap? by mutableStateOf(null)
        var style: Style? by mutableStateOf(null)
        var styleKey: String? = null
        var overlay: String? = null
        var cameraId: Long? = null
        var located = false
        var listener: Listener? = null
        var lastPadding: IntArray = intArrayOf(0, 0, 0, 0)
    }

    @Composable
    fun Content(controller: Controller, snapshot: Snapshot, following: Boolean, padding: PaddingValues, listener: Listener, modifier: Modifier = Modifier) {
        val context = LocalContext.current
        val lifecycle = LocalLifecycleOwner.current.lifecycle
        controller.listener = listener
        val view = remember {
            MapView(context).also { mapView ->
                mapView.onCreate(null)
                mapView.getMapAsync { map -> ready(controller, map) }
            }
        }
        DisposableEffect(lifecycle) {
            val observer = LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> view.onStart()
                    Lifecycle.Event.ON_RESUME -> view.onResume()
                    Lifecycle.Event.ON_PAUSE -> view.onPause()
                    Lifecycle.Event.ON_STOP -> view.onStop()
                    else -> {}
                }
            }
            lifecycle.addObserver(observer)
            onDispose {
                lifecycle.removeObserver(observer)
                // Left while the activity is shown (the settings open over it): the map is paused
                // and stopped first, as MapLibre needs, so that its connectivity receiver and file
                // source are let go.
                val state = lifecycle.currentState
                if (state.isAtLeast(Lifecycle.State.RESUMED)) view.onPause()
                if (state.isAtLeast(Lifecycle.State.STARTED)) view.onStop()
                view.onDestroy()
                if (controller.map != null) {
                    controller.map = null
                    controller.style = null
                }
            }
        }
        val density = LocalDensity.current
        val direction = LocalLayoutDirection.current
        val pixels = with(density) {
            intArrayOf(
                padding.calculateLeftPadding(direction).roundToPx(),
                padding.calculateTopPadding().roundToPx(),
                padding.calculateRightPadding(direction).roundToPx(),
                padding.calculateBottomPadding().roundToPx(),
            )
        }
        AndroidView(factory = { view }, modifier = modifier)
        LaunchedEffect(snapshot.style, controller.map) { controller.map?.let { applyStyle(controller, it, snapshot.style, snapshot.overlay, snapshot.location) } }
        LaunchedEffect(snapshot.overlay, controller.style) { applyOverlay(controller, snapshot.overlay) }
        LaunchedEffect(snapshot.location, controller.style) { applyLocation(controller, snapshot.location) }
        LaunchedEffect(pixels.toList(), controller.map) {
            controller.lastPadding = pixels
            controller.map?.moveCamera(CameraUpdateFactory.paddingTo(pixels[0].toDouble(), pixels[1].toDouble(), pixels[2].toDouble(), pixels[3].toDouble()))
        }
        LaunchedEffect(snapshot.camera?.id, controller.map) {
            val camera = snapshot.camera ?: return@LaunchedEffect
            val map = controller.map ?: return@LaunchedEffect
            if (controller.cameraId == camera.id || following) return@LaunchedEffect
            controller.cameraId = camera.id
            val bounds = camera.bounds
            val center = camera.center
            when {
                bounds != null -> map.easeCamera(CameraUpdateFactory.newLatLngBounds(LatLngBounds.from(bounds.north, bounds.east, bounds.south, bounds.west), 96), 700)
                center != null -> map.easeCamera(CameraUpdateFactory.newLatLngZoom(LatLng(center.latitude, center.longitude), camera.zoom ?: map.cameraPosition.zoom.coerceAtLeast(15.0)), 700)
            }
        }
        // While following, the camera rides with the position, ahead in the direction of travel.
        val navigation = snapshot.navigation
        // Also when the map becomes ready (an activity made again during guidance).
        LaunchedEffect(navigation?.position, navigation?.bearing, navigation?.zoom, following, controller.map) {
            val map = controller.map ?: return@LaunchedEffect
            if (!following) return@LaunchedEffect
            val position = navigation?.position ?: snapshot.location?.coordinate ?: return@LaunchedEffect
            val bearing = navigation?.bearing ?: snapshot.location?.bearing ?: map.cameraPosition.bearing
            // The core chooses the zoom and the tilt by the speed and the next maneuver.
            val zoom = navigation?.zoom ?: 17.0
            val camera = CameraPosition.Builder().target(LatLng(position.latitude, position.longitude)).zoom(zoom).bearing(bearing).tilt(navigation?.tilt ?: 0.0).build()
            map.easeCamera(CameraUpdateFactory.newCameraPosition(camera), 900)
        }
        // The first location centres the map once, as a map application opens where the user is.
        LaunchedEffect(snapshot.location != null, controller.map) {
            val location = snapshot.location ?: return@LaunchedEffect
            val map = controller.map ?: return@LaunchedEffect
            if (controller.located || snapshot.camera != null) return@LaunchedEffect
            controller.located = true
            map.moveCamera(CameraUpdateFactory.newLatLngZoom(LatLng(location.coordinate.latitude, location.coordinate.longitude), 15.0))
        }
    }

    /** Shows the whole route left, north up, for the overview of the guidance. */
    fun overview(controller: Controller, snapshot: Snapshot) {
        val map = controller.map ?: return
        val points = runCatching {
            val root = org.json.JSONObject(snapshot.overlay)
            val features = root.getJSONArray("features")
            buildList {
                for (i in 0 until features.length()) {
                    val feature = features.getJSONObject(i)
                    if (feature.getJSONObject("properties").optString("kind") != "route") continue
                    val coordinates = feature.getJSONObject("geometry").getJSONArray("coordinates")
                    for (j in 0 until coordinates.length()) {
                        val c = coordinates.getJSONArray(j)
                        add(LatLng(c.getDouble(1), c.getDouble(0)))
                    }
                }
            }
        }.getOrDefault(emptyList())
        if (points.size < 2) return
        val bounds = LatLngBounds.Builder().includes(points).build()
        map.easeCamera(CameraUpdateFactory.newLatLngBounds(bounds, 0.0, 0.0, 120), 800)
    }

    /** Centres the map on `coordinate`. */
    fun center(controller: Controller, coordinate: Coordinate, zoom: Double = 16.0) {
        controller.map?.easeCamera(CameraUpdateFactory.newLatLngZoom(LatLng(coordinate.latitude, coordinate.longitude), zoom), 600)
    }

    /** Sets up a map that became ready, for `controller`. */
    fun ready(controller: Controller, map: MapLibreMap) {
        // A new map has none of what was drawn on another.
        controller.map = map
        controller.style = null
        controller.styleKey = null
        controller.overlay = null
        controller.cameraId = null
        map.uiSettings.isLogoEnabled = false
        // The attribution is written on the map by the screen (`MapStyle.attribution`).
        map.uiSettings.isAttributionEnabled = false
        map.uiSettings.isCompassEnabled = true
        map.moveCamera(CameraUpdateFactory.newLatLngZoom(LatLng(35.681236, 139.767125), 11.0))
        map.addOnMapClickListener { point ->
            val screen = map.projection.toScreenLocation(point)
            val box = 24f
            val area = android.graphics.RectF(screen.x - box, screen.y - box, screen.x + box, screen.y + box)
            val features = map.queryRenderedFeatures(area, "myap-results", "myap-selected", "myap-route-alternative")
            val feature = features.firstOrNull() ?: return@addOnMapClickListener false
            val index = feature.getNumberProperty("index")?.toInt() ?: return@addOnMapClickListener false
            when (feature.getStringProperty("kind")) {
                "result" -> controller.listener?.onResult(index)
                "route_alternative" -> controller.listener?.onAlternative(index)
                else -> return@addOnMapClickListener false
            }
            true
        }
        map.addOnMapLongClickListener { point ->
            controller.listener?.onLongPress(Coordinate(point.latitude, point.longitude))
            true
        }
        map.addOnCameraMoveStartedListener { reason ->
            if (reason == MapLibreMap.OnCameraMoveStartedListener.REASON_API_GESTURE) controller.listener?.onUserMoved()
        }
        map.addOnCameraIdleListener {
            val target = map.cameraPosition.target ?: return@addOnCameraIdleListener
            val region = map.projection.visibleRegion.latLngBounds
            controller.listener?.onViewport(
                Coordinate(target.latitude, target.longitude),
                Bounds(region.latitudeSouth, region.longitudeWest, region.latitudeNorth, region.longitudeEast),
                map.cameraPosition.zoom,
            )
        }
    }

    private fun styleKey(style: MapStyle): String = listOf(style.url, style.json, style.overlays.joinToString { it.id + it.tiles.firstOrNull() }).joinToString("|")

    /** Draws `mapStyle` with Myap's layers, unless it is already drawn. */
    fun applyStyle(controller: Controller, map: MapLibreMap, mapStyle: MapStyle, overlay: String, location: Location?) {
        val key = styleKey(mapStyle)
        if (controller.styleKey == key) return
        controller.styleKey = key
        controller.style = null
        val builder = when {
            mapStyle.json != null -> Style.Builder().fromJson(mapStyle.json)
            mapStyle.url != null -> Style.Builder().fromUri(mapStyle.url)
            else -> return
        }
        map.setStyle(builder) { style ->
            if (controller.styleKey != key) return@setStyle
            // The layers of Myap go under the labels of the base map, so that names stay readable
            // over routes, but above its 3D buildings, which would hide a route once the camera
            // tilts: under the first labels after the last extruded layer. (In OpenFreeMap's
            // styles the one-way arrows, labels too, come before the buildings.)
            val layers = style.layers
            val lastExtrusion = layers.indexOfLast { it is FillExtrusionLayer }
            val firstLabels = layers.drop(lastExtrusion + 1).firstOrNull { it is SymbolLayer }?.id
            fun add(layer: Layer) {
                if (firstLabels != null) style.addLayerBelow(layer, firstLabels) else style.addLayer(layer)
            }
            mapStyle.overlays.forEach { o ->
                val tiles = TileSet("2.1.0", *o.tiles.toTypedArray()).apply { maxZoom = o.maxZoom.toFloat() }
                style.addSource(RasterSource(o.id, tiles, 256))
                add(RasterLayer(o.id, o.id).withProperties(PropertyFactory.rasterOpacity(o.opacity.toFloat())))
            }
            style.addSource(GeoJsonSource(SOURCE))
            style.addSource(GeoJsonSource(ME))
            val kind = Expression.get("kind")
            fun line(id: String, value: String, color: String, width: Float) = LineLayer(id, SOURCE).withFilter(Expression.eq(kind, Expression.literal(value))).withProperties(
                PropertyFactory.lineColor(color),
                PropertyFactory.lineWidth(width),
                PropertyFactory.lineCap(Property.LINE_CAP_ROUND),
                PropertyFactory.lineJoin(Property.LINE_JOIN_ROUND),
            )
            add(line("myap-route-alternative", "route_alternative", MapColors.ROUTE_ALTERNATIVE, 7f))
            add(line("myap-route-passed", "route_passed", MapColors.ROUTE_PASSED, 7f))
            add(line("myap-route-casing", "route", MapColors.ROUTE_CASING, 10f))
            add(line("myap-route", "route", MapColors.ROUTE, 7f))
            // The congestion over the route: orange to dark red, as traffic maps colour it.
            add(
                LineLayer("myap-route-traffic", SOURCE).withFilter(Expression.eq(kind, Expression.literal("route_traffic"))).withProperties(
                    PropertyFactory.lineColor(
                        Expression.match(
                            Expression.get("congestion"),
                            Expression.color(android.graphics.Color.parseColor("#F29900")),
                            Expression.stop("light", Expression.color(android.graphics.Color.parseColor("#F9AB00"))),
                            Expression.stop("moderate", Expression.color(android.graphics.Color.parseColor("#F29900"))),
                            Expression.stop("heavy", Expression.color(android.graphics.Color.parseColor("#D93025"))),
                            Expression.stop("stopped", Expression.color(android.graphics.Color.parseColor("#A50E0E"))),
                            Expression.stop("closed", Expression.color(android.graphics.Color.parseColor("#3C4043"))),
                        ),
                    ),
                    PropertyFactory.lineWidth(7f),
                    PropertyFactory.lineCap(Property.LINE_CAP_ROUND),
                    PropertyFactory.lineJoin(Property.LINE_JOIN_ROUND),
                ),
            )
            add(
                FillLayer("myap-avoid-area", SOURCE).withFilter(Expression.eq(kind, Expression.literal("avoid_area"))).withProperties(
                    PropertyFactory.fillColor(MapColors.AVOID),
                    PropertyFactory.fillOpacity(0.25f),
                    PropertyFactory.fillOutlineColor(MapColors.AVOID),
                ),
            )
            fun circle(id: String, value: String, color: String, radius: Float, stroke: String = "#FFFFFF", strokeWidth: Float = 2f) = CircleLayer(id, SOURCE).withFilter(Expression.eq(kind, Expression.literal(value))).withProperties(
                PropertyFactory.circleColor(color),
                PropertyFactory.circleRadius(radius),
                PropertyFactory.circleStrokeColor(stroke),
                PropertyFactory.circleStrokeWidth(strokeWidth),
            )
            // Points and their labels go above everything, labels of the base map included.
            style.addLayer(circle("myap-saved", "saved", MapColors.SAVED, 6f))
            style.addLayer(circle("myap-alerts", "alert", MapColors.ALERT, 6f, "#3D2E00", 2f))
            style.addLayer(circle("myap-results", "result", MapColors.RESULT, 7f))
            style.addLayer(circle("myap-stops", "stop", MapColors.STOP, 7f))
            style.addLayer(circle("myap-origin", "origin", MapColors.ORIGIN, 7f, MapColors.ROUTE, 3f))
            style.addLayer(circle("myap-destination", "destination", MapColors.RESULT, 9f, "#FFFFFF", 3f))
            style.addLayer(circle("myap-selected", "selected", MapColors.RESULT, 11f, "#FFFFFF", 3f))
            style.addLayer(
                SymbolLayer("myap-labels", SOURCE).withFilter(Expression.any(Expression.eq(kind, Expression.literal("result")), Expression.eq(kind, Expression.literal("selected")))).withProperties(
                    PropertyFactory.textField(Expression.get("name")),
                    PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
                    PropertyFactory.textSize(12f),
                    PropertyFactory.textAnchor(Property.TEXT_ANCHOR_TOP),
                    PropertyFactory.textOffset(arrayOf(0f, 1.1f)),
                    PropertyFactory.textColor("#202124"),
                    PropertyFactory.textHaloColor("#FFFFFF"),
                    PropertyFactory.textHaloWidth(1.5f),
                    PropertyFactory.textOptional(true),
                ),
            )
            style.addLayer(CircleLayer("myap-me-halo", ME).withProperties(PropertyFactory.circleColor(MapColors.ME), PropertyFactory.circleOpacity(0.18f), PropertyFactory.circleRadius(18f)))
            style.addLayer(CircleLayer("myap-me", ME).withProperties(PropertyFactory.circleColor(MapColors.ME), PropertyFactory.circleRadius(8f), PropertyFactory.circleStrokeColor("#FFFFFF"), PropertyFactory.circleStrokeWidth(3f)))
            controller.style = style
            controller.overlay = null
            applyOverlay(controller, overlay)
            applyLocation(controller, location)
        }
    }

    fun applyOverlay(controller: Controller, overlay: String) {
        val style = controller.style ?: return
        if (controller.overlay == overlay) return
        controller.overlay = overlay
        style.getSourceAs<GeoJsonSource>(SOURCE)?.setGeoJson(overlay)
    }

    fun applyLocation(controller: Controller, location: Location?) {
        val style = controller.style ?: return
        val json = location?.let { """{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[${it.coordinate.longitude},${it.coordinate.latitude}]},"properties":{}}]}""" }
            ?: """{"type":"FeatureCollection","features":[]}"""
        style.getSourceAs<GeoJsonSource>(ME)?.setGeoJson(json)
    }
}
