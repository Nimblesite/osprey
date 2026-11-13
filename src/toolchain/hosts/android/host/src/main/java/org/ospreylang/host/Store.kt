package org.ospreylang.host

import android.os.Handler
import android.os.Looper

// Implements [NATIVE-SHM-VIEW], [NATIVE-SHM-EVENT] and [NATIVE-SHM-ASK] for
// the host's model, as the SwiftUI host's SharedStore.swift and RegionStore.swift
// do: it opens the bridge, calls the application's exports with typed events,
// keeps the checked frame they drew, and answers what that frame asks. Slice
// writes land in the host's slice table, diagnostics show on the host's strip,
// and requests go to capability adapters whose completions return as events.
// Everything runs on the UI thread.

class Store(private val adapters: List<Adapter>) {
    /** Bumped whenever a frame is adopted, so the views read the rows again. */
    var onFrame: ((Store) -> Unit)? = null
    var shown = Shown.EMPTY
        private set
    var regions: Map<String, Region> = emptyMap()
        private set
    var theme: Theme? = null
        private set
    /** Whether the last frame told components; a frame of plain view rows is
     * drawn by the row view. */
    var told = true
        private set
    val diagnostics = mutableListOf<String>()
    var failure: String? = null
        private set
    var revision = 0
        private set
    var animating = false
        private set

    private var handle = 0L
    private val seen = mutableSetOf<String>()
    private val main = Handler(Looper.getMainLooper())
    private val runner = Runner(adapters, main, Bridge::write)
    private val clock = Clock { elapsed -> send(Touch.frame(elapsed)) }

    val count get() = shown.controls.size
    fun control(index: Int) = shown.controls.getOrNull(index)
    fun kids(index: Int) = shown.controls.getOrNull(index)?.children ?: emptyList()
    fun scene(index: Int) = shown.scenes[index]
    fun words(at: Int) = control(at)?.words ?: ""
    fun action(at: Int) = control(at)?.action ?: -1L
    fun region(key: String) = regions[key]

    /** Opens the instance, runs the program, hands over the launch arguments
     * in order, draws the first frame for this appearance and says it is active. */
    fun start(arguments: List<String>, dark: Boolean, platform: String, home: String, offset: Long) {
        if (handle != 0L || failure != null) return
        try {
            handle = Bridge.openBridge()
            Bridge.mainEntry()
            // An argument's frame is committed and checked, but no view is adopted from it.
            for (argument in arguments) call { Bridge.argument(handle, argument.toByteArray(Charsets.UTF_8)) }
            adopt(call { Bridge.openApp(handle, moment(), offset, platform.toByteArray(Charsets.UTF_8), home.toByteArray(Charsets.UTF_8), dark) })
            send(Touch.phase("active"))
        } catch (error: Throwable) {
            fail("Osprey could not start: ${error.message}")
        }
    }

    /** Delivers one host event and keeps the frame it drew. A frame that
     * fails its checks is refused whole and the last good view stays. */
    fun send(touch: Touch) {
        if (handle == 0L || failure != null) return
        try {
            adopt(call { bytes(touch) })
        } catch (refusal: RowRefusal) {
            diagnose(refusal.why)
        } catch (error: Throwable) {
            fail("Osprey could not handle an event: ${error.message}")
        }
    }

    /** The system appearance changed; the application redraws for it. */
    fun appear(dark: Boolean) = send(Touch.appearance(dark))

    /** Adds a host diagnostic, in encounter order. */
    fun diagnose(message: String) {
        diagnostics.add(message)
    }

    /** Adds a recurring diagnostic only on its first encounter. */
    fun diagnoseOnce(message: String) {
        if (seen.add(message)) diagnose(message)
    }

    /** One exported call: its frame is read, checked and released whole. */
    private fun call(export: () -> Array<java.nio.ByteBuffer>): Shown =
        Bridge.read(handle, export()) { runs ->
            Rows.read(runs[0], runs[1], runs[2], runs[3])
        }

    private fun bytes(touch: Touch): Array<java.nio.ByteBuffer> =
        Bridge.event(handle, moment(), touch.kind, touch.ordinal, touch.words.toByteArray(Charsets.UTF_8),
            touch.detail.toByteArray(Charsets.UTF_8), touch.x, touch.y, touch.down, touch.shift)

    /** Applies a checked frame: its slice writes, its diagnostics, its view
     * and its requests. Slice writes land first, so a request that names one
     * finds the text it wrote. */
    private fun adopt(next: Shown) {
        next.slices.forEach { (key, text) -> Bridge.write(key, text) }
        diagnostics.addAll(next.diagnostics)
        adoptRegions(next)
        shown = next
        revision++
        onFrame?.invoke(this)
        answer(next.requests)
    }

    /** Keeps the regions of a frame: a region whose tree did not change keeps
     * its cell untouched, so only what changed draws again. */
    private fun adoptRegions(next: Shown) {
        val frame = Told.frame(next)
        if (frame == null) {
            told = next.controls.isEmpty()
            return
        }
        told = true
        if (frame.theme != theme) theme = frame.theme
        val live = frame.regions.toMap()
        regions = live.mapValues { (key, node) ->
            val cell = regions[key]
            if (cell != null && cell.node == node) cell
            else Region(key, (cell?.revision?.plus(1)) ?: 0, node)
        }
    }

    /** Answers every capability the latest frame requested; an unknown kind is
     * diagnosed and still completes, with an error naming it. */
    private fun answer(asks: List<Ask>) {
        for (ask in asks) {
            when {
                ask.kind == "animate" -> {
                    animating = ask.flag(1)
                    clock.running = animating
                    send(Touch.completed(ask.ticket, ""))
                }
                runner.knows(ask.kind) -> runner.run(ask) { outcome ->
                    send(when (outcome) {
                        is Outcome.Done -> Touch.completed(ask.ticket, outcome.text)
                        is Outcome.Bare -> Touch.completed(ask.ticket, outcome.path)
                        is Outcome.Refused -> Touch.completedFailed(ask.ticket, outcome.why)
                    })
                }
                else -> {
                    val problem = "Unknown capability ${ask.code}"
                    diagnose("$problem (ticket ${ask.ticket})")
                    send(Touch.completedFailed(ask.ticket, problem))
                }
            }
        }
    }

    private fun fail(problem: String) {
        failure = problem
        if (handle != 0L) {
            Bridge.closeBridge(handle)
            handle = 0L
        }
    }

    /** Unix seconds for an event or launch context. */
    private fun moment() = System.currentTimeMillis() / 1000

    fun close() {
        if (handle != 0L) {
            Bridge.closeBridge(handle)
            handle = 0L
        }
        clock.running = false
        runner.close()
    }
}
