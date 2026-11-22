package dev.nercone.myap.car

import android.content.Intent
import androidx.car.app.CarAppService
import androidx.car.app.CarContext
import androidx.car.app.Screen
import androidx.car.app.ScreenManager
import androidx.car.app.Session
import androidx.car.app.validation.HostValidator
import dev.nercone.myap.BuildConfig
import dev.nercone.myap.MyapApplication
import dev.nercone.myap.core.Input
import dev.nercone.myap.service.Host
import dev.nercone.myap.ui.GeoLink
import dev.nercone.myap.ui.MapPane

/**
 * Myap in Android Auto: an application of the navigation kind of the Car App Library. The car's
 * screens are templates the host draws; the map is Myap's own, drawn by MapLibre on the surface
 * the host gives (`CarMap`). Everything else is the same core as on the phone, through the same
 * host: the car's screens are a third interface of the core, besides the phone's and the guidance
 * service.
 */
class MyapCarAppService : CarAppService() {
    /**
     * Which hosts may bind: any while debugging (the Desktop Head Unit), else those of the Car
     * App Library's list of known hosts (Android Auto and the hosts of car makers).
     */
    override fun createHostValidator(): HostValidator = if (BuildConfig.DEBUG) {
        HostValidator.ALLOW_ALL_HOSTS_VALIDATOR
    } else {
        HostValidator.Builder(applicationContext).addAllowedHosts(androidx.car.app.R.array.hosts_allowlist_sample).build()
    }

    override fun onCreateSession(): Session = MyapCarSession()
}

/** One connection to a car: its map and its screens. */
class MyapCarSession : Session() {
    private lateinit var host: Host
    private lateinit var map: CarMap

    override fun onCreateScreen(intent: Intent): Screen {
        host = (carContext.applicationContext as MyapApplication).host
        host.start()
        MapPane.initialize(carContext, host.http)
        map = CarMap(carContext, host, lifecycle)
        val guidance = CarGuidance(carContext, host, lifecycle)
        val root = MapCarScreen(carContext, host, map, guidance)
        val search = navigateQuery(intent) ?: return root
        // Asked by the car to go somewhere ("navigate to…" by voice): the search of it, over the map.
        carContext.getCarService(ScreenManager::class.java).push(root)
        return searchFor(search)
    }

    override fun onNewIntent(intent: Intent) {
        val search = navigateQuery(intent) ?: return
        val screens = carContext.getCarService(ScreenManager::class.java)
        screens.popToRoot()
        screens.push(searchFor(search))
    }

    private fun searchFor(query: String): Screen {
        host.input(Input.Search(host.now(), query))
        return PlacesCarScreen(carContext, host, PlacesCarScreen.Kind.RESULTS)
    }

    /** The search of a navigation intent of the car (`geo:` URI), if it is one. */
    private fun navigateQuery(intent: Intent): String? {
        if (intent.action != CarContext.ACTION_NAVIGATE) return null
        return intent.data?.let(GeoLink::query)
    }
}
