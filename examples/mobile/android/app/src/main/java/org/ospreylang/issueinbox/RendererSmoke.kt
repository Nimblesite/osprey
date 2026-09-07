package org.ospreylang.issueinbox

import android.content.Context
import android.widget.FrameLayout
import org.json.JSONObject

// A node's latest style must replace the previous style when native views are reused.
internal fun verifyRenderer(context: Context) {
    val container = FrameLayout(context)
    val renderer = NativeRenderer(context) {}
    fun group(style: String) = JSONObject().put("kind", "column").put("style", style)
    renderer.update(container, group("card"))
    val native = container.getChildAt(0)
    check(native.background != null && native.paddingLeft > 0)
    renderer.update(container, group("list"))
    check(container.getChildAt(0) === native) { "Renderer needlessly replaced the native container" }
    check(native.background == null && native.paddingLeft == 0) { "Unstyled column retained the previous card background/padding" }
}
