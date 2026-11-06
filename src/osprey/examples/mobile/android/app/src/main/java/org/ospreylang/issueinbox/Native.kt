package org.ospreylang.issueinbox

// Byte arrays preserve standard UTF-8, including emoji; JNI strings use Modified UTF-8.
internal object Native {
    init { System.loadLibrary("osprey_inbox") }
    external fun start(): ByteArray
    external fun dispatch(model: ByteArray, event: ByteArray): ByteArray
}
