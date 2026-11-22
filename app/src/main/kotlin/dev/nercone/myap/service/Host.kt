package dev.nercone.myap.service

import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.util.Log
import dev.nercone.myap.core.Core
import dev.nercone.myap.core.CoreJson
import dev.nercone.myap.core.Effect
import dev.nercone.myap.core.GraphBuild
import dev.nercone.myap.core.HttpRequest
import dev.nercone.myap.core.HttpResponse
import dev.nercone.myap.core.Input
import dev.nercone.myap.core.Location
import dev.nercone.myap.core.LogLevel
import dev.nercone.myap.core.Response
import dev.nercone.myap.core.Snapshot
import java.io.File
import java.io.IOException
import java.util.TimeZone
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

/**
 * Carries out what the core asks (`Effect`) and tells it what happens (`Input`): the platform half
 * of the core, shared by the screens and the navigation service of this process.
 *
 * The core runs on one thread (`dispatcher`); the callbacks of OkHttp, the builds and the
 * locations are handed over to it, so that the core sees them in order.
 */
class Host(private val context: Context) {
    private val dispatcher = Dispatchers.IO.limitedParallelism(1)
    // An unexpected exception is logged and the host carries on: it must never end the process in
    // the middle of a navigation.
    private val scope = CoroutineScope(SupervisorJob() + dispatcher + CoroutineExceptionHandler { _, error -> Log.e(TAG, "host", error) })
    private val core = Core(dispatcher)

    /** No cookies are kept by the client: the core keeps Google's session itself, in memory. */
    val http: OkHttpClient = OkHttpClient.Builder().connectTimeout(15, TimeUnit.SECONDS).readTimeout(30, TimeUnit.SECONDS).build()
    private val calls = mutableMapOf<Long, Call>()
    private var wake: Job? = null
    private var started = false
    /** The builds of graphs, one at a time, away from the thread of the core. */
    private val builds = CoroutineScope(SupervisorJob() + Dispatchers.Default.limitedParallelism(1) + CoroutineExceptionHandler { _, error -> Log.e(TAG, "build", error) })
    private val speech = Speech(context)
    private val locator = Locator(context) { location -> input(Input.Located(now(), location)) }

    private val _snapshot = MutableStateFlow<Snapshot?>(null)
    val snapshot: StateFlow<Snapshot?> = _snapshot.asStateFlow()

    fun now(): Long = System.currentTimeMillis()

    /** Starts the core once; later calls do nothing. */
    fun start() = scope.launch {
        if (started) return@launch
        started = true
        val directory = context.filesDir.resolve("core").apply { mkdirs() }
        handle(Input.Start(now(), directory.absolutePath, locale(), offsetMinutes()))
        handle(Input.Appearance(now(), dark()))
    }

    /** Hands an input to the core and carries out its answer. */
    fun input(input: Input) = scope.launch { handle(input) }

    /**
     * The locale or the appearance changed, as `configuration` (of an activity) says: an activity's
     * configuration follows the language chosen for the application, which the application's
     * own context does not until the process starts again.
     */
    fun configurationChanged(configuration: Configuration) {
        input(Input.Locale(now(), configuration.locales.get(0).toLanguageTag(), offsetMinutes()))
        input(Input.Appearance(now(), (configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES))
    }

    /** What needs the location: the phone's screens, the car's, the guidance service. */
    enum class Locating { SCREEN, CAR, GUIDANCE }

    /** The precision each user of the location asks for. */
    private val locating = mutableMapOf<Locating, Boolean>()

    /**
     * Locates the device for `user` with the precision asked. The device is located while any
     * user needs it, precisely when any asks for it, so that one user stopping (the phone's
     * screen turned off) never stops the location of another (the car's screen).
     */
    fun locate(user: Locating, precise: Boolean) {
        synchronized(locating) { locating[user] = precise }
        applyLocating()
    }

    fun stopLocating(user: Locating) {
        synchronized(locating) { locating.remove(user) }
        applyLocating()
    }

    /** Whether the phone's screens are shown (between the activity's start and stop). */
    @Volatile var screenShown = false

    /**
     * The guidance ended: the screens no longer need precision, and a screen left during the
     * guidance (which kept its location for it) no longer needs the location at all.
     */
    private fun guidanceEnded() {
        synchronized(locating) {
            if (locating.containsKey(Locating.SCREEN)) {
                if (screenShown) locating[Locating.SCREEN] = false else locating.remove(Locating.SCREEN)
            }
            if (locating.containsKey(Locating.CAR)) locating[Locating.CAR] = false
        }
        applyLocating()
    }

    /** The location permission may have changed: the providers are chosen again. */
    fun locationPermissionChanged() = locator.refresh()

    private fun applyLocating() {
        val wanted = synchronized(locating) { if (locating.isEmpty()) null else locating.values.any { it } }
        if (wanted == null) locator.stop() else locator.start(wanted)
    }

    private suspend fun handle(input: Input): Response {
        val response = core.handle(input)
        response.error?.let { Log.e(TAG, "core: $it") }
        response.effects.forEach { effect ->
            try {
                execute(effect)
            } catch (error: RuntimeException) {
                Log.e(TAG, "effect $effect", error)
            }
        }
        response.snapshot?.let { _snapshot.value = it }
        return response
    }

    private fun execute(effect: Effect) {
        when (effect) {
            is Effect.Fetch -> fetch(effect.id, effect.request)
            is Effect.Cancel -> calls.remove(effect.id)?.cancel()
            is Effect.BuildGraph -> build(effect)
            is Effect.Speak -> speak(effect.text, effect.language)
            is Effect.Wake -> schedule(effect.at)
            is Effect.Navigating -> if (effect.active) {
                NavigationService.start(context)
            } else {
                NavigationService.stop(context)
                guidanceEnded()
            }
            is Effect.Log -> when (effect.level) {
                LogLevel.INFO -> Log.i(TAG, effect.message)
                LogLevel.WARNING -> Log.w(TAG, effect.message)
                LogLevel.ERROR -> Log.e(TAG, effect.message)
            }
        }
    }

    private fun fetch(id: Long, request: HttpRequest) {
        // A URL or header OkHttp refuses is answered as a failure, so that nothing waits for it.
        val call = try {
            val builder = Request.Builder().url(request.url)
            request.headers.forEach { pair -> if (pair.size == 2) builder.addHeader(pair[0], pair[1]) }
            if (request.method == "POST") {
                val type = request.headers.firstOrNull { it.firstOrNull().equals("Content-Type", ignoreCase = true) }?.getOrNull(1)
                builder.post((request.body ?: "").toRequestBody(type?.toMediaTypeOrNull()))
            }
            http.newBuilder().callTimeout(request.timeoutMs, TimeUnit.MILLISECONDS).build().newCall(builder.build())
        } catch (error: IllegalArgumentException) {
            input(Input.Fetched(now(), id, HttpResponse(0, error = error.message ?: "invalid request")))
            return
        }
        calls[id] = call
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                request.saveTo?.let { File("$it.part").delete() }
                scope.launch {
                    if (calls.remove(id) != null) handle(Input.Fetched(now(), id, HttpResponse(0, error = e.message ?: e.javaClass.simpleName)))
                }
            }

            override fun onResponse(call: Call, response: okhttp3.Response) {
                val answer = try {
                    response.use { r ->
                        // Only the cookies are of use to the core among the headers.
                        val headers = r.headers.filter { it.first.equals("Set-Cookie", ignoreCase = true) }.map { listOf(it.first, it.second) }
                        val saveTo = request.saveTo
                        if (saveTo != null && r.isSuccessful) {
                            // Large downloads go to a file, written whole before it replaces any old one.
                            val target = File(saveTo)
                            val partial = File("$saveTo.part")
                            try {
                                r.body.byteStream().use { input -> partial.outputStream().use { output -> input.copyTo(output) } }
                                if (!partial.renameTo(target)) throw IOException("cannot write $saveTo")
                            } finally {
                                partial.delete()
                            }
                            HttpResponse(r.code, headers = headers)
                        } else {
                            // A text answer larger than any the core reads is refused, not held in memory.
                            val length = r.body.contentLength()
                            if (length > MAX_BODY_BYTES) throw IOException("response of $length bytes")
                            val bytes = r.body.source().apply { request(MAX_BODY_BYTES + 1) }.buffer
                            if (bytes.size > MAX_BODY_BYTES) throw IOException("response larger than $MAX_BODY_BYTES bytes")
                            HttpResponse(r.code, r.body.string(), headers)
                        }
                    }
                } catch (e: IOException) {
                    HttpResponse(0, error = e.message ?: e.javaClass.simpleName)
                } catch (e: RuntimeException) {
                    // Anything else thrown here would end the process on OkHttp's thread, and the core
                    // would wait for this answer for ever.
                    Log.e(TAG, "response", e)
                    HttpResponse(0, error = e.javaClass.simpleName)
                } catch (e: OutOfMemoryError) {
                    request.saveTo?.let { File("$it.part").delete() }
                    HttpResponse(0, error = "out of memory")
                }
                scope.launch { if (calls.remove(id) != null) handle(Input.Fetched(now(), id, answer)) }
            }
        })
    }

    /**
     * Builds a graph on a thread of its own: it can take minutes, and the core goes on meanwhile.
     * One build at a time: each can take much memory.
     */
    private fun build(effect: Effect.BuildGraph) {
        builds.launch {
            val result = try {
                Core.buildGraph(GraphBuild(effect.input, effect.format, effect.output, effect.elevation))
            } catch (error: RuntimeException) {
                """{"Err":"${error.message?.replace("\"", "'")}"}"""
            }
            val element = try {
                CoreJson.json.parseToJsonElement(result)
            } catch (error: RuntimeException) {
                CoreJson.json.parseToJsonElement("""{"Err":"unreadable result"}""")
            }
            input(Input.GraphBuilt(now(), effect.id, element))
        }
    }

    private fun speak(text: String, language: String) = speech.say(text, language)

    /**
     * Copies a PBF file the user chose where the core can read it, then adds it as a region. The
     * copy runs here, not on a screen, so that leaving the screen never leaves half a copy; a copy
     * that fails is removed. The core removes the copy once it is built.
     */
    fun importRegion(uri: android.net.Uri, name: String) = scope.launch {
        val target = File(context.filesDir, "core/downloads/import-${now()}.osm.pbf")
        val copied = withContext(Dispatchers.IO) {
            try {
                target.parentFile?.mkdirs()
                context.contentResolver.openInputStream(uri)?.use { input -> target.outputStream().use { input.copyTo(it) } } != null
            } catch (error: IOException) {
                Log.w(TAG, "import", error)
                false
            } catch (error: SecurityException) {
                // The provider of the file took back the permission to read it.
                Log.w(TAG, "import", error)
                false
            }.also { ok -> if (!ok) target.delete() }
        }
        if (copied) handle(Input.AddRegionFile(now(), name, target.absolutePath))
    }

    private fun schedule(at: Long) {
        wake?.cancel()
        wake = scope.launch {
            delay((at - now()).coerceAtLeast(0))
            handle(Input.Tick(now()))
        }
    }

    /** The device's language and region, as a BCP 47 tag. */
    private fun locale(): String = context.resources.configuration.locales.get(0).toLanguageTag()

    private fun offsetMinutes(): Int = TimeZone.getDefault().getOffset(now()) / 60_000

    private fun dark(): Boolean = (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    companion object {
        const val TAG = "Myap"
        /** The largest text response read (Overpass and PBF downloads go to files instead). */
        const val MAX_BODY_BYTES = 32L * 1024 * 1024
        /** The honest identity of Myap, for the services of OpenStreetMap and the map tiles. */
        val USER_AGENT = "Myap/${dev.nercone.myap.BuildConfig.VERSION_NAME} (+https://github.com/nercone-labs/myap; Android ${Build.VERSION.RELEASE})"
    }
}
