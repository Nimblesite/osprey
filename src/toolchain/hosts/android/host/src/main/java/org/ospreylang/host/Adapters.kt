package org.ospreylang.host

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID

// Implements [UI-EFFECTS]: the asks this host answers with the platform's own
// services. Store files live in the app's private store; HTTP runs on
// HttpURLConnection off the main thread; host-calls answer the `android`
// namespace (open-url, clipboard) and a chooser runs through the Storage
// Access Framework. Every other ask completes with the typed refusal the
// SwiftUI host gives, naming the capability.

/** App-private files, asked for by the typed fields of ask rows. Names are
 * relative paths inside the store and never escape it; writes are atomic. */
class StoreAdapter(private val root: File) : Adapter {
    override val kinds = setOf("store-read", "store-write", "store-list", "store-delete")

    override fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit) {
        try {
            root.mkdirs()
            done(answer(ask, staged))
        } catch (error: Exception) {
            done(Outcome.Refused(error.message ?: "The store failed"))
        }
    }

    private fun answer(ask: Ask, staged: StagedSlices): Outcome = when (ask.kind) {
        "store-read" -> read(ask, staged)
        "store-write" -> write(ask)
        "store-list" -> list(ask)
        else -> delete(ask)
    }

    /** A name stays inside the store: no absolute paths, no parent references. */
    private fun location(name: String): File {
        val parts = name.split('/').filter { it.isNotEmpty() }
        if (name.isEmpty() || name.startsWith("/") || parts.contains(".."))
            throw IOException("Invalid store name")
        return parts.fold(root) { folder, part -> File(folder, part) }
    }

    private fun read(ask: Ask, staged: StagedSlices): Outcome {
        val file = location(ask.text(1))
        if (!file.isFile) return Outcome.Done(Doc.obj(listOf("found" to Doc.bool(false))))
        val limit = ask.number(2).let { if (it < 0 || it >= Int.MAX_VALUE) throw IOException("Invalid file byte limit") else it.toInt() }
        if (file.length() > limit) throw IOException("${file.length()} bytes exceeds the $limit byte limit")
        staged.write(ask.text(3), String(file.readBytes(), Charsets.UTF_8))
        return Outcome.Done(Doc.obj(listOf("found" to Doc.bool(true))))
    }

    private fun write(ask: Ask): Outcome {
        val file = location(ask.text(1))
        file.parentFile?.mkdirs()
        val temporary = File(file.parentFile, ".osprey-writing-${UUID.randomUUID()}")
        temporary.writeBytes(Bridge.text(ask.text(2)))
        if (!temporary.renameTo(file)) {
            temporary.delete()
            throw IOException("The store write could not be settled")
        }
        return Outcome.Done(Doc.obj(listOf("path" to Doc.str(file.absolutePath))))
    }

    private fun list(ask: Ask): Outcome {
        val folder = location(ask.text(1))
        val names = if (folder.isDirectory) folder.list()?.sorted() ?: emptyList() else emptyList()
        return Outcome.Done(Doc.obj(listOf("names" to Doc.arr(names.map { Doc.str(it) }))))
    }

    private fun delete(ask: Ask): Outcome {
        location(ask.text(1)).delete()
        return Outcome.Done(Doc.obj(emptyList()))
    }
}

/** HTTPS, or HTTP on this machine: no redirects, no cookies, no cache, a
 * total deadline and a streamed response limit. Nothing is retried. */
class HttpAdapter : Adapter {
    override val kinds = setOf("http")

    override fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit) {
        try {
            val deadline = maxOf(ask.number(6), 1L)
            val limit = ask.number(5).let { if (it < 0 || it >= Int.MAX_VALUE) throw IOException("Invalid response byte limit") else it.toInt() }
            val connection = outgoing(ask, deadline)
            val status = connection.responseCode
            val body = connection.errorStream ?: connection.inputStream
            val bytes = bounded(body, limit)
            connection.disconnect()
            staged.write(ask.text(7), String(bytes, Charsets.UTF_8))
            done(Outcome.Done(Doc.obj(listOf("status" to Doc.num(status.toLong()), "headers" to headers(connection)))))
        } catch (error: Exception) {
            done(Outcome.Refused(error.message ?: "The request failed"))
        }
    }

    /** Reads at most `limit` bytes, refusing a body that runs past it. */
    private fun bounded(body: java.io.InputStream, limit: Int): ByteArray {
        val held = ByteArray(limit + 1)
        var at = 0
        while (at <= limit) {
            val took = body.read(held, at, limit + 1 - at)
            if (took < 0) break
            at += took
            if (at > limit) throw IOException("${at} bytes exceeds the $limit byte limit")
        }
        return held.copyOf(at)
    }

    private fun outgoing(ask: Ask, deadline: Long): HttpURLConnection {
        val url = URL(ask.text(2))
        val loopback = url.protocol == "http" && url.host in setOf("localhost", "127.0.0.1", "::1")
        if (url.protocol != "https" && !loopback) throw IOException("Only https URLs, or http on this machine, are allowed")
        val connection = url.openConnection() as HttpURLConnection
        connection.requestMethod = ask.text(1).ifEmpty { "GET" }
        // The deadline is in milliseconds, as the platform's timeouts are.
        val wait = deadline.coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
        connection.connectTimeout = wait
        connection.readTimeout = wait
        connection.useCaches = false
        connection.instanceFollowRedirects = false
        ask.pairs(3).forEach { connection.setRequestProperty(it.first, it.second) }
        val body = ask.text(4)
        if (body.isNotEmpty()) {
            connection.doOutput = true
            connection.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        }
        return connection
    }

    private fun headers(connection: HttpURLConnection): String {
        var bytes = 0
        val headers = mutableListOf<String>()
        for ((name, value) in connection.headerFields.filterKeys { it != null }) {
            bytes += name.length + value.joinToString("").length
            if (bytes > 16384 || headers.size >= 100) throw IOException("The response metadata exceeds its limit")
            headers.add(Doc.obj(listOf("name" to Doc.str(name.lowercase()), "value" to Doc.str(value.joinToString(",")))))
        }
        return Doc.arr(headers.sorted())
    }
}

/** The `android` host-call namespace: `open-url` hands a page to the browser,
 * `clipboard` reads or writes the person's clipboard. */
class HostCallAdapter(private val context: Context) : Adapter {
    override val kinds = setOf("host-call")

    override fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit) {
        try {
            if (ask.text(1) != "android") throw IOException("This host answers no ${ask.text(1)} host-calls")
            done(when (ask.text(2)) {
                "open-url" -> openUrl(ask)
                "clipboard" -> clipboard(ask)
                else -> throw IOException("This host answers no ${ask.text(2)} host-call")
            })
        } catch (error: Exception) {
            done(Outcome.Refused(error.message ?: "The host-call failed"))
        }
    }

    private fun openUrl(ask: Ask): Outcome {
        val address = ask.pairs(3).firstOrNull()?.first ?: throw IOException("open-url has no valid url")
        val url = URL(address)
        if (url.protocol != "https") throw IOException("Only https URLs open")
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(address)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return Outcome.Done(Doc.obj(emptyList()))
    }

    private fun clipboard(ask: Ask): Outcome {
        val board = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val words = ask.pairs(3).firstOrNull()?.first ?: ""
        return if (words.isEmpty())
            Outcome.Done(Doc.obj(listOf("text" to Doc.str(board.primaryClip?.getItemAt(0)?.text?.toString() ?: ""))))
        else {
            board.setPrimaryClip(ClipData.newPlainText("Osprey", words))
            Outcome.Done(Doc.obj(emptyList()))
        }
    }
}

/** The chooser through the Storage Access Framework: the document the person
 * picks is copied into the host's import folder and the ask completes with
 * that copy's bare path. The activity owns the launcher, which runs on the
 * UI thread. */
class ChooseAdapter(private val launch: (Ask, (Outcome) -> Unit) -> Unit) : Adapter {
    override val kinds = setOf("choose-file")

    override fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit) {
        if (ask.texts(1).isEmpty()) {
            done(Outcome.Refused("choose-file has no valid types"))
            return
        }
        launch(ask, done)
    }

}

/** Reads text files this host itself imported, bounded by the ask's limit, so
 * a chooser's answer can be read back without granting the store a window on
 * the rest of the device. */
class ReadTextsAdapter(private val root: File) : Adapter {
    override val kinds = setOf("read-texts")

    override fun perform(ask: Ask, staged: StagedSlices, done: (Outcome) -> Unit) {
        try {
            val limit = ask.number(2).let { if (it < 0 || it >= Int.MAX_VALUE) throw IOException("Invalid file byte limit") else it.toInt() }
            val texts = ask.texts(1).map { path ->
                val file = File(path).canonicalFile
                if (!file.path.startsWith(root.canonicalPath + File.separator)) throw IOException("$path is outside the host's files")
                val bytes = file.readBytes()
                if (bytes.size > limit) throw IOException("${bytes.size} bytes exceeds the $limit byte limit")
                String(bytes, Charsets.UTF_8)
            }
            staged.write(ask.text(3), texts.joinToString("\n"))
            done(Outcome.Done(Doc.obj(emptyList())))
        } catch (error: Exception) {
            done(Outcome.Refused(error.message ?: "The read failed"))
        }
    }
}
