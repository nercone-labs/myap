package dev.nercone.myap.core

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * One instance of the Rust core. Every call runs on one thread (`dispatcher`), in order, so the
 * core, which is not thread-safe, is never entered twice at once.
 */
class Core(private val dispatcher: CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1)) {
    private val handle: Long = NativeCore.create()

    /** Hands `input` to the core. A response that cannot be read becomes an error response. */
    suspend fun handle(input: Input): Response = withContext(dispatcher) {
        val text = CoreJson.json.encodeToString(Input.serializer(), input)
        val bytes = NativeCore.handle(handle, text.toByteArray(Charsets.UTF_8)) ?: return@withContext Response(error = "the core returned nothing")
        decode(bytes.toString(Charsets.UTF_8))
    }

    companion object {
        /** Runs a graph build on the calling thread, and answers with the JSON of its result. */
        fun buildGraph(build: GraphBuild): String {
            val input = CoreJson.json.encodeToString(GraphBuild.serializer(), build).toByteArray(Charsets.UTF_8)
            return NativeCore.buildGraph(input)?.toString(Charsets.UTF_8) ?: """{"Err":"the core returned nothing"}"""
        }

        /**
         * Reads a response. Each effect is read on its own and the snapshot apart from them, so
         * that a part this version cannot read never costs the others.
         */
        fun decode(text: String): Response {
            val root = try {
                CoreJson.json.parseToJsonElement(text) as? JsonObject
            } catch (error: SerializationException) {
                null
            } ?: return Response(error = "unreadable response of the core")
            val problems = mutableListOf<String>()
            fun <T> part(name: String, read: () -> T?): T? = try {
                read()
            } catch (error: SerializationException) {
                problems += "$name: ${error.message}"
                null
            } catch (error: IllegalArgumentException) {
                problems += "$name: ${error.message}"
                null
            }
            val effects = (root["effects"] as? JsonArray).orEmpty().mapNotNull { element ->
                part("effect") { CoreJson.json.decodeFromJsonElement(Effect.serializer(), element) }
            }
            val snapshot = root["snapshot"]?.takeIf { it !is JsonNull }?.let { part("snapshot") { CoreJson.json.decodeFromJsonElement(Snapshot.serializer(), it) } }
            val error = (root["error"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            return Response(effects, snapshot, listOfNotNull(error, problems.takeIf { it.isNotEmpty() }?.joinToString("; ")).joinToString("; ").ifEmpty { null })
        }
    }
}

data class Response(val effects: List<Effect> = emptyList(), val snapshot: Snapshot? = null, val error: String? = null)
