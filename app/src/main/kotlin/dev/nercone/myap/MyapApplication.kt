package dev.nercone.myap

import android.app.Application
import dev.nercone.myap.service.Host
import dev.nercone.myap.service.NavigationService

class MyapApplication : Application() {
    lateinit var host: Host
        private set

    override fun onCreate() {
        super.onCreate()
        NavigationService.createChannel(this)
        host = Host(this)
    }
}
