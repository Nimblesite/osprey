package org.ospreylang.talon

import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URI
import java.io.ByteArrayOutputStream
import java.util.concurrent.Executors

/** Osprey owns all banking state, validation, effects, routes and semantic views. */
internal class BankHost(val baseUrl: String, savedModel: String? = null) {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor()
    private var closed = false
    var observer: ((JSONObject) -> Unit)? = null
    var commandObserver: ((JSONObject) -> Unit)? = null
    var errorObserver: ((Throwable) -> Unit)? = null
    var envelope = JSONObject()
        private set
    val model: String get() = envelope.optString("model", "{}")
    val state: JSONObject get() = JSONObject(model)
    var pending = 0
        private set

    init {
        require(validBaseUrl(baseUrl)) { "Use HTTPS or a local Android development server URL." }
        envelope = JSONObject(if (savedModel == null) Native.start() else Native.dispatch(JSONObject().put("kind", "restore").put("model", savedModel).toString()))
    }

    fun attach(render: (JSONObject) -> Unit, command: (JSONObject) -> Unit, error: (Throwable) -> Unit) {
        observer = render; commandObserver = command; errorObserver = error
        render(envelope)
    }

    fun startCommands(restoredProcess: Boolean) {
        runCommands(envelope.optJSONArray("commands") ?: JSONArray())
        if (restoredProcess) {
            // Never replay a mutation after process death; refresh its authoritative result.
            val recovered = JSONObject(model).put("busy", "false")
            envelope = JSONObject(Native.dispatch(JSONObject().put("kind", "restore").put("model", recovered.toString()).toString()))
            dispatch(JSONObject().put("kind", "click").put("id", "topbar-refresh"))
        }
    }

    fun dispatch(event: JSONObject) {
        check(Looper.myLooper() == Looper.getMainLooper())
        if (closed) return
        try {
            if (event.optString("kind") == "submit" && state.optString("busy") == "true") return
            event.put("model", model)
            envelope = JSONObject(Native.dispatch(event.toString()))
            observer?.invoke(envelope)
            runCommands(envelope.optJSONArray("commands") ?: JSONArray())
        } catch (error: Exception) { errorObserver?.invoke(error) }
    }

    private fun runCommands(commands: JSONArray) {
        for (index in 0 until commands.length()) {
            val command = commands.optJSONObject(index) ?: continue
            when (command.optString("kind", command.optString("type"))) {
                "http" -> request(command)
                else -> commandObserver?.invoke(command)
            }
        }
    }

    private fun request(command: JSONObject) {
        pending += 1
        worker.execute {
            var connection: HttpURLConnection? = null
            var status = 0
            val data = try {
                val relative = URI(command.getString("url"))
                require(!relative.isAbsolute && relative.rawAuthority == null) { "Bank API commands must use relative paths" }
                val target = URI(baseUrl.trimEnd('/') + "/").resolve(relative)
                connection = target.toURL().openConnection() as HttpURLConnection
                connection.connectTimeout = 10000; connection.readTimeout = 15000
                connection.instanceFollowRedirects = false
                connection.requestMethod = command.optString("method", "GET").uppercase()
                connection.setRequestProperty("Accept", "application/json")
                val body = command.optString("body")
                if (body.isNotEmpty() && connection.requestMethod !in listOf("GET", "HEAD")) {
                    connection.doOutput = true
                    connection.setRequestProperty("Content-Type", "application/json; charset=utf-8")
                    connection.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
                }
                status = connection.responseCode
                val stream = if (status in 200..299) connection.inputStream else connection.errorStream
                stream?.use { source ->
                    val result = ByteArrayOutputStream()
                    val buffer = ByteArray(8192)
                    while (true) {
                        val count = source.read(buffer)
                        if (count < 0) break
                        require(result.size() + count <= 4 * 1024 * 1024) { "Ledger response is too large" }
                        result.write(buffer, 0, count)
                    }
                    result.toByteArray().toString(Charsets.UTF_8)
                } ?: "{}"
            } catch (error: Exception) {
                status = 0
                JSONObject().put("error", "Could not reach the ledger: ${error.message ?: "network error"}").toString()
            } finally { connection?.disconnect() }
            main.post {
                pending -= 1
                if (!closed) dispatch(JSONObject().put("kind", "http").put("id", command.optString("id")).put("status", status).put("data", data))
            }
        }
    }

    fun detach() { observer = null; commandObserver = null; errorObserver = null }
    fun close() { closed = true; detach(); worker.shutdownNow() }

    companion object {
        const val DEFAULT_URL = "http://127.0.0.1:18790"
        fun validBaseUrl(value: String): Boolean = try {
            val url = URI(value)
            url.userInfo == null && url.query == null && url.fragment == null && !url.host.isNullOrBlank() &&
                (url.scheme == "https" || (url.scheme == "http" && url.host in setOf("127.0.0.1", "localhost", "10.0.2.2")))
        } catch (_: Exception) { false }
    }
}
