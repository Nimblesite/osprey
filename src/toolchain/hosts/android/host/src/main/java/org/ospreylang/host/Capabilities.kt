package org.ospreylang.host

import android.os.Handler
import android.os.Looper
import java.util.concurrent.Executors
import java.util.concurrent.Future

// Implements [UI-EFFECTS] for the Android host: each capability ask goes to
// one mechanical adapter that runs off the UI thread, is bounded by the
// limits Osprey named and can be cancelled by ticket. Adapters take the typed
// Ask the frame's ask rows became and answer with the result document the
// application's Doc reads, as compact text, or a bare path a chooser completes
// with; they decide nothing. A completion nobody receives, because its request
// was cancelled or replaced first, is discarded.

/** What an adapter hands its runner. */
sealed class Outcome {
    /** The result document as compact text. */
    class Done(val text: String) : Outcome()

    /** A chooser's bare path, exactly as the completion carries it. */
    class Bare(val path: String) : Outcome()

    /** The typed refusal the host gives for an ask it cannot answer. */
    class Refused(val why: String) : Outcome()
}

/** Slice writes an adapter stages; only a live successful completion publishes them. */
class StagedSlices {
    private val staged = mutableListOf<Pair<String, String>>()

    fun write(key: String, text: String) {
        staged.add(key to text)
    }

    fun publish(onto: (String, String) -> Unit) = staged.forEach { onto(it.first, it.second) }
}

/** One mechanical capability adapter. */
interface Adapter {
    val kinds: Set<String>

    /** Runs the ask off the UI thread and hands the outcome to `done`, which
     * is safe to call from any thread. */
    fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit)
}

/** Builds the compact result documents the application's Doc decodes. */
object Doc {
    fun quote(value: String): String = buildString {
        append('"')
        for (character in value) when (character) {
            '"' -> append("\\\"")
            '\\' -> append("\\\\")
            '\n' -> append("\\n")
            '\r' -> append("\\r")
            '\t' -> append("\\t")
            '\b' -> append("\\b")
            '\u000C' -> append("\\f")
            else -> if (character.code < 0x20) append("\\u%04x".format(character.code)) else append(character)
        }
        append('"')
    }

    fun obj(members: List<Pair<String, String>>) =
        "{" + members.joinToString(",") { quote(it.first) + ":" + it.second } + "}"

    fun arr(items: List<String>) = "[" + items.joinToString(",") + "]"
    fun str(value: String) = quote(value)
    fun bool(value: Boolean) = if (value) "true" else "false"
    fun num(value: Long) = value.toString()
}

/** One in-flight ask, cancellable by ticket. */
private class Flight(val identity: Int, val task: Future<*>)

/** Runs asks on a small worker pool and completes them on the main thread. */
class Runner(adapters: List<Adapter>, private val main: Handler,
             private val publish: (String, String) -> Unit) {
    private val table = adapters.flatMap { adapter -> adapter.kinds.map { it to adapter } }.toMap()
    private val pool = Executors.newCachedThreadPool()
    private val inflight = HashMap<String, Flight>()
    private var identities = 0

    fun knows(kind: String) = kind == "cancel" || table.containsKey(kind)

    fun run(ask: Ask, complete: (Outcome) -> Unit) {
        if (ask.kind == "cancel") return cancel(ask, complete)
        val adapter = table[ask.kind] ?: return complete(Outcome.Refused("Unknown capability ${ask.kind}"))
        inflight.remove(ask.ticket)?.task?.cancel(true)
        val identity = identities++
        val staged = StagedSlices()
        val task = pool.submit {
            adapter.perform(ask, staged) { outcome -> land(ask, identity, staged, outcome, complete) }
        }
        inflight[ask.ticket] = Flight(identity, task)
    }

    /** Publishes staged slices and lands the completion. A completion nobody
     * waits for, because its ask was cancelled or replaced first, is discarded. */
    private fun land(ask: Ask, identity: Int, staged: StagedSlices,
                     outcome: Outcome, complete: (Outcome) -> Unit) {
        main.post {
            val held = inflight[ask.ticket]
            if (held?.identity != identity) return@post
            inflight.remove(ask.ticket)
            if (outcome is Outcome.Done) staged.publish(publish)
            complete(outcome)
        }
    }

    private fun cancel(ask: Ask, complete: (Outcome) -> Unit) {
        inflight.remove(ask.text(1))?.task?.cancel(true)
        complete(Outcome.Done(Doc.obj(emptyList())))
    }

    fun close() {
        pool.shutdownNow()
    }
}
