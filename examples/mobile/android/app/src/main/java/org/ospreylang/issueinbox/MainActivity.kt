package org.ospreylang.issueinbox

import android.app.Activity
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.FrameLayout
import org.json.JSONObject

// Osprey owns the model, transition, view tree and commands. [MOBILE-NATIVE-HOST]
class MainActivity : Activity() {
    private lateinit var container: FrameLayout
    private lateinit var renderer: NativeRenderer
    private lateinit var services: HostServices
    private var model = ""
    private val queue = Handler(Looper.getMainLooper())
    private var smoke: SmokeCheck? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = android.graphics.Color.rgb(12, 20, 34)
        window.navigationBarColor = window.statusBarColor
        container = FrameLayout(this)
        setContentView(container)
        renderer = NativeRenderer(this) { event -> queue.post { dispatch(event) } }
        val phase = intent.getStringExtra("smoke_phase")
        if (phase != null) verifyRenderer(this)
        val live = intent.getBooleanExtra("live", false)
        val fixture = if (phase != null && !live) SmokeCheck.fixture(phase) else null
        services = HostServices(this, if (phase == null) "inbox.sqlite" else "inbox-smoke.sqlite", fixture, ::dispatch)
        if (phase != null) smoke = SmokeCheck(this, phase, live) { event -> queue.post { dispatch(event) } }
        accept(Native.start())
    }

    private fun dispatch(event: JSONObject) {
        check(Looper.myLooper() == Looper.getMainLooper())
        try { accept(Native.dispatch(model.toByteArray(Charsets.UTF_8), event.toString().toByteArray(Charsets.UTF_8))) }
        catch (error: Exception) { Log.e("OspreyInbox", "Osprey host failed", error); throw error }
    }

    private fun accept(bytes: ByteArray) {
        val envelope = JSONObject(bytes.toString(Charsets.UTF_8))
        model = envelope.getString("model")
        renderer.update(container, envelope.getJSONObject("ui"))
        val commands = envelope.getJSONArray("commands")
        for (index in 0 until commands.length()) services.execute(commands.getJSONObject(index))
        smoke?.observe(envelope)
    }

    override fun onDestroy() {
        services.close()
        super.onDestroy()
    }
}
