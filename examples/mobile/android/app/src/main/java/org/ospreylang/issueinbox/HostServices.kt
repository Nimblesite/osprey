package org.ospreylang.issueinbox

import android.content.Context
import android.database.Cursor
import android.database.sqlite.SQLiteCursor
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteProgram
import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import java.net.URI
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.concurrent.Executors
import javax.net.ssl.HttpsURLConnection

// Generic asynchronous transport. SQL, URLs and response interpretation come from Osprey.
// Implements [MOBILE-SERVICES] and [MOBILE-NATIVE-HOST].
internal class HostServices(context: Context, filename: String = "inbox.sqlite", private val httpFixture: ((JSONObject) -> JSONObject)? = null, private val complete: (JSONObject) -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val sqlQueue = Executors.newSingleThreadExecutor()
    private val httpQueue = Executors.newCachedThreadPool()
    private val database = SQLiteDatabase.openOrCreateDatabase(context.getDatabasePath(filename).apply { parentFile?.mkdirs() }, null)
    private var closed = false

    fun execute(command: JSONObject) {
        val kind = command.getString("kind")
        require(kind == "sql" || kind == "http") { "Unknown Osprey host command: $kind" }
        (if (kind == "sql") sqlQueue else httpQueue).execute {
            val event = if (kind == "sql") sql(command) else httpFixture?.invoke(command) ?: http(command)
            main.post { if (!closed) complete(event) }
        }
    }

    fun close() {
        closed = true
        sqlQueue.execute { database.close() }
        sqlQueue.shutdown()
        httpQueue.shutdownNow()
    }

    private fun sql(command: JSONObject): JSONObject {
        val event = JSONObject().put("type", "sql").put("id", command.getString("id"))
        return try {
            val statement = command.getString("sql")
            val params = command.getJSONArray("params")
            validateSql(statement, params.length())
            val query = statement.trimStart().uppercase().let { it.startsWith("SELECT") || it.startsWith("WITH") || it.startsWith("PRAGMA") || it.startsWith("EXPLAIN") }
            val rows = if (query) query(statement, params) else {
                database.compileStatement(statement).use { bind(it, params); it.execute() }
                JSONArray()
            }
            event.put("ok", true).put("rows", rows).put("error", "")
        } catch (error: Exception) { event.put("ok", false).put("rows", JSONArray()).put("error", error.message ?: error.toString()) }
    }

    private fun bind(statement: SQLiteProgram, params: JSONArray) {
        for (index in 0 until params.length()) when (val value = params.get(index)) {
            JSONObject.NULL -> statement.bindNull(index + 1)
            is Boolean -> statement.bindLong(index + 1, if (value) 1 else 0)
            is Int, is Long -> statement.bindLong(index + 1, (value as Number).toLong())
            is Number -> { require(value.toDouble().isFinite()); statement.bindDouble(index + 1, value.toDouble()) }
            is String -> statement.bindString(index + 1, value)
            else -> error("SQL parameters must be finite JSON scalars")
        }
    }

    private fun query(sql: String, params: JSONArray): JSONArray {
        val factory = SQLiteDatabase.CursorFactory { _, driver, table, query -> bind(query, params); SQLiteCursor(driver, table, query) }
        return database.rawQueryWithFactory(factory, sql, emptyArray(), null).use { cursor ->
            require(cursor.columnNames.toSet().size == cursor.columnCount) { "SQL returned duplicate column names" }
            JSONArray().apply { while (cursor.moveToNext()) put(row(cursor)) }
        }
    }

    private fun row(cursor: Cursor) = JSONObject().apply {
        for (index in 0 until cursor.columnCount) put(cursor.getColumnName(index), when (cursor.getType(index)) {
            Cursor.FIELD_TYPE_NULL -> JSONObject.NULL
            Cursor.FIELD_TYPE_INTEGER -> cursor.getLong(index)
            Cursor.FIELD_TYPE_FLOAT -> cursor.getDouble(index).also { require(it.isFinite()) }
            Cursor.FIELD_TYPE_STRING -> cursor.getString(index)
            else -> error("SQL blobs cannot be returned as JSON scalars")
        })
    }

    private fun http(command: JSONObject): JSONObject {
        val event = JSONObject().put("type", "http").put("id", command.getString("id")).put("status", 0)
        return try {
            val address = URI(command.getString("url"))
            require(address.scheme == "https" && address.host != null && address.userInfo == null) { "HTTP commands require an HTTPS URL" }
            val connection = address.toURL().openConnection() as HttpsURLConnection
            try { response(connection, event) } finally { connection.disconnect() }
        } catch (error: Exception) { event.put("body", "").put("error", error.message ?: error.toString()) }
    }

    private fun response(connection: HttpsURLConnection, event: JSONObject): JSONObject {
        connection.connectTimeout = 20_000
        connection.readTimeout = 20_000
        connection.instanceFollowRedirects = false
        connection.setRequestProperty("User-Agent", "Osprey-Issue-Inbox")
        connection.setRequestProperty("Accept", "application/vnd.github+json")
        val status = connection.responseCode
        event.put("status", status)
        require(connection.contentLengthLong <= 2 * 1024 * 1024) { "HTTP response exceeds 2 MiB" }
        val stream = if (status >= 400) connection.errorStream else connection.inputStream
        val bytes = stream?.use { source ->
            val output = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            val deadline = android.os.SystemClock.elapsedRealtime() + 30_000
            while (true) {
                val count = source.read(buffer)
                if (count < 0) break
                require(output.size() + count <= 2 * 1024 * 1024) { "HTTP response exceeds 2 MiB" }
                require(android.os.SystemClock.elapsedRealtime() <= deadline) { "HTTP response timed out" }
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: byteArrayOf()
        require(bytes.size <= 2 * 1024 * 1024) { "HTTP response exceeds 2 MiB" }
        val body = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
        return event.put("body", body).put("error", "")
    }
}

// Android's SQLite wrapper does not expose sqlite3_prepare's tail or bind count.
// Restrict the transport to one statement and positional '?' parameters explicitly.
private fun validateSql(sql: String, count: Int) {
    require(!sql.contains('\u0000')) { "SQL contains a NUL byte" }
    val tokens = StringBuilder()
    var quote = ' '
    var index = 0
    while (index < sql.length) {
        val ch = sql[index]
        val next = sql.getOrNull(index + 1)
        when {
            quote != ' ' -> if (ch == quote) { if (next == quote) index++ else quote = ' ' }
            ch in "'\"`[" -> quote = if (ch == '[') ']' else ch
            ch == '-' && next == '-' -> { while (index < sql.length && sql[index] != '\n') index++ }
            ch == '/' && next == '*' -> { val end = sql.indexOf("*/", index + 2); require(end >= 0); index = end + 1 }
            else -> tokens.append(ch)
        }
        index++
    }
    require(quote == ' ') { "Unclosed SQL string or identifier" }
    val statement = tokens.toString().trim().removeSuffix(";")
    require(statement.isNotBlank() && !statement.contains(';')) { "SQL commands must contain exactly one statement" }
    require(!Regex("\\?[0-9]|[:@$][A-Za-z_]").containsMatchIn(statement)) { "Use positional '?' SQL parameters" }
    require(statement.count { it == '?' } == count) { "SQL parameter count does not match statement" }
}
