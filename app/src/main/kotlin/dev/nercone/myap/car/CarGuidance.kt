package dev.nercone.myap.car

import androidx.car.app.CarContext
import androidx.car.app.navigation.NavigationManager
import androidx.car.app.navigation.NavigationManagerCallback
import androidx.car.app.navigation.model.Destination
import androidx.car.app.navigation.model.TravelEstimate
import androidx.car.app.navigation.model.Trip
import androidx.car.app.model.DateTimeWithZone
import android.util.Log
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.coroutineScope
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.NavigationState
import dev.nercone.myap.core.NavigationView
import dev.nercone.myap.service.Host
import dev.nercone.myap.service.Locator
import java.util.TimeZone
import kotlin.math.roundToLong
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * The guidance as the car knows it, for the whole of a session: the host is told when it starts
 * and ends (so that it can show it on the instrument cluster and stop another application's), and
 * of each change of the trip; when the car asks to stop (another application starts to navigate),
 * the guidance stops. The location is asked for the car while it is connected, precisely during
 * the guidance.
 */
class CarGuidance(private val carContext: CarContext, private val host: Host, lifecycle: Lifecycle) : DefaultLifecycleObserver {
    private val manager = carContext.getCarService(NavigationManager::class.java)
    private var navigating = false
    private var updates: Job? = null

    init {
        manager.setNavigationManagerCallback(object : NavigationManagerCallback {
            override fun onStopNavigation() {
                host.input(Input.StopNavigation(host.now()))
            }
        })
        lifecycle.addObserver(this)
        updates = lifecycle.coroutineScope.launch {
            host.snapshot.map { it?.navigation to (it?.imperial ?: false) }.distinctUntilChanged().collect { (nav, imperial) -> changed(nav, imperial) }
        }
    }

    override fun onStart(owner: LifecycleOwner) = locate()

    override fun onStop(owner: LifecycleOwner) = host.stopLocating(Host.Locating.CAR)

    override fun onDestroy(owner: LifecycleOwner) {
        updates?.cancel()
        if (navigating) end()
        manager.clearNavigationManagerCallback()
    }

    /** Asks for the location of the car's screens, when it is allowed. */
    fun locate() {
        if (Locator(carContext) {}.permitted()) host.locate(Host.Locating.CAR, precise = navigating)
    }

    private fun changed(nav: NavigationView?, imperial: Boolean) {
        if (nav != null && !navigating) {
            try {
                manager.navigationStarted()
                navigating = true
                locate()
            } catch (error: IllegalStateException) {
                Log.w(Host.TAG, "car: navigation not started", error)
            }
        } else if (nav == null && navigating) {
            end()
            locate()
        }
        if (nav != null && navigating) manager.updateTrip(trip(nav, imperial))
    }

    private fun end() {
        navigating = false
        try {
            manager.navigationEnded()
        } catch (error: IllegalStateException) {
            Log.w(Host.TAG, "car: navigation not ended", error)
        }
    }

    /** The trip for the host's own displays: the next step, and the destination. */
    private fun trip(nav: NavigationView, imperial: Boolean): Trip {
        val trip = Trip.Builder().setLoading(nav.state == NavigationState.REROUTING)
        nav.road?.let(trip::setCurrentRoad)
        val destination = CoreScreen.estimate(nav, imperial)
        if (nav.state == NavigationState.NAVIGATING) {
            // The time to the step, by the share of the distance left.
            val share = if (nav.remainingM > 0) (nav.distanceM / nav.remainingM).coerceIn(0.0, 1.0) else 0.0
            val seconds = (nav.remainingS * share).coerceAtLeast(0.0).roundToLong()
            val toStep = TravelEstimate.Builder(CoreScreen.distance(nav.distanceM, imperial), DateTimeWithZone.create(host.now() + seconds * 1_000, TimeZone.getDefault()))
                .setRemainingTimeSeconds(seconds)
                .build()
            trip.addStep(CoreScreen.step(carContext, nav), toStep)
        }
        val name = host.snapshot.value?.directions?.destination?.takeIf { it.isNotBlank() } ?: nav.instruction
        trip.addDestination(Destination.Builder().setName(name).build(), destination)
        return trip.build()
    }
}
