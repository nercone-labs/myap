package dev.nercone.myap.car

import android.app.Presentation
import android.graphics.Rect
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.util.Log
import androidx.car.app.CarContext
import androidx.car.app.SurfaceCallback
import androidx.car.app.SurfaceContainer
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.coroutineScope
import dev.nercone.myap.core.Snapshot
import dev.nercone.myap.service.Host
import dev.nercone.myap.ui.MapPane
import kotlin.math.ln
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapView

/**
 * The map of the car's screen: MapLibre drawing, on the surface the host gives, the same style
 * and features as the phone's map. The surface is a display of its own (a virtual display), and
 * the map a view shown on it (a presentation), as the Car App Library advises for drawing with
 * views. The host tells which part of the surface its templates leave visible, and the camera
 * keeps its target in that part.
 *
 * Every callback of the host comes on the main thread, as MapLibre needs.
 */
class CarMap(private val carContext: CarContext, private val host: Host, private val lifecycle: Lifecycle) : SurfaceCallback, DefaultLifecycleObserver {
    private val controller = MapPane.Controller()
    private var display: VirtualDisplay? = null
    private var presentation: Presentation? = null
    private var view: MapView? = null
    private var updates: Job? = null
    private var size = Rect()
    private var visible: Rect? = null

    /** Whether the camera rides with the position (otherwise the user moved the map). */
    var following = true
        private set

    init {
        lifecycle.addObserver(this)
    }

    override fun onCreate(owner: LifecycleOwner) {
        carContext.getCarService(androidx.car.app.AppManager::class.java).setSurfaceCallback(this)
    }

    override fun onDestroy(owner: LifecycleOwner) {
        release()
    }

    override fun onSurfaceAvailable(container: SurfaceContainer) {
        val surface = container.surface ?: return
        release()
        size = Rect(0, 0, container.width, container.height)
        try {
            val manager = carContext.getSystemService(DisplayManager::class.java)
            val made = manager.createVirtualDisplay("myap-car-map", container.width, container.height, container.dpi, surface, DisplayManager.VIRTUAL_DISPLAY_FLAG_OWN_CONTENT_ONLY)
            display = made
            val shown = Presentation(carContext, made.display)
            val mapView = MapView(shown.context)
            shown.setContentView(mapView)
            shown.show()
            presentation = shown
            view = mapView
            mapView.onCreate(null)
            mapView.onStart()
            mapView.onResume()
            mapView.getMapAsync { map ->
                if (view !== mapView) return@getMapAsync
                MapPane.ready(controller, map)
                map.uiSettings.isCompassEnabled = false
                applyPadding()
                host.snapshot.value?.let { draw(it, force = true) }
            }
        } catch (error: RuntimeException) {
            // A surface the display cannot use: the templates go on without a map.
            Log.e(Host.TAG, "car map", error)
            release()
            return
        }
        updates = lifecycle.coroutineScope.launch { host.snapshot.collect { snapshot -> snapshot?.let { draw(it, force = false) } } }
    }

    override fun onSurfaceDestroyed(container: SurfaceContainer) = release()

    override fun onVisibleAreaChanged(visibleArea: Rect) {
        visible = Rect(visibleArea)
        applyPadding()
    }

    override fun onStableAreaChanged(stableArea: Rect) {}

    override fun onScroll(distanceX: Float, distanceY: Float) {
        val map = controller.map ?: return
        following = false
        map.scrollBy(-distanceX, -distanceY, 0)
    }

    override fun onScale(focusX: Float, focusY: Float, scaleFactor: Float) {
        val map = controller.map ?: return
        if (scaleFactor <= 0f) return
        following = false
        val by = ln(scaleFactor.toDouble()) / ln(2.0)
        // A negative focus means the host has none (a double tap of a button): round the centre.
        val update = if (focusX < 0f || focusY < 0f) CameraUpdateFactory.zoomBy(by) else CameraUpdateFactory.zoomBy(by, android.graphics.Point(focusX.toInt(), focusY.toInt()))
        map.moveCamera(update)
    }

    /** Zooms in (`by` > 0) or out, around the centre. */
    fun zoom(by: Double) {
        controller.map?.animateCamera(CameraUpdateFactory.zoomBy(by), 300)
    }

    /** The camera rides with the position again. */
    fun recenter() {
        following = true
        host.snapshot.value?.let { follow(it, immediately = false) }
    }

    /** The map moved by hand from the templates (the pan mode). */
    fun panned() {
        following = false
    }

    private fun applyPadding() {
        val map = controller.map ?: return
        val area = visible ?: return
        map.moveCamera(CameraUpdateFactory.paddingTo(area.left.toDouble(), area.top.toDouble(), (size.width() - area.right).coerceAtLeast(0).toDouble(), (size.height() - area.bottom).coerceAtLeast(0).toDouble()))
    }

    private var cameraId: Long? = null

    private fun draw(snapshot: Snapshot, force: Boolean) {
        val map = controller.map ?: return
        if (force) controller.styleKey = null
        MapPane.applyStyle(controller, map, snapshot.style, snapshot.overlay, snapshot.location)
        MapPane.applyOverlay(controller, snapshot.overlay)
        MapPane.applyLocation(controller, snapshot.location)
        val camera = snapshot.camera
        if (snapshot.navigation == null && camera != null && camera.id != cameraId) {
            cameraId = camera.id
            following = false
            val bounds = camera.bounds
            val center = camera.center
            when {
                bounds != null -> map.easeCamera(CameraUpdateFactory.newLatLngBounds(LatLngBounds.from(bounds.north, bounds.east, bounds.south, bounds.west), 48), 700)
                center != null -> map.easeCamera(CameraUpdateFactory.newLatLngZoom(LatLng(center.latitude, center.longitude), camera.zoom ?: 15.0), 700)
            }
            return
        }
        if (following) follow(snapshot, immediately = force)
    }

    /** The camera on the position: during guidance ahead in the direction of travel, tilted as the core asks. */
    private fun follow(snapshot: Snapshot, immediately: Boolean) {
        val map = controller.map ?: return
        val navigation = snapshot.navigation
        val position = navigation?.position ?: snapshot.location?.coordinate ?: return
        val camera = CameraPosition.Builder()
            .target(LatLng(position.latitude, position.longitude))
            .zoom(navigation?.zoom ?: 15.0)
            .bearing(navigation?.bearing ?: 0.0)
            .tilt(navigation?.tilt ?: 0.0)
            .build()
        if (immediately) map.moveCamera(CameraUpdateFactory.newCameraPosition(camera)) else map.easeCamera(CameraUpdateFactory.newCameraPosition(camera), 900)
    }

    private fun release() {
        updates?.cancel()
        updates = null
        view?.let { v ->
            v.onPause()
            v.onStop()
            v.onDestroy()
        }
        view = null
        controller.map = null
        controller.style = null
        controller.styleKey = null
        controller.overlay = null
        presentation?.dismiss()
        presentation = null
        display?.release()
        display = null
    }
}
