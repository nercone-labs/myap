package dev.nercone.myap.service

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationListener
import android.location.LocationManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import dev.nercone.myap.core.Coordinate
import dev.nercone.myap.core.Location

/**
 * The device's location from the platform's `LocationManager` (Google Play services are not
 * used): the GPS, with the fused or network provider beside it for a quick first fix indoors.
 * Precision (navigation) asks for a fix every second instead of every five. The GPS is used in
 * both: the fused provider at balanced power may give no fix at all outdoors without one, and
 * phones without Google's services may have no fused provider.
 *
 * Every call runs on the main thread (calls from other threads are posted to it), so that a start
 * and a stop from two threads never leave a provider registered. The providers are registered
 * whether they are on or off, so that turning the location on from the quick settings brings
 * fixes without leaving the application, and chosen again when the permission changes
 * (`refresh`).
 *
 * The time of each fix is taken from the clock of elapsed time (`elapsedRealtimeNanos`), the same
 * for every provider, and given as the time of the device's own clock: the GPS and the network
 * stamp their fixes with clocks of their own, and a fix stamped minutes ahead would make every
 * later one look stale to the core.
 */
class Locator(private val context: Context, private val onLocation: (Location) -> Unit) {
    private val manager = context.getSystemService(LocationManager::class.java)
    private val main = Handler(Looper.getMainLooper())

    /** The precision asked for, or `null` while stopped. */
    private var precise: Boolean? = null

    /** What is registered now: the precision, whether fine location is permitted, and the providers. */
    private var registered: Triple<Boolean, Boolean, List<String>>? = null

    // Every method is implemented: before Android 11 they have no default bodies, and a lambda
    // would end the process with an AbstractMethodError when a provider is turned on or off.
    private val listener = object : LocationListener {
        override fun onLocationChanged(location: android.location.Location) = deliver(location)

        override fun onProviderEnabled(provider: String) = refresh()

        override fun onProviderDisabled(provider: String) = refresh()

        @Deprecated("Called only before Android 10")
        override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) {}
    }

    fun permitted(): Boolean = context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED ||
        context.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED

    /** Locates with the precision asked, registering again only what changed. */
    fun start(precise: Boolean) = onMain {
        this.precise = precise
        register()
    }

    fun stop() = onMain {
        precise = null
        unregister()
    }

    /** Chooses the providers again (a provider or the permission changed). */
    fun refresh() = onMain { register() }

    private fun onMain(action: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) action() else main.post(action)
    }

    private fun unregister() {
        manager?.removeUpdates(listener)
        registered = null
    }

    @SuppressLint("MissingPermission")
    private fun register() {
        val precise = precise ?: return
        if (manager == null || !permitted()) {
            unregister()
            return
        }
        // Every provider the device has, on or off: one registered while off starts giving fixes
        // when the user turns the location on.
        val providers = manager.allProviders
        val fine = context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
        val chosen = buildList {
            if (fine && LocationManager.GPS_PROVIDER in providers) add(LocationManager.GPS_PROVIDER)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && LocationManager.FUSED_PROVIDER in providers) add(LocationManager.FUSED_PROVIDER)
            else if (LocationManager.NETWORK_PROVIDER in providers) add(LocationManager.NETWORK_PROVIDER)
        }
        val wanted = Triple(precise, fine, chosen)
        if (registered == wanted) return
        manager.removeUpdates(listener)
        registered = wanted
        val interval = if (precise) 1_000L else 5_000L
        for (provider in chosen) {
            try {
                // A last known fix only when recent: an old one may be far from where the device is.
                manager.getLastKnownLocation(provider)?.takeIf { ageMs(it) in 0..120_000 }?.let(::deliver)
                manager.requestLocationUpdates(provider, interval, 0f, listener, Looper.getMainLooper())
            } catch (error: SecurityException) {
                // The permission was taken back between the check and the request.
                unregister()
                return
            } catch (error: IllegalArgumentException) {
                // The provider went away in the meantime; the next change chooses again.
            }
        }
    }

    /** How old a fix is, by the clock of elapsed time. */
    private fun ageMs(location: android.location.Location): Long = (SystemClock.elapsedRealtimeNanos() - location.elapsedRealtimeNanos) / 1_000_000

    private fun deliver(location: android.location.Location) {
        onLocation(
            Location(
                coordinate = Coordinate(location.latitude, location.longitude),
                accuracyM = if (location.hasAccuracy()) location.accuracy.toDouble() else null,
                bearing = if (location.hasBearing()) location.bearing.toDouble() else null,
                speedMps = if (location.hasSpeed()) location.speed.toDouble() else null,
                timeMs = System.currentTimeMillis() - ageMs(location).coerceAtLeast(0),
            ),
        )
    }
}
