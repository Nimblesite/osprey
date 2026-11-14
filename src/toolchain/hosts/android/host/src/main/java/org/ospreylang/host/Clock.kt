package org.ospreylang.host

import android.view.Choreographer

// Implements [UI-SCENE]'s frame half: the choreographer callback an `animate`
// ask turns on. While it runs, every display refresh reaches the application
// as a `frame` event, `elapsed` seconds after the last — the first 1/60th of a
// second, and never more than a tenth, so a window the system paused cannot
// deliver a jump the application did not ask for.

class Clock(private val tick: (Double) -> Unit) {
    var running = false
        set(on) {
            field = on
            if (on) start() else stop()
        }
    private var last = 0.0
    private val choreographer get() = Choreographer.getInstance()
    private val callback = Choreographer.FrameCallback { frame() }

    private fun start() {
        last = 0.0
        choreographer.postFrameCallback(callback)
    }

    private fun stop() {
        choreographer.removeFrameCallback(callback)
    }

    private fun frame() {
        if (!running) return
        val at = System.nanoTime() / 1e9
        val elapsed = if (last == 0.0) 1.0 / 60 else minOf(at - last, 0.1)
        last = at
        tick(elapsed)
        choreographer.postFrameCallback(callback)
    }
}
