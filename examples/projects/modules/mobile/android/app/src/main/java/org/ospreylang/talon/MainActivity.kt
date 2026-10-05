package org.ospreylang.talon

import android.app.Activity
import android.app.AlertDialog
import android.graphics.Color
import android.os.Bundle
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import org.json.JSONObject

class MainActivity : Activity() {
    internal lateinit var host: BankHost
    internal lateinit var renderer: BankRenderer
    private var retained = false
    private var previousRoute = "overview"
    private val history = mutableListOf<String>()
    private var handlingBack = false

    private class Retained(val host: BankHost, val drafts: String, val history: List<String>, val previousRoute: String)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = Color.rgb(247, 247, 242)
        window.navigationBarColor = Color.rgb(247, 247, 242)
        window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        try {
            val saved = lastNonConfigurationInstance as? Retained
            val server = intent.getStringExtra("server_url") ?: getPreferences(MODE_PRIVATE).getString("server_url", BankHost.DEFAULT_URL)!!
            host = saved?.host ?: BankHost(server, savedInstanceState?.getString("model"))
            previousRoute = saved?.previousRoute ?: savedInstanceState?.getString("previousRoute") ?: "overview"
            history.addAll(saved?.history ?: savedInstanceState?.getStringArrayList("history") ?: emptyList())
            renderer = BankRenderer(this, { event -> host.dispatch(event) }, { settings() })
            renderer.restore(saved?.drafts ?: savedInstanceState?.getString("drafts") ?: "")
            setContentView(renderer.root)
            host.attach({ envelope ->
                val next = host.state.optString("route", "overview")
                if (next != previousRoute) {
                    if (!handlingBack) history.add(previousRoute)
                    previousRoute = next
                }
                renderer.render(envelope)
            }, { command -> renderer.command(command) }, { error -> showError(error) })
            if (saved == null) host.startCommands(savedInstanceState != null)
        } catch (error: Exception) { showError(error) }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        if (::host.isInitialized && ::renderer.isInitialized) {
            outState.putString("model", host.model)
            outState.putString("drafts", renderer.snapshot())
            outState.putString("previousRoute", previousRoute)
            outState.putStringArrayList("history", ArrayList(history))
        }
        super.onSaveInstanceState(outState)
    }

    override fun onRetainNonConfigurationInstance(): Any? {
        if (!::host.isInitialized || !::renderer.isInitialized) return null
        retained = true
        return Retained(host, renderer.snapshot(), history.toList(), previousRoute)
    }

    override fun onDestroy() {
        if (::host.isInitialized) { if (retained) host.detach() else host.close() }
        super.onDestroy()
    }

    @Deprecated("Android platform Back compatibility")
    override fun onBackPressed() {
        if (!::host.isInitialized) { super.onBackPressed(); return }
        val model = host.state
        when {
            model.optString("modal") == "open" -> host.dispatch(JSONObject().put("kind", "click").put("id", "close-modal"))
            model.optString("menu") == "open" -> host.dispatch(JSONObject().put("kind", "click").put("id", "toggle-menu"))
            history.isNotEmpty() -> {
                val target = history.removeAt(history.lastIndex)
                handlingBack = true
                try { host.dispatch(JSONObject().put("kind", "location").put("value", "#/$target")) } finally { handlingBack = false }
            }
            model.optString("route") != "overview" -> host.dispatch(JSONObject().put("kind", "location").put("value", "#/overview"))
            else -> super.onBackPressed()
        }
    }

    private fun settings() {
        val input = EditText(this).apply { setText(host.baseUrl); hint = "https://bank.example.com"; isSingleLine = true; inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_VARIATION_URI }
        val dialog = AlertDialog.Builder(this).setTitle("Server connection")
            .setMessage("Connect to the same Talon ledger as the web app.")
            .setView(input).setNegativeButton("Cancel", null).setPositiveButton("Connect", null).create()
        dialog.setOnShowListener { dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
            val value = input.text.toString().trim().trimEnd('/')
            if (!BankHost.validBaseUrl(value)) input.error = "Enter HTTPS, or http://127.0.0.1:18790 for the local sample."
            else {
                getPreferences(MODE_PRIVATE).edit().putString("server_url", value).apply()
                intent.putExtra("server_url", value)
                host.close()
                // A new connection starts a fresh session; never migrate pending writes.
                finish(); startActivity(intent); dialog.dismiss()
            }
        } }
        dialog.show()
    }

    private fun showError(error: Throwable) {
        val box = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(32, 64, 32, 32); setBackgroundColor(Color.rgb(247, 247, 242)) }
        box.addView(TextView(this).apply { setText(R.string.startup_failure); textSize = 24f })
        box.addView(TextView(this).apply { text = error.message ?: error.toString(); textSize = 15f })
        setContentView(box)
        android.util.Log.e("TalonBank", "Native bank host error", error)
    }
}
