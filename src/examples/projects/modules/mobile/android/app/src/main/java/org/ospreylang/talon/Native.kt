package org.ospreylang.talon

/** Standard UTF-8 bytes avoid JNI modified UTF-8 corrupting names and emoji. */
internal object Native {
    init { System.loadLibrary("osprey_talon") }
    private external fun startBytes(): ByteArray
    private external fun dispatchBytes(payload: ByteArray): ByteArray
    @Synchronized fun start(): String = startBytes().toString(Charsets.UTF_8)
    @Synchronized fun dispatch(payload: String): String = dispatchBytes(payload.toByteArray(Charsets.UTF_8)).toString(Charsets.UTF_8)
}
