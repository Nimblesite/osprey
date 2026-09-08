package org.ospreylang.issueinbox

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File

// Real JNI + SQLite with deterministic HTTP or optional live GitHub. [MOBILE-VERIFICATION]
internal class SmokeCheck(context: Context, private val phase: String, private val live: Boolean, private val dispatch: (JSONObject) -> Unit) {
    private var step = 0
    private var savedId = ""
    private val idFile = File(context.filesDir, "mobile-smoke-id")
    private val result = File(context.filesDir, "mobile-smoke-$phase.json")
    private val impossible = "Osprey 🦅 Ω 不存在 smoke search 70c29a"
    private val note = "Review parser's 🦉 handling"

    fun observe(envelope: JSONObject) {
        if (step == 99) return
        try { verify(envelope) } catch (failure: Exception) {
            step = 99
            result.writeText(JSONObject().put("ok", false).put("error", failure.message).toString())
            Log.e("OspreyInbox", "OSPREY_ANDROID_SMOKE_FAILED", failure)
        }
    }

    private fun verify(envelope: JSONObject) {
        val view = envelope.getJSONObject("view")
        val model = JSONObject(envelope.getString("model"))
        if (model.getString("stage") != "ready" || view.getBoolean("loading") || model.getString("write").isNotEmpty()) return
        if (phase != "restore" || step != 3) check(view.getString("error").isEmpty()) { view.getString("error") }
        check(view.getInt("total") > 0) { "HTTP request returned no issues" }
        if (phase == "restore") restored(view) else if (step < 5) inbox(view) else detail(view)
    }

    private fun inbox(view: JSONObject) {
        val items = view.getJSONArray("items")
        when (step) {
            0 -> {
                check(view.getString("status").contains("synced with GitHub"))
                if (!live) check(view.getInt("total") == 2) { "Pull requests were not excluded" }
                savedId = items.getJSONObject(0).getString("id")
                idFile.writeText(savedId)
                step = 1; send("bookmark", "id", savedId)
            }
            1 -> {
                check(view.getInt("bookmarked") == 1)
                step = 2; send("search", "value", impossible)
            }
            2 -> {
                check(view.getString("search") == impossible && items.length() == 0) { "UTF-8 reactive search failed" }
                step = 3; send("search", "value", "")
            }
            3 -> { check(items.length() > 0); step = 4; send("filter", "value", "saved") }
            4 -> {
                check(items.length() == 1 && items.getJSONObject(0).getString("id") == savedId)
                check(items.getJSONObject(0).getBoolean("bookmarked"))
                step = 5; send("open", "id", savedId)
            }
        }
    }

    private fun detail(view: JSONObject) {
        when (step) {
            5 -> {
                check(view.getString("selected") == savedId && view.getJSONObject("detail").getString("id") == savedId)
                if (!live) check(view.getJSONObject("detail").getString("body") == "Steps to **reproduce** 🦉" && view.getJSONObject("detail").getString("labels").contains("bug"))
                step = 6; dispatch(JSONObject().put("type", "note").put("id", savedId).put("value", note))
            }
            6 -> {
                check(view.getJSONObject("detail").getString("note") == note)
                step = 7; dispatch(JSONObject().put("type", "priority").put("id", savedId).put("value", "high"))
            }
            7 -> { check(view.getJSONObject("detail").getString("priority") == "high"); step = 8; dispatch(JSONObject().put("type", "back")) }
            8 -> { check(view.getString("selected").isEmpty() && view.isNull("detail")); succeeded(view) }
        }
    }

    private fun restored(view: JSONObject) {
        val id = idFile.readText()
        when (step) {
            0 -> {
                check(view.getString("status").contains("Loaded from SQLite")) { "Reopened app did not load SQLite" }
                check(view.getString("selected").isEmpty()) { "Transient navigation was incorrectly restored" }
                val items = view.getJSONArray("items")
                check((0 until items.length()).any { items.getJSONObject(it).getString("id") == id && items.getJSONObject(it).getBoolean("bookmarked") })
                step = 1; send("open", "id", id)
            }
            1 -> {
                check(view.getJSONObject("detail").getString("note") == note && view.getJSONObject("detail").getString("priority") == "high") { "SQLite lost local triage data" }
                step = 2; dispatch(JSONObject().put("type", "back"))
            }
            2 -> if (live) succeeded(view) else { step = 3; dispatch(JSONObject().put("type", "refresh")) }
            3 -> {
                check(view.getString("error").isNotEmpty() && view.getInt("total") == 2 && view.getInt("bookmarked") == 1) { "Failed HTTP refresh lost cached state or hid the error" }
                succeeded(view)
            }
        }
    }

    private fun send(type: String, key: String, value: String) = dispatch(JSONObject().put("type", type).put(key, value))

    companion object {
        fun fixture(phase: String): (JSONObject) -> JSONObject = { command ->
            val status = if (phase == "restore") 403 else 200
            val body = if (status == 403) "{\"message\":\"API rate limit exceeded\"}" else """
                [{"id":101,"number":42,"title":"First issue 🦅","body":"Steps to **reproduce** 🦉","labels":[{"name":"bug"}],"user":{"login":"alice"},"comments":3},
                 {"id":102,"number":43,"title":"Second issue","user":{"login":"bob"},"comments":0},
                 {"id":103,"number":44,"title":"A pull request","user":{"login":"carol"},"comments":0,"pull_request":{}}]
            """.trimIndent()
            JSONObject().put("type", "http").put("id", command.getString("id")).put("status", status).put("body", body).put("error", "")
        }
    }

    private fun succeeded(view: JSONObject) {
        step = 99
        result.writeText(JSONObject().put("ok", true).put("phase", phase).put("source", if (live) "live GitHub" else "HTTP fixture").put("view", view).toString())
        Log.i("OspreyInbox", "OSPREY_ANDROID_SMOKE_OK $phase issues=${view.getInt("total")} saved=${view.getInt("bookmarked")}")
    }
}
