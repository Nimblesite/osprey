package org.ospreylang.host

import java.nio.ByteBuffer

// Implements [NATIVE-SHM-OWNERSHIP] and [NATIVE-SHM-PUBLISH] on Kotlin's side:
// the externs of the JNI surface bridge.c owns, and the frame lifetime. Every
// export call happens on one host thread, commits its frame, and hands back
// the four sealed runs as direct buffers over the leased bytes; the leases
// are released the moment the frame has been read, so no buffer outlives it.

object Bridge {
    init {
        System.loadLibrary("osprey_host")
    }

    external fun openBridge(): Long
    external fun mainEntry()
    external fun argument(handle: Long, text: ByteArray): Array<ByteBuffer>
    external fun openApp(handle: Long, now: Long, offset: Long, platform: ByteArray,
                         home: ByteArray, dark: Boolean): Array<ByteBuffer>
    external fun event(handle: Long, now: Long, kind: Long, ordinal: Long, words: ByteArray,
                       detail: ByteArray, x: Double, y: Double, down: Boolean,
                       shift: Boolean): Array<ByteBuffer>
    external fun releaseFrame(handle: Long)
    external fun closeBridge(handle: Long)
    external fun sliceWrite(key: ByteArray, text: ByteArray)
    external fun sliceText(key: ByteArray): ByteArray

    /** Keeps `text` under `key` in the host's slice table. */
    fun write(key: String, text: String) = sliceWrite(key.toByteArray(Charsets.UTF_8), text.toByteArray(Charsets.UTF_8))

    /** The bytes the slice table holds under `key`, empty when it holds none. */
    fun text(key: String): ByteArray = sliceText(key.toByteArray(Charsets.UTF_8))

    /** Reads one frame's four runs and releases its leases, whatever happens. */
    fun <R> read(handle: Long, runs: Array<ByteBuffer>, check: (Array<ByteBuffer>) -> R): R {
        try {
            return check(runs)
        } finally {
            releaseFrame(handle)
        }
    }
}
