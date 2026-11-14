package org.ospreylang.host

import android.app.Activity
import android.util.Log
import java.io.File

// Implements [NATIVE-SHM-VIEW]'s headless check, as the SwiftUI host's
// SelfCheck.swift does: the application opens with no person present, the
// first frame must hold at least one row with nothing refused, and pressing
// the first actionable control must draw a second frame. A frame of told
// components carries no row actions: its first told control — a button
// before any other — is found in the regions and delivered by its ordinal.
// The verdict goes to the app's own files and to logcat, tagged OspreyHost.

object ViewCheck {
    private const val TAG = "OspreyHost"

    /** The kinds that carry a control event, in the order a frame draws them. */
    private val CONTROLS = setOf("button", "field", "toggle", "picker", "disclosure", "alert",
                                 "dialog", "tabs", "rows", "aside")

    /** Runs the whole check and writes the verdict; no window opens. */
    fun run(activity: Activity, store: Store) {
        val verdict = try {
            checked(store, activity)
        } catch (error: Throwable) {
            "OSPREY_ANDROID_VIEW_FAIL ${error.message}"
        }
        write(activity, verdict)
        Log.i(TAG, verdict)
        activity.finish()
    }

    private fun checked(store: Store, activity: Activity): String {
        store.start(emptyList(), false, "android", activity.filesDir.path,
                    java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000L)
        store.failure?.let { return "OSPREY_ANDROID_VIEW_FAIL $it" }
        if (store.diagnostics.isNotEmpty())
            return "OSPREY_ANDROID_VIEW_FAIL ${store.diagnostics.last()}"
        val rows = store.count
        if (rows == 0) return "OSPREY_ANDROID_VIEW_FAIL the first frame drew no rows"
        val before = store.revision
        pressFirst(store) ?: return "OSPREY_ANDROID_VIEW_FAIL no actionable control in the first frame"
        store.failure?.let { return "OSPREY_ANDROID_VIEW_FAIL $it" }
        if (store.revision <= before) return "OSPREY_ANDROID_VIEW_FAIL no second frame arrived"
        return "OSPREY_ANDROID_VIEW_OK rows=$rows"
    }

    /** The first control that delivers one: a plain row's action, or a told
     * component's event ordinal — a button before any other control. */
    private fun pressFirst(store: Store): Touch? {
        for (index in 0 until store.count) {
            val control = store.control(index) ?: continue
            if (control.action == null || control.has(Flags.DISABLED)) continue
            val words = when (control.kind) {
                Kind.TOGGLE -> if (control.has(Flags.CHOSEN)) "false" else "true"
                else -> ""
            }
            store.send(Touch.control(control.action, words))
            return Touch.control(control.action, words)
        }
        return toldPress(store)
    }

    /** The first told control of the frame's regions, a button before any other. */
    private fun toldPress(store: Store): Touch? {
        val found = mutableListOf<Tree>()
        for (region in store.regions.values) controls(region.node, found, 0)
        val node = found.firstOrNull { it.text("kind") == "button" && it.flag("enabled") }
            ?: found.firstOrNull() ?: return null
        return Touch.control(node.count("event").toLong(), delivered(node)).also { store.send(it) }
    }

    /** Every control under `node`, depth first in the order the walk told them. */
    private fun controls(node: Tree, found: MutableList<Tree>, depth: Int) {
        if (depth > 64) return
        if (node.text("kind") in CONTROLS && node["event"] != null) found.add(node)
        for (name in listOf("children", "body", "toolbar", "items", "child", "pane", "aside"))
            node.list(name).forEach { controls(it, found, depth + 1) }
    }

    /** The words the control's binding expects: a tap's emptiness, a flip, a key or the field's words. */
    private fun delivered(node: Tree): String = when (node.text("kind")) {
        "toggle" -> (!node.flag("on")).toString()
        "disclosure" -> (!node.flag("expanded")).toString()
        "picker" -> node.list("choices").firstOrNull()?.text("key") ?: ""
        "field" -> node.text("value")
        else -> ""
    }

    /** The verdict line, newest in the app's own files folder. */
    private fun write(activity: Activity, verdict: String) {
        try {
            val file = File(activity.filesDir, "view-check.txt")
            file.parentFile?.mkdirs()
            file.writeText(verdict + "\n")
        } catch (error: Exception) {
            Log.e(TAG, "Could not write the view check", error)
        }
    }
}
