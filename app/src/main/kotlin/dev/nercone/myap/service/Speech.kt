package dev.nercone.myap.service

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.util.Log
import java.util.Locale

/**
 * The voice of the guidance, with the platform's `TextToSpeech`. Every call runs on the main
 * thread, in order. Only the latest announcement waits for the engine to start (an older one is
 * stale by then). An engine that fails to start, or fails to speak (its service died, or it was
 * updated), is let go and made again for the next announcement, so that the voice is never lost
 * for the rest of the process.
 */
class Speech(private val context: Context) {
    private val main = Handler(Looper.getMainLooper())
    private var engine: TextToSpeech? = null
    private var ready = false
    private var waiting: Pair<String, String>? = null
    /** Whether the engine was just made again after failing to speak: it is not made again twice in a row. */
    private var remade = false

    fun say(text: String, language: String) {
        main.post { speakOrWait(text, language) }
    }

    private fun speakOrWait(text: String, language: String) {
        val current = engine
        if (current != null && ready) {
            speak(current, text, language)
            return
        }
        waiting = text to language
        if (current == null) start()
    }

    private fun start() {
        ready = false
        var made: TextToSpeech? = null
        made = TextToSpeech(context) { status ->
            main.post {
                if (engine !== made) return@post
                if (status == TextToSpeech.SUCCESS) {
                    ready = true
                    val pending = waiting
                    waiting = null
                    pending?.let { (text, language) -> made?.let { speak(it, text, language) } }
                    return@post
                } else {
                    Log.w(Host.TAG, "speech: the engine did not start ($status)")
                    release()
                }
                waiting = null
            }
        }
        engine = made
    }

    private fun speak(current: TextToSpeech, text: String, language: String) {
        val locale = Locale.forLanguageTag(language)
        val available = current.isLanguageAvailable(locale)
        // A language the engine lacks is spoken in its default voice rather than not at all.
        if (available >= TextToSpeech.LANG_AVAILABLE) current.language = locale
        if (current.speak(text, TextToSpeech.QUEUE_FLUSH, null, "myap") != TextToSpeech.ERROR) {
            remade = false
            return
        }
        Log.w(Host.TAG, "speech: the engine failed to speak")
        release()
        if (!remade) {
            remade = true
            waiting = text to language
            start()
        }
    }

    private fun release() {
        engine?.shutdown()
        engine = null
        ready = false
    }
}
