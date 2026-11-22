package dev.nercone.myap.core

/**
 * The Rust core (`crates/myap-ffi`, the library `libmyap_ffi.so`), through the Java Native
 * Interface. Inputs and responses are UTF-8 JSON (`myap::protocol`). A core is not thread-safe:
 * `Core` serialises the calls. `buildGraph` holds no state and may run on any thread.
 */
object NativeCore {
    init {
        System.loadLibrary("myap_ffi")
    }

    @JvmStatic external fun create(): Long

    /** The response to `input`; `null` only when the JVM could not pass the arrays. */
    @JvmStatic external fun handle(core: Long, input: ByteArray): ByteArray?

    /** Builds a graph of Myap Router (`myap::GraphBuild`) and answers with its result. */
    @JvmStatic external fun buildGraph(input: ByteArray): ByteArray?

    @JvmStatic external fun destroy(core: Long)
}
