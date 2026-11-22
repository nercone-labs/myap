package dev.nercone.myap.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import dev.nercone.myap.MyapApplication
import dev.nercone.myap.R
import dev.nercone.myap.core.NavigationView
import dev.nercone.myap.ui.MainActivity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * Keeps the guidance going with the screen off: a foreground service of type location, with
 * precise location, whose notification shows the next maneuver. It runs only while navigating.
 */
class NavigationService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    override fun onBind(intent: Intent?): IBinder? = null

    private var updates: kotlinx.coroutines.Job? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val host = (application as MyapApplication).host
        if (intent?.action == ACTION_STOP) {
            // "Stop" of the notification: the core ends the guidance, and stops this service.
            host.input(dev.nercone.myap.core.Input.StopNavigation(host.now()))
            return START_NOT_STICKY
        }
        // A service of type location may not start without the permission (Android 14 and later).
        val permitted = checkSelfPermission(android.Manifest.permission.ACCESS_FINE_LOCATION) == android.content.pm.PackageManager.PERMISSION_GRANTED ||
            checkSelfPermission(android.Manifest.permission.ACCESS_COARSE_LOCATION) == android.content.pm.PackageManager.PERMISSION_GRANTED
        if (!permitted) {
            stopSelf()
            return START_NOT_STICKY
        }
        val notification = notification(host.snapshot.value?.navigation)
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION)
            } else {
                startForeground(ID, notification)
            }
        } catch (error: RuntimeException) {
            // Refused (the permission went away, or the application is in the background).
            android.util.Log.w(Host.TAG, "the guidance service cannot be in the foreground", error)
            stopSelf()
            return START_NOT_STICKY
        }
        host.locate(Host.Locating.GUIDANCE, precise = true)
        updates?.cancel()
        updates = scope.launch {
            host.snapshot.map { it?.navigation }.distinctUntilChanged { a, b -> a?.instruction == b?.instruction && a?.distance == b?.distance && a?.state == b?.state }.collect { nav ->
                getSystemService(NotificationManager::class.java).notify(ID, notification(nav))
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private fun notification(nav: NavigationView?): Notification {
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE)
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(nav?.let { "${it.distance} · ${it.instruction}" } ?: getString(R.string.navigation_running))
            .setContentText(nav?.let { getString(R.string.navigation_remaining, it.remainingTime, it.remainingDistance, it.arrival) })
            .setContentIntent(open)
            .addAction(
                Notification.Action.Builder(
                    android.graphics.drawable.Icon.createWithResource(this, R.drawable.ic_notification),
                    getString(R.string.end_navigation),
                    PendingIntent.getService(this, 1, Intent(this, NavigationService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE),
                ).build(),
            )
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_NAVIGATION)
            .build()
    }

    companion object {
        const val CHANNEL = "navigation"
        const val ACTION_STOP = "dev.nercone.myap.action.STOP_NAVIGATION"
        const val ID = 1

        fun createChannel(context: Context) {
            val channel = NotificationChannel(CHANNEL, context.getString(R.string.channel_navigation), NotificationManager.IMPORTANCE_LOW)
            context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }

        /**
         * Starts the service, when it may run: a service of type location may not start without
         * the permission (it would be ended for not calling `startForeground` in time), nor, from
         * Android 12, while the application is in the background. Guidance goes on without it
         * while the screens are shown (or the car's), only not with the screen off.
         */
        fun start(context: Context) {
            if (!Locator(context) {}.permitted()) return
            try {
                context.startForegroundService(Intent(context, NavigationService::class.java))
            } catch (error: IllegalStateException) {
                android.util.Log.w(dev.nercone.myap.service.Host.TAG, "the guidance service cannot start now", error)
            } catch (error: SecurityException) {
                android.util.Log.w(dev.nercone.myap.service.Host.TAG, "the guidance service cannot start", error)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, NavigationService::class.java))
            (context.applicationContext as MyapApplication).host.stopLocating(Host.Locating.GUIDANCE)
        }
    }
}
