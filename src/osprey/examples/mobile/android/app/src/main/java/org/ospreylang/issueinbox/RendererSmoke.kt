package org.ospreylang.issueinbox

import android.content.Context
import android.graphics.Typeface
import android.text.Spanned
import android.text.style.StyleSpan
import android.text.style.URLSpan
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import org.json.JSONArray
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
    verifyRichSpans(context)
}

// Osprey's Markdown spans must become real Android text spans. [MOBILE-MARKDOWN]
private fun verifyRichSpans(context: Context) {
    fun span(text: String, style: String, url: String) = JSONObject().put("text", text).put("style", style).put("url", url)
    val rich = JSONObject().put("kind", "rich").put("style", "body").put("spans", JSONArray()
        .put(span("plain ", "plain", "")).put(span("bold", "bold", "")).put(span("link", "link", "https://github.com")))
    val container = FrameLayout(context)
    NativeRenderer(context) {}.update(container, JSONObject().put("kind", "column").put("style", "card").put("children", JSONArray().put(rich)))
    val spanned = ((container.getChildAt(0) as ViewGroup).getChildAt(0) as TextView).text as Spanned
    check(spanned.toString() == "plain boldlink") { "Rich text lost span content" }
    check(spanned.getSpans(6, 10, StyleSpan::class.java).any { it.style == Typeface.BOLD }) { "Bold Markdown span was not rendered" }
    check(spanned.getSpans(10, 14, URLSpan::class.java).any { it.url == "https://github.com" }) { "Markdown link was not rendered" }
}
