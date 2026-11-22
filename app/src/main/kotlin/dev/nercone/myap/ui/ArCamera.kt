package dev.nercone.myap.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.hardware.camera2.CameraAccessException
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.hardware.camera2.params.OutputConfiguration
import android.hardware.camera2.params.SessionConfiguration
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.util.Size
import android.view.TextureView
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.GpsOff
import androidx.compose.material.icons.filled.PhotoCamera
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.IntState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableLongState
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.nercone.myap.R
import dev.nercone.myap.core.ArView
import dev.nercone.myap.core.NavigationState
import dev.nercone.myap.core.NavigationView
import dev.nercone.myap.core.TravelMode
import java.util.concurrent.Executor
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.hypot
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sqrt
import kotlin.math.tan
import android.view.Surface as ViewSurface

/**
 * The camera view of the guidance: the live picture of the back camera, full screen, with the
 * route ahead drawn over it in perspective on the ground, chevrons along it, a marker standing
 * over the next maneuver, and the guidance banner at the top.
 *
 * The core computes the route ahead (`NavigationView.ar`: points in metres east, north and up
 * from the traveller); this only projects and draws them. The projection is redone at the rate
 * of the orientation sensor, while the route changes at each location fix.
 *
 * Nothing of the camera is recorded or sent: the frames go only to the screen. Only the platform
 * is used (Camera2 and the sensors), no other library.
 */
object ArCamera {
    /** At most this many points of the route are drawn; more are thinned evenly. */
    const val MAX_POINTS = 60
    /** The largest side of the camera's preview stream, in pixels. */
    const val MAX_PREVIEW = 1920
    /** How high the marker of the next maneuver stands over the ground, in metres. */
    const val MARKER_HEIGHT = 2f
    /** The distance between two chevrons on the route, in metres. */
    const val CHEVRON_SPACING = 10f
    /** Where the first chevron is, from the traveller, in metres. */
    const val CHEVRON_START = 6f
    /** The distance at which chevrons have faded the most, in metres. */
    const val FADE_DISTANCE = 250f
    /** Half the width of the bright line along the middle of the route, in metres. */
    const val CENTRE_HALF_WIDTH = 0.15f
    /** How much of each new orientation is taken in (a low-pass filter, 0 to 1). */
    const val SMOOTHING = 0.2f

    private val ROUTE_COLOR = Color(android.graphics.Color.parseColor(MapColors.ROUTE))
    private val CENTRE_COLOR = lerp(ROUTE_COLOR, Color.White, 0.55f)
    private val SIGN_RED = Color(0xFFD93025)
    private val SCRIM = Color.Black.copy(alpha = 0.6f)

    /**
     * The camera view: asks for the camera when it is not granted yet, then shows the picture
     * and the route over it. [onClose] is called by the close button and by back.
     */
    @Composable
    fun Content(nav: NavigationView, onClose: () -> Unit) {
        val context = LocalContext.current
        var granted by remember { mutableStateOf(context.checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) }
        var denied by remember { mutableStateOf(false) }
        val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { result ->
            granted = result
            denied = !result
        }
        LaunchedEffect(Unit) { if (!granted) launcher.launch(Manifest.permission.CAMERA) }
        BackHandler(onBack = onClose)
        val view = LocalView.current
        DisposableEffect(view) {
            view.keepScreenOn = true
            onDispose { view.keepScreenOn = false }
        }
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            if (granted) Granted(nav, onClose) else Permission(denied, onClose)
        }
    }

    /** Why the camera is needed, while it is asked for or after it was refused. */
    @Composable
    private fun Permission(denied: Boolean, onClose: () -> Unit) {
        Box(Modifier.fillMaxSize().safeDrawingPadding().padding(24.dp)) {
            Column(Modifier.align(Alignment.Center).widthIn(max = 560.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Icon(Icons.Default.PhotoCamera, null, Modifier.size(48.dp), tint = Color.White)
                Text(stringResource(R.string.ar_title), style = MaterialTheme.typography.headlineSmall, color = Color.White)
                Text(stringResource(R.string.ar_permission), style = MaterialTheme.typography.bodyLarge, color = Color.White, textAlign = TextAlign.Center)
                if (denied) Button(onClick = onClose) { Text(stringResource(R.string.ar_close)) }
            }
        }
    }

    @Composable
    private fun Granted(nav: NavigationView, onClose: () -> Unit) {
        val scene = remember { Scene() }
        val rotation = rememberDisplayRotation()
        TrackOrientation(scene, rotation)
        val density = LocalDensity.current.density
        val buffers = remember(density) { Buffers(density) }
        val marker = remember { FloatArray(5) }
        val route = remember(nav.ar) { Route.of(nav.ar) }
        val halfWidth = if (nav.mode == TravelMode.CAR) 1.8f else 1.2f
        val drawn = nav.state != NavigationState.ARRIVED && route.count >= 2
        Box(Modifier.fillMaxSize()) {
            CameraPreview(scene, rotation.intValue)
            if (drawn) {
                Canvas(Modifier.fillMaxSize()) {
                    scene.version.longValue
                    if (scene.oriented) drawRoute(route, scene, halfWidth, buffers)
                }
                if (route.hasManeuver) Marker(nav, route, scene, marker)
            }
            Overlay(nav, scene, onClose)
        }
    }

    /**
     * The marker of the next maneuver: its icon and distance, standing [MARKER_HEIGHT] over the
     * ground where the maneuver is. It is placed again at each orientation, without composing
     * again, and not shown while the maneuver is off the screen.
     */
    @Composable
    private fun Marker(nav: NavigationView, route: Route, scene: Scene, point: FloatArray) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.layout { measurable, constraints ->
                val placeable = measurable.measure(constraints.copy(minWidth = 0, minHeight = 0))
                val width = constraints.maxWidth
                val height = constraints.maxHeight
                layout(width, height) {
                    scene.version.longValue
                    val front = scene.oriented && scene.project(route.maneuverEast, route.maneuverNorth, route.maneuverUp + MARKER_HEIGHT, width.toFloat(), height.toFloat(), point)
                    if (front && point[3] >= 0f && point[3] <= width && point[4] >= 0f && point[4] <= height) {
                        placeable.place(point[3].roundToInt() - placeable.width / 2, point[4].roundToInt() - placeable.height)
                    }
                }
            },
        ) {
            Surface(color = MapColors.GUIDANCE, contentColor = MapColors.ON_GUIDANCE, shape = CircleShape, shadowElevation = 6.dp) {
                Icon(Labels.maneuver(nav.icon, nav.drivingSide), null, Modifier.padding(10.dp).size(44.dp))
            }
            Surface(color = SCRIM, contentColor = Color.White, shape = RoundedCornerShape(12.dp), modifier = Modifier.padding(top = 4.dp)) {
                Text(nav.distance, Modifier.padding(horizontal = 10.dp, vertical = 4.dp), style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Bold)
            }
        }
    }

    /** The guidance banner, the notices, the speed and the close button, over the picture. */
    @Composable
    private fun Overlay(nav: NavigationView, scene: Scene, onClose: () -> Unit) {
        var safety by rememberSaveable { mutableStateOf(true) }
        Box(Modifier.fillMaxSize()) {
            Column(Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(12.dp).widthIn(max = 720.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (safety) {
                    Surface(color = MaterialTheme.colorScheme.secondaryContainer, contentColor = MaterialTheme.colorScheme.onSecondaryContainer, shape = RoundedCornerShape(16.dp)) {
                        Row(Modifier.padding(start = 16.dp, top = 4.dp, bottom = 4.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(stringResource(R.string.ar_safety), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
                            IconButton(onClick = { safety = false }) { Icon(Icons.Default.Close, stringResource(R.string.close)) }
                        }
                    }
                }
                Surface(
                    color = MapColors.GUIDANCE,
                    contentColor = MapColors.ON_GUIDANCE,
                    shape = RoundedCornerShape(24.dp),
                    shadowElevation = 6.dp,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Labels.maneuver(nav.icon, nav.drivingSide), null, Modifier.size(56.dp))
                        Spacer(Modifier.width(16.dp))
                        Column(Modifier.weight(1f)) {
                            if (nav.state != NavigationState.ARRIVED) Text(nav.distance, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
                            Text(nav.instruction, style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                            nav.road?.takeIf { it !in nav.instruction }?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                        }
                    }
                }
                if (nav.estimated || nav.alerts.isNotEmpty()) {
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (nav.estimated) {
                            Surface(color = SCRIM, contentColor = Color.White, shape = RoundedCornerShape(16.dp)) {
                                Icon(Icons.Default.GpsOff, null, Modifier.padding(8.dp).size(20.dp))
                            }
                        }
                        nav.alerts.forEach { alert ->
                            Surface(color = SCRIM, contentColor = Color.White, shape = RoundedCornerShape(16.dp)) {
                                Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Icon(Labels.alert(alert.icon), null, Modifier.size(20.dp))
                                    Spacer(Modifier.width(6.dp))
                                    Text("${alert.name} · ${alert.distance}", style = MaterialTheme.typography.labelLarge)
                                }
                            }
                        }
                    }
                }
                if (nav.state == NavigationState.REROUTING) Notice(stringResource(R.string.rerouting))
                if (scene.noSensor) Notice(stringResource(R.string.ar_no_sensor)) else if (scene.calibrate) Notice(stringResource(R.string.ar_calibrate))
                if (scene.cameraFailed) Notice(stringResource(R.string.ar_no_camera))
            }
            Row(Modifier.align(Alignment.BottomStart).navigationBarsPadding().padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                nav.speedLimit?.let { limit ->
                    Box(Modifier.size(60.dp).background(Color.White, CircleShape).border(6.dp, SIGN_RED, CircleShape), contentAlignment = Alignment.Center) {
                        Text("$limit", color = Color.Black, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    }
                }
                nav.speed?.let { speed ->
                    Surface(color = SCRIM, contentColor = if (nav.speeding) SIGN_RED else Color.White, shape = RoundedCornerShape(16.dp)) {
                        Text("$speed", Modifier.padding(horizontal = 12.dp, vertical = 6.dp), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                    }
                }
            }
            FilledTonalIconButton(onClick = onClose, modifier = Modifier.align(Alignment.BottomEnd).navigationBarsPadding().padding(16.dp).size(56.dp)) {
                Icon(Icons.Default.Close, stringResource(R.string.ar_close))
            }
        }
    }

    @Composable
    private fun Notice(text: String) {
        Surface(color = MaterialTheme.colorScheme.tertiaryContainer, contentColor = MaterialTheme.colorScheme.onTertiaryContainer, shape = RoundedCornerShape(16.dp)) {
            Text(text, Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
        }
    }

    /** The rotation of the display (`Surface.ROTATION_*`), followed as it changes. */
    @Composable
    private fun rememberDisplayRotation(): IntState {
        val view = LocalView.current
        val state = remember(view) { mutableIntStateOf(view.display?.rotation ?: ViewSurface.ROTATION_0) }
        DisposableEffect(view) {
            val displays = view.context.getSystemService(DisplayManager::class.java)
            val listener = object : DisplayManager.DisplayListener {
                override fun onDisplayAdded(displayId: Int) {}
                override fun onDisplayRemoved(displayId: Int) {}
                override fun onDisplayChanged(displayId: Int) {
                    val display = view.display ?: return
                    if (display.displayId == displayId) state.intValue = display.rotation
                }
            }
            displays?.registerDisplayListener(listener, Handler(Looper.getMainLooper()))
            onDispose { displays?.unregisterDisplayListener(listener) }
        }
        return state
    }

    /**
     * Follows the orientation of the device into [scene] while the screen is resumed, from the
     * rotation vector (or, without it, the geomagnetic rotation vector), and the accuracy of the
     * compass: of the magnetometer when there is one, else of the rotation vector.
     */
    @Composable
    private fun TrackOrientation(scene: Scene, rotation: IntState) {
        val context = LocalContext.current
        val lifecycle = LocalLifecycleOwner.current.lifecycle
        DisposableEffect(scene) {
            val manager = context.getSystemService(SensorManager::class.java)
            val sensor = manager?.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR) ?: manager?.getDefaultSensor(Sensor.TYPE_GEOMAGNETIC_ROTATION_VECTOR)
            if (manager == null || sensor == null) {
                scene.noSensor = true
                return@DisposableEffect onDispose {}
            }
            val magnetometer = manager.getDefaultSensor(Sensor.TYPE_MAGNETIC_FIELD)
            val listener = object : SensorEventListener {
                var magnetic = SensorManager.SENSOR_STATUS_ACCURACY_HIGH
                var vector = SensorManager.SENSOR_STATUS_ACCURACY_HIGH

                fun judge() {
                    val accuracy = if (magnetometer != null) magnetic else vector
                    val low = accuracy == SensorManager.SENSOR_STATUS_UNRELIABLE || accuracy == SensorManager.SENSOR_STATUS_ACCURACY_LOW
                    if (scene.calibrate != low) scene.calibrate = low
                }

                override fun onSensorChanged(event: SensorEvent) {
                    if (event.sensor.type == Sensor.TYPE_MAGNETIC_FIELD) return
                    if (event.accuracy != vector) {
                        vector = event.accuracy
                        judge()
                    }
                    scene.orient(event.values, rotation.intValue)
                }

                override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {
                    if (sensor.type == Sensor.TYPE_MAGNETIC_FIELD) magnetic = accuracy else vector = accuracy
                    judge()
                }
            }
            var registered = false
            val observer = LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_RESUME -> if (!registered) {
                        registered = true
                        manager.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME)
                        if (magnetometer != null) manager.registerListener(listener, magnetometer, SensorManager.SENSOR_DELAY_UI)
                    }
                    Lifecycle.Event.ON_PAUSE -> if (registered) {
                        registered = false
                        manager.unregisterListener(listener)
                    }
                    else -> {}
                }
            }
            lifecycle.addObserver(observer)
            onDispose {
                lifecycle.removeObserver(observer)
                manager.unregisterListener(listener)
            }
        }
    }

    /** The picture of the back camera, opened while the screen is resumed. */
    @Composable
    private fun CameraPreview(scene: Scene, rotation: Int) {
        val context = LocalContext.current
        val lifecycle = LocalLifecycleOwner.current.lifecycle
        val preview = remember { Preview(context, rotation).also { it.listener = scene } }
        SideEffect { preview.rotate(rotation) }
        DisposableEffect(preview) {
            val observer = LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_RESUME -> preview.resume()
                    Lifecycle.Event.ON_PAUSE -> preview.pause()
                    else -> {}
                }
            }
            lifecycle.addObserver(observer)
            onDispose {
                lifecycle.removeObserver(observer)
                preview.release()
            }
        }
        AndroidView(factory = { preview.view }, modifier = Modifier.fillMaxSize())
    }

    /**
     * Draws the route: the ribbon on the ground, its bright middle line, the chevrons, the pole
     * of the maneuver's marker, and an arrow at the edge of the screen toward the maneuver when it
     * is off the screen. Allocates nothing: every array and path is in [b].
     */
    private fun DrawScope.drawRoute(route: Route, scene: Scene, halfWidth: Float, b: Buffers) {
        val r = scene.matrix
        val w = size.width
        val h = size.height
        val fx = Projection.focal(w, scene.tanX)
        val fy = Projection.focal(h, scene.tanY)
        ribbon(route, r, halfWidth, w, h, fx, fy, b, b.ribbon)
        drawPath(b.ribbon, ROUTE_COLOR, alpha = 0.55f)
        ribbon(route, r, CENTRE_HALF_WIDTH, w, h, fx, fy, b, b.centre)
        drawPath(b.centre, CENTRE_COLOR, alpha = 0.9f)
        for (c in 0 until route.chevronCount) {
            if (chevron(route, c, r, halfWidth, w, h, fx, fy, b)) {
                val fade = (1f - route.chevrons[c * Route.CHEVRON_FIELDS + 5] / FADE_DISTANCE).coerceIn(0.15f, 1f)
                drawPath(b.chevron, Color.White, alpha = 0.85f * fade)
            }
        }
        if (!route.hasManeuver) return
        val s = b.segment
        Projection.toCamera(r, route.maneuverEast, route.maneuverNorth, route.maneuverUp, s, 0)
        Projection.toCamera(r, route.maneuverEast, route.maneuverNorth, route.maneuverUp + MARKER_HEIGHT, s, 3)
        if (Projection.clipSegment(s)) {
            drawLine(
                Color.White,
                Offset(Projection.screenX(s[0], s[2], w, fx), Projection.screenY(s[1], s[2], h, fy)),
                Offset(Projection.screenX(s[3], s[5], w, fx), Projection.screenY(s[4], s[5], h, fy)),
                strokeWidth = b.pole,
                cap = StrokeCap.Round,
                alpha = 0.85f,
            )
        }
        val p = b.point
        val front = scene.project(route.maneuverEast, route.maneuverNorth, route.maneuverUp + MARKER_HEIGHT, w, h, p)
        if (front && p[3] >= 0f && p[3] <= w && p[4] >= 0f && p[4] <= h) return
        // Toward the maneuver on the screen: the direction of its projection from the centre,
        // which for a point behind is still the side to turn to.
        var vx = p[0] * fx
        var vy = -p[1] * fy
        if (abs(vx) < 1e-3f && abs(vy) < 1e-3f) vy = 1f
        val t = min((w / 2f - b.margin).coerceAtLeast(1f) / max(abs(vx), 1e-6f), (h / 2f - b.margin).coerceAtLeast(1f) / max(abs(vy), 1e-6f))
        val angle = (atan2(vy, vx) * 180.0 / PI).toFloat()
        translate(w / 2f + vx * t, h / 2f + vy * t) {
            rotate(angle, Offset.Zero) {
                drawPath(b.arrow, ROUTE_COLOR)
                drawPath(b.arrow, Color.White, style = b.outline)
            }
        }
    }

    /** Puts into [path] the strip of the route on the ground, [offset] metres to each side. */
    private fun ribbon(route: Route, r: FloatArray, offset: Float, w: Float, h: Float, fx: Float, fy: Float, b: Buffers, path: Path) {
        path.reset()
        for (k in 0 until route.count - 1) {
            route.vertex(k, -offset, r, b.polygon, 0)
            route.vertex(k, offset, r, b.polygon, 1)
            route.vertex(k + 1, offset, r, b.polygon, 2)
            route.vertex(k + 1, -offset, r, b.polygon, 3)
            val count = Projection.clipPolygon(b.polygon, 4, b.clipped)
            addPolygon(path, b.clipped, count, w, h, fx, fy)
        }
    }

    /** Puts chevron [c] into `b.chevron`; false when it is wholly behind the camera. */
    private fun chevron(route: Route, c: Int, r: FloatArray, halfWidth: Float, w: Float, h: Float, fx: Float, fy: Float, b: Buffers): Boolean {
        val f = route.chevrons
        val base = c * Route.CHEVRON_FIELDS
        val e = f[base]
        val n = f[base + 1]
        val u = f[base + 2] + 0.03f
        val de = f[base + 3]
        val dn = f[base + 4]
        val s = halfWidth * 0.55f
        // The tip ahead, the two back corners to the right and left, and the notch between them.
        Projection.toCamera(r, e + de * s * 1.2f, n + dn * s * 1.2f, u, b.polygon, 0)
        Projection.toCamera(r, e - de * s * 0.6f + dn * s, n - dn * s * 0.6f - de * s, u, b.polygon, 3)
        Projection.toCamera(r, e - de * s * 0.1f, n - dn * s * 0.1f, u, b.polygon, 6)
        Projection.toCamera(r, e - de * s * 0.6f - dn * s, n - dn * s * 0.6f + de * s, u, b.polygon, 9)
        val count = Projection.clipPolygon(b.polygon, 4, b.clipped)
        b.chevron.reset()
        addPolygon(b.chevron, b.clipped, count, w, h, fx, fy)
        return count >= 3
    }

    private fun addPolygon(path: Path, v: FloatArray, count: Int, w: Float, h: Float, fx: Float, fy: Float) {
        if (count < 3) return
        for (i in 0 until count) {
            val x = Projection.screenX(v[i * 3], v[i * 3 + 2], w, fx)
            val y = Projection.screenY(v[i * 3 + 1], v[i * 3 + 2], h, fy)
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        path.close()
    }

    /**
     * The geometry of the camera view, as plain functions on numbers.
     *
     * The world frame is East-North-Up with the camera at its origin; a point of the route is
     * (east, north, up − eye height). The rotation matrix is the one of
     * `SensorManager.getRotationMatrixFromVector`, remapped for the display rotation: row-major,
     * it turns a vector of the screen's frame (x to the right of the screen, y to its top, z out
     * of it toward the user) into the world frame. The camera looks out of the back, along −z,
     * so a world point in the camera's frame is the transpose of that matrix applied to it, and it
     * is in front of the camera when its z is at most −[NEAR].
     */
    object Projection {
        /** The near plane, in metres in front of the camera. */
        const val NEAR = 0.5f

        /** Turns a world point into the camera's frame: `out[at..at+2] = rᵀ · (east, north, up)`. */
        fun toCamera(r: FloatArray, east: Float, north: Float, up: Float, out: FloatArray, at: Int) {
            out[at] = r[0] * east + r[3] * north + r[6] * up
            out[at + 1] = r[1] * east + r[4] * north + r[7] * up
            out[at + 2] = r[2] * east + r[5] * north + r[8] * up
        }

        /** Whether a point of camera depth [z] is in front of the near plane. */
        fun visible(z: Float, near: Float = NEAR): Boolean = z <= -near

        /** The fraction from a to b at which the depth crosses the near plane. */
        fun crossing(za: Float, zb: Float, near: Float = NEAR): Float = (-near - za) / (zb - za)

        /** The horizontal position on a screen [width] wide of a point (x, z) of the camera. */
        fun screenX(x: Float, z: Float, width: Float, focal: Float): Float = width / 2f + x / -z * focal

        /** The vertical position, from the top, on a screen [height] high of a point (y, z). */
        fun screenY(y: Float, z: Float, height: Float, focal: Float): Float = height / 2f - y / -z * focal

        /** The focal length in pixels of a side of [size] pixels seeing ±[tanHalf] (tangent of half the field). */
        fun focal(size: Float, tanHalf: Float): Float = size / 2f / tanHalf

        /**
         * Clips the segment of [segment] (a then b, x y z each, in the camera's frame) to the
         * front of the near plane, in place. False when it is wholly behind.
         */
        fun clipSegment(segment: FloatArray, near: Float = NEAR): Boolean {
            val va = visible(segment[2], near)
            val vb = visible(segment[5], near)
            if (!va && !vb) return false
            if (va && vb) return true
            val t = crossing(segment[2], segment[5], near)
            val from = if (va) 3 else 0
            for (i in 0 until 3) segment[from + i] = segment[i] + t * (segment[3 + i] - segment[i])
            return true
        }

        /**
         * Clips the polygon of [count] vertices of [input] (x y z each) to the front of the near
         * plane into [output], which must hold count + 1 vertices. Returns the vertices kept.
         */
        fun clipPolygon(input: FloatArray, count: Int, output: FloatArray, near: Float = NEAR): Int {
            var n = 0
            for (i in 0 until count) {
                val a = i * 3
                val b = ((i + 1) % count) * 3
                val va = visible(input[a + 2], near)
                val vb = visible(input[b + 2], near)
                if (va) {
                    output[n * 3] = input[a]
                    output[n * 3 + 1] = input[a + 1]
                    output[n * 3 + 2] = input[a + 2]
                    n++
                }
                if (va != vb) {
                    val t = crossing(input[a + 2], input[b + 2], near)
                    for (j in 0 until 3) output[n * 3 + j] = input[a + j] + t * (input[b + j] - input[a + j])
                    n++
                }
            }
            return n
        }

        /**
         * Remaps the rotation matrix [raw] of the device's natural frame into [out], the frame of
         * the screen at [displayRotation] (`Surface.ROTATION_*`).
         */
        fun remap(raw: FloatArray, displayRotation: Int, out: FloatArray) {
            when (displayRotation) {
                ViewSurface.ROTATION_90 -> SensorManager.remapCoordinateSystem(raw, SensorManager.AXIS_Y, SensorManager.AXIS_MINUS_X, out)
                ViewSurface.ROTATION_180 -> SensorManager.remapCoordinateSystem(raw, SensorManager.AXIS_MINUS_X, SensorManager.AXIS_MINUS_Y, out)
                ViewSurface.ROTATION_270 -> SensorManager.remapCoordinateSystem(raw, SensorManager.AXIS_MINUS_Y, SensorManager.AXIS_X, out)
                else -> raw.copyInto(out)
            }
        }

        /**
         * The field of view on the screen, as the tangents of its half width and half height
         * (`out[0]`, `out[1]`). [tanWidth] and [tanHeight] are those of the whole sensor in its
         * own orientation; the stream of [streamWidth] × [streamHeight] (also in the sensor's
         * orientation) is the largest centred crop of the sensor of its aspect; it is turned by
         * [rotation] degrees onto the screen, and cropped in its centre to fill the view of
         * [viewWidth] × [viewHeight].
         */
        fun fieldOfView(tanWidth: Float, tanHeight: Float, streamWidth: Int, streamHeight: Int, rotation: Int, viewWidth: Int, viewHeight: Int, out: FloatArray) {
            var tw = tanWidth
            var th = tanHeight
            val aspect = streamWidth.toFloat() / streamHeight
            if (aspect > tw / th) th = tw / aspect else tw = th * aspect
            val swapped = ((rotation % 360) + 360) % 180 != 0
            val tx = if (swapped) th else tw
            val ty = if (swapped) tw else th
            val dw = (if (swapped) streamHeight else streamWidth).toFloat()
            val dh = (if (swapped) streamWidth else streamHeight).toFloat()
            val scale = max(viewWidth / dw, viewHeight / dh)
            out[0] = tx * viewWidth / (dw * scale)
            out[1] = ty * viewHeight / (dh * scale)
        }
    }

    /**
     * The route ahead, ready to be projected: at most [MAX_POINTS] points (east, north, up
     * relative to the camera, in metres), the unit vector to the right of the route at each, the
     * chevrons (position, direction and distance along the route) and the next maneuver on the
     * ground. Made once per snapshot, so that drawing allocates nothing.
     */
    class Route private constructor(
        val count: Int,
        val points: FloatArray,
        val normals: FloatArray,
        val chevronCount: Int,
        val chevrons: FloatArray,
        val hasManeuver: Boolean,
        val maneuverEast: Float,
        val maneuverNorth: Float,
        val maneuverUp: Float,
    ) {
        /** Puts into `out[index * 3..]` point [k] moved [offset] metres to its right, in the camera's frame. */
        fun vertex(k: Int, offset: Float, rotation: FloatArray, out: FloatArray, index: Int) {
            Projection.toCamera(rotation, points[k * 3] + normals[k * 2] * offset, points[k * 3 + 1] + normals[k * 2 + 1] * offset, points[k * 3 + 2], out, index * 3)
        }

        companion object {
            /** The numbers of a chevron in [chevrons]: east, north, up, direction east, direction north, distance. */
            const val CHEVRON_FIELDS = 6

            val EMPTY = Route(0, FloatArray(0), FloatArray(0), 0, FloatArray(0), false, 0f, 0f, 0f)

            /** The route of [ar], thinned to [maxPoints] points, relative to the camera at the eye height. */
            fun of(ar: ArView, maxPoints: Int = MAX_POINTS): Route {
                val source = ar.points
                val n = source.size
                if (n < 2 || maxPoints < 2) return EMPTY
                val stride = (n + maxPoints - 1) / maxPoints
                val count = (n - 1) / stride + 1
                val points = FloatArray(count * 3)
                val normals = FloatArray(count * 2)
                val distances = FloatArray(count)
                var de = 0f
                var dn = 1f
                for (k in 0 until count) {
                    val i = k * stride
                    val p = source[i]
                    val previous = source[max(i - stride, 0)]
                    val next = source[min(i + stride, n - 1)]
                    val x = (next.east - previous.east).toFloat()
                    val y = (next.north - previous.north).toFloat()
                    val length = hypot(x, y)
                    if (length > 0.01f) {
                        de = x / length
                        dn = y / length
                    }
                    points[k * 3] = p.east.toFloat()
                    points[k * 3 + 1] = p.north.toFloat()
                    points[k * 3 + 2] = (p.up - ar.eyeHeightM).toFloat()
                    normals[k * 2] = dn
                    normals[k * 2 + 1] = -de
                    if (k > 0) distances[k] = distances[k - 1] + hypot(points[k * 3] - points[k * 3 - 3], points[k * 3 + 1] - points[k * 3 - 2])
                }
                val total = distances[count - 1]
                val chevronCount = if (total < CHEVRON_START) 0 else ((total - CHEVRON_START) / CHEVRON_SPACING).toInt() + 1
                val chevrons = FloatArray(chevronCount * CHEVRON_FIELDS)
                var k = 0
                for (c in 0 until chevronCount) {
                    val d = CHEVRON_START + c * CHEVRON_SPACING
                    while (k < count - 2 && distances[k + 1] < d) k++
                    val span = distances[k + 1] - distances[k]
                    val t = if (span > 0f) ((d - distances[k]) / span).coerceIn(0f, 1f) else 0f
                    val a = k * 3
                    val x = points[a + 3] - points[a]
                    val y = points[a + 4] - points[a + 1]
                    val length = hypot(x, y)
                    val base = c * CHEVRON_FIELDS
                    chevrons[base] = points[a] + x * t
                    chevrons[base + 1] = points[a + 1] + y * t
                    chevrons[base + 2] = points[a + 2] + (points[a + 5] - points[a + 2]) * t
                    chevrons[base + 3] = if (length > 0.01f) x / length else -normals[k * 2 + 1]
                    chevrons[base + 4] = if (length > 0.01f) y / length else normals[k * 2]
                    chevrons[base + 5] = d
                }
                val maneuver = ar.maneuver?.takeIf { it in 0 until n }?.let { source[it] }
                return Route(
                    count, points, normals, chevronCount, chevrons,
                    maneuver != null,
                    maneuver?.east?.toFloat() ?: 0f,
                    maneuver?.north?.toFloat() ?: 0f,
                    maneuver?.let { (it.up - ar.eyeHeightM).toFloat() } ?: 0f,
                )
            }
        }
    }

    /** The arrays and paths drawing reuses, sized by the screen's [density]. */
    class Buffers(density: Float) {
        val polygon = FloatArray(3 * 4)
        val clipped = FloatArray(3 * 5)
        val segment = FloatArray(6)
        val point = FloatArray(5)
        val ribbon = Path()
        val centre = Path()
        val chevron = Path()
        /** The arrow at the edge of the screen, pointing to +x from its origin. */
        val arrow = Path().apply {
            moveTo(18f * density, 0f)
            lineTo(-12f * density, 14f * density)
            lineTo(-6f * density, 0f)
            lineTo(-12f * density, -14f * density)
            close()
        }
        val outline = Stroke(width = 2f * density, join = StrokeJoin.Round)
        val pole = 3f * density
        val margin = 40f * density
    }

    /**
     * The state of the camera view that changes faster than the snapshot: the orientation of the
     * screen in the world (smoothed), the field of view of the picture, and what to tell the user
     * about the camera and the sensors. Everything is read and written on the main thread.
     */
    class Scene : Preview.Listener {
        /** The rotation from the screen's frame to East-North-Up (see [Projection]). */
        val matrix = FloatArray(9)
        /** Counts the orientations: reading it makes drawing follow them. */
        val version: MutableLongState = mutableLongStateOf(0L)
        /** Whether an orientation has come yet. */
        var oriented = false
            private set
        var tanX by mutableFloatStateOf(tan(PI.toFloat() / 6f))
            private set
        var tanY by mutableFloatStateOf(tan(PI.toFloat() / 8f))
            private set
        var cameraFailed by mutableStateOf(false)
            private set
        var calibrate by mutableStateOf(false)
        var noSensor by mutableStateOf(false)

        private val raw = FloatArray(9)
        private val sample = FloatArray(4)
        private val smooth = FloatArray(4)
        private val vector = FloatArray(4)

        /**
         * Takes in a rotation vector ([values] of the sensor) for the screen at
         * [displayRotation]: its quaternion is filtered (normalised linear interpolation toward
         * it by [SMOOTHING]), turned into a matrix and remapped to the screen's frame.
         */
        fun orient(values: FloatArray, displayRotation: Int) {
            SensorManager.getQuaternionFromVector(sample, values)
            if (!oriented) {
                sample.copyInto(smooth)
            } else {
                var dot = 0f
                for (i in 0 until 4) dot += sample[i] * smooth[i]
                // q and −q are the same rotation: go toward the nearer one.
                val sign = if (dot < 0f) -1f else 1f
                var norm = 0f
                for (i in 0 until 4) {
                    smooth[i] += SMOOTHING * (sign * sample[i] - smooth[i])
                    norm += smooth[i] * smooth[i]
                }
                norm = sqrt(norm)
                if (norm > 0f) for (i in 0 until 4) smooth[i] /= norm
            }
            vector[0] = smooth[1]
            vector[1] = smooth[2]
            vector[2] = smooth[3]
            vector[3] = smooth[0]
            SensorManager.getRotationMatrixFromVector(raw, vector)
            Projection.remap(raw, displayRotation, matrix)
            oriented = true
            version.longValue++
        }

        /**
         * Projects a world point onto a screen of [width] × [height]: `out[0..2]` its position in
         * the camera's frame, `out[3..4]` on the screen. False (and no screen position) when it
         * is behind the near plane.
         */
        fun project(east: Float, north: Float, up: Float, width: Float, height: Float, out: FloatArray): Boolean {
            Projection.toCamera(matrix, east, north, up, out, 0)
            val z = out[2]
            if (!Projection.visible(z)) return false
            out[3] = Projection.screenX(out[0], z, width, Projection.focal(width, tanX))
            out[4] = Projection.screenY(out[1], z, height, Projection.focal(height, tanY))
            return true
        }

        override fun onFieldOfView(tanX: Float, tanY: Float) {
            this.tanX = tanX
            this.tanY = tanY
        }

        override fun onState(working: Boolean) {
            cameraFailed = !working
        }
    }

    /**
     * The preview of the back camera with Camera2 in a [TextureView]: opened while resumed and
     * while the view has its surface, closed otherwise. The camera is driven on its own thread;
     * the view and [listener] only on the main thread. Errors never throw: they are told to
     * [listener] (`onState(false)`).
     */
    class Preview(context: Context, private var rotation: Int) : TextureView.SurfaceTextureListener {
        /** What the preview tells, on the main thread. */
        interface Listener {
            /** The field of view of the picture as shown, as tangents of its half width and height. */
            fun onFieldOfView(tanX: Float, tanY: Float)
            /** Whether the camera works (true once frames come, false after an error). */
            fun onState(working: Boolean)
        }

        val view: TextureView = TextureView(context)
        var listener: Listener? = null

        private val manager: CameraManager? = context.getSystemService(CameraManager::class.java)
        private val thread = HandlerThread("ArCamera").apply { start() }
        private val handler = Handler(thread.looper)
        private val main = Handler(Looper.getMainLooper())
        private val executor = Executor { handler.post(it) }

        // Main thread.
        private var texture: SurfaceTexture? = null
        private var width = 0
        private var height = 0
        private var resumed = false
        private var released = false
        private var opened = false
        private var openedTexture: SurfaceTexture? = null
        private var openedWidth = 0
        private var openedHeight = 0
        private var openedRotation = 0

        // Camera thread.
        private var device: CameraDevice? = null
        private var session: CameraCaptureSession? = null
        private var surface: ViewSurface? = null
        private var generation = 0

        init {
            view.surfaceTextureListener = this
        }

        fun resume() {
            resumed = true
            refresh()
        }

        fun pause() {
            resumed = false
            refresh()
        }

        /** Follows the display's rotation (`Surface.ROTATION_*`). */
        fun rotate(rotation: Int) {
            if (this.rotation == rotation) return
            this.rotation = rotation
            refresh()
        }

        /** Closes the camera and ends its thread; the preview cannot be used again. */
        fun release() {
            if (released) return
            released = true
            resumed = false
            handler.post { close() }
            // The thread ends once the texture is gone too, so that the camera is closed before
            // the texture is released (see onSurfaceTextureDestroyed).
            if (texture == null) thread.quitSafely()
        }

        override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) {
            texture = surface
            this.width = width
            this.height = height
            refresh()
        }

        override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) {
            this.width = width
            this.height = height
            refresh()
        }

        override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
            texture = null
            opened = false
            if (!handler.post { close(); surface.release() }) surface.release()
            if (released) thread.quitSafely()
            return false
        }

        override fun onSurfaceTextureUpdated(surface: SurfaceTexture) {}

        private fun refresh() {
            if (released) return
            val t = texture
            if (!resumed || t == null || width <= 0 || height <= 0) {
                if (opened) {
                    opened = false
                    handler.post { close() }
                }
                return
            }
            if (opened && openedTexture === t && openedWidth == width && openedHeight == height && openedRotation == rotation) return
            opened = true
            openedTexture = t
            openedWidth = width
            openedHeight = height
            openedRotation = rotation
            val w = width
            val h = height
            val r = rotation
            handler.post { open(t, w, h, r) }
        }

        // The permission is asked before the camera view is shown, and a refusal (taken back since)
        // throws a SecurityException that `attempt` turns into `fail`.
        @android.annotation.SuppressLint("MissingPermission")
        private fun open(texture: SurfaceTexture, width: Int, height: Int, rotation: Int) {
            close()
            val manager = manager ?: return fail()
            attempt {
                val id = manager.cameraIdList.firstOrNull { manager.getCameraCharacteristics(it).get(CameraCharacteristics.LENS_FACING) == CameraCharacteristics.LENS_FACING_BACK } ?: return fail()
                val characteristics = manager.getCameraCharacteristics(id)
                val map = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP) ?: return fail()
                val sizes = map.getOutputSizes(SurfaceTexture::class.java)
                if (sizes.isNullOrEmpty()) return fail()
                val sensorOrientation = characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 90
                val displayDegrees = degrees(rotation)
                val relative = (sensorOrientation - displayDegrees + 360) % 360
                val swapped = relative % 180 != 0
                val size = choose(sizes, if (swapped) height.toFloat() / width else width.toFloat() / height, MAX_PREVIEW)
                val tangents = FloatArray(2)
                sensorTangents(characteristics, tangents)
                val field = FloatArray(2)
                Projection.fieldOfView(tangents[0], tangents[1], size.width, size.height, relative, width, height, field)
                val matrix = transform(size, sensorOrientation, displayDegrees, width, height)
                main.post {
                    if (!released) {
                        view.setTransform(matrix)
                        listener?.onFieldOfView(field[0], field[1])
                    }
                }
                val modes = characteristics.get(CameraCharacteristics.CONTROL_AF_AVAILABLE_MODES)
                val focus = when {
                    modes?.contains(CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_VIDEO) == true -> CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_VIDEO
                    modes?.contains(CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE) == true -> CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE
                    else -> null
                }
                texture.setDefaultBufferSize(size.width, size.height)
                val token = generation
                manager.openCamera(id, object : CameraDevice.StateCallback() {
                    override fun onOpened(camera: CameraDevice) {
                        if (token != generation) {
                            camera.close()
                            return
                        }
                        device = camera
                        start(camera, texture, focus, token)
                    }

                    override fun onDisconnected(camera: CameraDevice) {
                        camera.close()
                        if (token == generation) fail()
                    }

                    override fun onError(camera: CameraDevice, error: Int) {
                        camera.close()
                        if (token == generation) fail()
                    }
                }, handler)
            }
        }

        private fun start(camera: CameraDevice, texture: SurfaceTexture, focus: Int?, token: Int) {
            attempt {
                val target = ViewSurface(texture)
                surface = target
                val callback = object : CameraCaptureSession.StateCallback() {
                    override fun onConfigured(configured: CameraCaptureSession) {
                        if (token != generation) {
                            configured.close()
                            return
                        }
                        session = configured
                        attempt {
                            val request = camera.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW).apply {
                                addTarget(target)
                                set(CaptureRequest.CONTROL_MODE, CaptureRequest.CONTROL_MODE_AUTO)
                                if (focus != null) set(CaptureRequest.CONTROL_AF_MODE, focus)
                            }.build()
                            configured.setRepeatingRequest(request, null, handler)
                            main.post { if (!released) listener?.onState(true) }
                        }
                    }

                    override fun onConfigureFailed(configured: CameraCaptureSession) {
                        configured.close()
                        if (token == generation) fail()
                    }
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    camera.createCaptureSession(SessionConfiguration(SessionConfiguration.SESSION_REGULAR, listOf(OutputConfiguration(target)), executor, callback))
                } else {
                    @Suppress("DEPRECATION")
                    camera.createCaptureSession(listOf(target), callback, handler)
                }
            }
        }

        /** Runs [block] on the camera thread, turning the errors of the camera into [fail]. */
        private inline fun attempt(block: () -> Unit) {
            try {
                block()
            } catch (e: CameraAccessException) {
                fail()
            } catch (e: SecurityException) {
                fail()
            } catch (e: IllegalArgumentException) {
                fail()
            } catch (e: IllegalStateException) {
                fail()
            }
        }

        private fun fail() {
            close()
            main.post { if (!released) listener?.onState(false) }
        }

        private fun close() {
            generation++
            session?.close()
            session = null
            device?.close()
            device = null
            surface?.release()
            surface = null
        }

        companion object {
            /** The degrees of a display rotation (`Surface.ROTATION_*`). */
            fun degrees(rotation: Int): Int = when (rotation) {
                ViewSurface.ROTATION_90 -> 90
                ViewSurface.ROTATION_180 -> 180
                ViewSurface.ROTATION_270 -> 270
                else -> 0
            }

            /**
             * The size of [sizes] (in the sensor's orientation) nearest in aspect to [aspect], of
             * at most [limit] pixels a side, the largest of those as near; the smallest of all when
             * none is that small.
             */
            fun choose(sizes: Array<Size>, aspect: Float, limit: Int): Size {
                val fitting = sizes.filter { max(it.width, it.height) <= limit }
                if (fitting.isEmpty()) return sizes.minBy { it.width.toLong() * it.height }
                fun error(size: Size): Float = abs(ln(size.width.toFloat() / size.height / aspect))
                val best = fitting.minOf { error(it) }
                return fitting.filter { error(it) <= best + 0.02f }.maxBy { it.width.toLong() * it.height }
            }

            /**
             * The tangents of half the width and half the height of what the sensor's active area
             * sees, from its focal length and physical size; 60° × 45° when the camera does not
             * tell them.
             */
            fun sensorTangents(characteristics: CameraCharacteristics, out: FloatArray) {
                val focal = characteristics.get(CameraCharacteristics.LENS_INFO_AVAILABLE_FOCAL_LENGTHS)?.firstOrNull()
                val physical = characteristics.get(CameraCharacteristics.SENSOR_INFO_PHYSICAL_SIZE)
                if (focal == null || focal <= 0f || physical == null || physical.width <= 0f || physical.height <= 0f) {
                    out[0] = tan(PI.toFloat() / 6f)
                    out[1] = tan(PI.toFloat() / 8f)
                    return
                }
                var width = physical.width
                var height = physical.height
                val pixels = characteristics.get(CameraCharacteristics.SENSOR_INFO_PIXEL_ARRAY_SIZE)
                val active = characteristics.get(CameraCharacteristics.SENSOR_INFO_ACTIVE_ARRAY_SIZE)
                if (pixels != null && active != null && pixels.width > 0 && pixels.height > 0) {
                    width *= active.width().toFloat() / pixels.width
                    height *= active.height().toFloat() / pixels.height
                }
                out[0] = width / (2f * focal)
                out[1] = height / (2f * focal)
            }

            /**
             * The transform of the view for a stream of [size] (in the sensor's orientation): the
             * texture is shown upright for the device's natural orientation and stretched to the
             * view; this gives it back its aspect, turns it against the display's rotation, and
             * scales it to fill the view, cropping the centre without distortion.
             */
            fun transform(size: Size, sensorOrientation: Int, displayDegrees: Int, width: Int, height: Int): Matrix {
                val vw = width.toFloat()
                val vh = height.toFloat()
                val cx = vw / 2f
                val cy = vh / 2f
                val natural = sensorOrientation % 180 != 0
                val bw = (if (natural) size.height else size.width).toFloat()
                val bh = (if (natural) size.width else size.height).toFloat()
                val turned = displayDegrees % 180 != 0
                val dw = if (turned) bh else bw
                val dh = if (turned) bw else bh
                val scale = max(vw / dw, vh / dh)
                return Matrix().apply {
                    setScale(bw / vw, bh / vh, cx, cy)
                    postRotate(-displayDegrees.toFloat(), cx, cy)
                    postScale(scale, scale, cx, cy)
                }
            }
        }
    }
}
