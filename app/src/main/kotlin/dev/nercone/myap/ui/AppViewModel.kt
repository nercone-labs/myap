package dev.nercone.myap.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import dev.nercone.myap.MyapApplication
import dev.nercone.myap.core.Bounds
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.Settings
import dev.nercone.myap.core.Snapshot
import kotlinx.coroutines.flow.StateFlow

/** The state of the screens: the snapshot of the core, and the inputs the screens give it. */
class AppViewModel(application: Application) : AndroidViewModel(application) {
    val host = (application as MyapApplication).host
    val snapshot: StateFlow<Snapshot?> = host.snapshot

    /** The id of the message last shown, so that a screen made again does not show it again. */
    var shownMessage: Long? = null

    /** The bounds of the map last shown, for adding the area on screen as a region. */
    var visibleBounds: Bounds? = null

    init {
        host.start()
    }

    fun now(): Long = host.now()

    fun input(make: (Long) -> Input) = host.input(make(now()))

    /**
     * The settings last sent, until the core has handled them: two changes in a row (before the
     * snapshot shows the first) each apply to the one before, and none is lost.
     */
    @Volatile private var sent: Settings? = null

    fun configure(change: (Settings) -> Settings) {
        val base = sent ?: snapshot.value?.settings ?: return
        val settings = change(base)
        sent = settings
        host.input(Input.Configure(now(), settings)).invokeOnCompletion { if (sent === settings) sent = null }
    }
}
