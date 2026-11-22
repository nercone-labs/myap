package dev.nercone.myap.ui

import android.content.Intent
import android.content.res.Configuration
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.nercone.myap.R
import dev.nercone.myap.core.Input
import dev.nercone.myap.service.Host
import dev.nercone.myap.service.Locator
import kotlinx.coroutines.flow.MutableStateFlow

class MainActivity : ComponentActivity() {
    val model: AppViewModel by viewModels()

    /** A search or a place a link (`geo:`) asked to open. */
    private val link = MutableStateFlow<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        MapPane.initialize(this, model.host.http)
        model.host.configurationChanged(resources.configuration)
        link.value = intent?.data?.let(GeoLink::query)
        setContent { MyapTheme.Content { MyapApp.Content(model, link) } }
    }

    override fun onStart() {
        super.onStart()
        model.host.screenShown = true
        if (Locator(this) {}.permitted()) model.host.locate(Host.Locating.SCREEN, precise = model.snapshot.value?.navigation != null)
    }

    override fun onStop() {
        super.onStop()
        model.host.screenShown = false
        // The guidance keeps locating (in its service, which may not have been allowed to start);
        // otherwise the location of the phone's screen stops with it (the car's goes on for itself).
        if (model.snapshot.value?.navigation == null) model.host.stopLocating(Host.Locating.SCREEN)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.data?.let(GeoLink::query)?.let { link.value = it }
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        model.host.configurationChanged(newConfig)
    }
}

/** The application: the map, or the settings over it. */
object MyapApp {
    @Composable
    fun Content(model: AppViewModel, link: MutableStateFlow<String?>) {
        val snapshot by model.snapshot.collectAsStateWithLifecycle()
        val opened by link.collectAsStateWithLifecycle()
        var page by rememberSaveable { mutableStateOf<SettingsScreen.Page?>(null) }
        var settings by rememberSaveable { mutableStateOf(false) }
        LaunchedEffect(opened) {
            opened?.let { query ->
                settings = false
                model.input { now -> Input.Search(now, query) }
                link.value = null
            }
        }
        val current = snapshot ?: return
        // The map stays composed under the settings, so that its view, its camera and the state
        // of the map screen (following, a pending start) are kept while the settings are open.
        // Hidden from screen readers then; the settings' surface takes the touches.
        Box(Modifier.fillMaxSize().then(if (settings) Modifier.clearAndSetSemantics {} else Modifier)) {
            MapScreen.Content(current, model, onSettings = { target ->
                page = target
                settings = true
            })
        }
        if (settings) {
            SettingsScreen.Content(current, model, page, onOpen = { page = it }, onBack = { if (page != null) page = null else settings = false })
        }
        // After the map screen, so that it is asked first while the settings are open.
        BackHandler(enabled = settings) {
            if (page != null) page = null else settings = false
        }
        if (!current.settings.onboarded) {
            Onboarding(onGoogle = {
                model.configure { it.copy(onboarded = true) }
                page = SettingsScreen.Page.GOOGLE
                settings = true
            }, onDone = { model.configure { it.copy(onboarded = true) } })
        }
    }

    /** The first start: Myap works without Google, and asks before using it. */
    @Composable
    fun Onboarding(onGoogle: () -> Unit, onDone: () -> Unit) {
        AlertDialog(
            onDismissRequest = {},
            title = { Text(stringResource(R.string.onboarding_title)) },
            text = { Text(stringResource(R.string.onboarding_text)) },
            confirmButton = { TextButton(onClick = onDone) { Text(stringResource(R.string.onboarding_without_google)) } },
            dismissButton = { TextButton(onClick = onGoogle) { Text(stringResource(R.string.onboarding_google_settings)) } },
        )
    }
}
