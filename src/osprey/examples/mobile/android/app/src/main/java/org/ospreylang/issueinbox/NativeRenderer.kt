package org.ospreylang.issueinbox

import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.text.Editable
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextWatcher
import android.text.method.LinkMovementMethod
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import android.text.style.URLSpan
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.widget.*
import org.json.JSONArray
import org.json.JSONObject
import java.util.WeakHashMap

// Generic native projection of the tree built by Osprey Ui. [MOBILE-REACTIVE-UI]
internal class NativeRenderer(private val context: Context, private val dispatch: (JSONObject) -> Unit) {
    private val nodes = WeakHashMap<View, JSONObject>()
    private var rendering = false
    private val ink = Color.rgb(232, 239, 250)
    private val muted = Color.rgb(159, 177, 201)
    private val accent = Color.rgb(70, 223, 194)
    private fun dp(value: Int) = (value * context.resources.displayMetrics.density).toInt()

    fun update(container: FrameLayout, node: JSONObject) {
        rendering = true
        try {
            val tree = render(node, container.getChildAt(0))
            if (tree.parent == null) { container.removeAllViews(); container.addView(tree) }
        } finally { rendering = false }
    }

    private fun render(node: JSONObject, previous: View?): View {
        val same = previous != null && nodes[previous]?.optString("kind") == node.getString("kind") &&
            nodes[previous]?.optString("id") == node.optString("id")
        val view = if (same) previous!! else create(node.getString("kind"))
        nodes[view] = node
        when (view) {
            is EditText -> input(view, node)
            is TextView -> text(view, node)
            is ViewGroup -> children(view, node)
        }
        style(view, node.optString("style"))
        return view
    }

    private fun create(kind: String): View = when (kind) {
        "column", "row" -> LinearLayout(context).apply {
            orientation = if (kind == "row") LinearLayout.HORIZONTAL else LinearLayout.VERTICAL
            gravity = Gravity.CENTER_VERTICAL
        }
        "scroll" -> ScrollView(context).apply { isFillViewport = true }
        "text", "rich" -> TextView(context)
        "button", "link" -> Button(context).apply { isAllCaps = false }
        "input" -> newInput()
        "divider" -> View(context).apply { setBackgroundColor(muted); minimumHeight = dp(1) }
        else -> error("Unknown Osprey UI node kind: $kind")
    }

    private fun children(parent: ViewGroup, node: JSONObject) {
        val children = node.optJSONArray("children") ?: return
        for (index in 0 until children.length()) {
            val childNode = children.getJSONObject(index)
            val previous = parent.getChildAt(index)
            val child = render(childNode, previous)
            if (child !== previous) {
                if (previous != null) parent.removeViewAt(index)
                parent.addView(child, index)
            }
            layout(parent, child, childNode)
        }
        while (parent.childCount > children.length()) parent.removeViewAt(parent.childCount - 1)
    }

    private fun layout(parent: ViewGroup, child: View, node: JSONObject) {
        val row = parent is LinearLayout && parent.orientation == LinearLayout.HORIZONTAL
        val flexible = row && node.getString("kind") in listOf("text", "rich")
        val width = if (flexible) 0 else if (row) -2 else -1
        child.layoutParams = if (parent is LinearLayout) LinearLayout.LayoutParams(width, -2).apply {
            if (flexible) weight = 1f
            setMargins(0, dp(4), if (row) dp(8) else 0, dp(4))
        } else FrameLayout.LayoutParams(-1, -2)
    }

    private fun text(view: TextView, node: JSONObject) {
        val rich = node.getString("kind") == "rich"
        view.text = if (rich) spans(node.optJSONArray("spans") ?: JSONArray()) else node.optString("text")
        if (rich) view.movementMethod = LinkMovementMethod.getInstance()
        view.setOnClickListener(if (node.has("event")) View.OnClickListener { dispatch(node.getJSONObject("event")) }
            else if (node.getString("kind") == "link") View.OnClickListener {
                val uri = Uri.parse(node.getString("url"))
                require(uri.scheme == "https") { "Only HTTPS links are supported" }
                context.startActivity(Intent(Intent.ACTION_VIEW, uri))
            } else null)
    }

    // Implements [MOBILE-MARKDOWN]: spans carry Osprey's emphasis, code and
    // HTTPS link decisions; Android only applies the platform presentation.
    private fun spans(spans: JSONArray): CharSequence {
        val builder = SpannableStringBuilder()
        for (index in 0 until spans.length()) {
            val span = spans.getJSONObject(index)
            val start = builder.length
            builder.append(span.optString("text"))
            val style: Any? = when (span.optString("style")) {
                "bold" -> StyleSpan(Typeface.BOLD)
                "italic" -> StyleSpan(Typeface.ITALIC)
                "code" -> TypefaceSpan("monospace")
                else -> null
            }
            if (style != null) builder.setSpan(style, start, builder.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            val url = span.optString("url")
            if (url.startsWith("https://")) builder.setSpan(URLSpan(url), start, builder.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        return builder
    }

    private fun newInput() = EditText(context).apply {
        setSingleLine(true)
        inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                val node = nodes[this@apply] ?: return
                if (!rendering && !node.optBoolean("submit")) changed(this@apply, node)
            }
            override fun afterTextChanged(s: Editable?) {}
        })
    }

    private fun changed(view: EditText, node: JSONObject) {
        dispatch(JSONObject(node.getJSONObject("event").toString()).put("value", view.text.toString()))
    }

    private fun input(view: EditText, node: JSONObject) {
        view.hint = node.optString("placeholder")
        view.contentDescription = node.optString("id")
        val value = node.optString("value")
        if (!view.hasFocus() && view.text.toString() != value) view.setText(value)
        view.imeOptions = EditorInfo.IME_ACTION_DONE
        view.setOnEditorActionListener { _, action, key ->
            if (node.optBoolean("submit") && (action == EditorInfo.IME_ACTION_DONE ||
                    (key?.keyCode == android.view.KeyEvent.KEYCODE_ENTER && key.action == android.view.KeyEvent.ACTION_UP))) {
                changed(view, node); true
            } else false
        }
    }

    private fun style(view: View, name: String) {
        if (view is TextView) {
            view.setTextColor(when (name) { "accent" -> accent; "caption", "bullet", "quote" -> muted; "error" -> Color.rgb(255, 143, 143); "primary" -> Color.rgb(12, 30, 36); else -> ink })
            view.textSize = when (name) { "hero" -> 36f; "heading" -> 22f; "title" -> 19f; "subheading" -> 18f; "caption", "accent" -> 13f; "code" -> 14f; else -> 16f }
            view.setTypeface(if (name == "code") Typeface.MONOSPACE else null,
                if (name in listOf("hero", "title", "accent", "heading", "subheading")) Typeface.BOLD else Typeface.NORMAL)
            if (view is EditText) view.setHintTextColor(muted)
        }
        if (name in listOf("screen", "card", "primary", "secondary", "code")) {
            view.background = GradientDrawable().apply {
                setColor(when (name) { "screen" -> Color.rgb(12, 20, 34); "primary" -> accent; "code" -> Color.rgb(18, 28, 44); else -> Color.rgb(25, 39, 59) })
                cornerRadius = if (name == "screen") 0f else dp(14).toFloat()
                if (name == "secondary") setStroke(dp(1), Color.rgb(53, 74, 100))
            }
            val padding = dp(when (name) { "screen" -> 20; "code" -> 12; else -> 14 })
            view.setPadding(padding, padding, padding, padding)
        } else if (name == "quote") {
            view.background = null
            view.setPadding(dp(12), 0, 0, 0)
        } else if (view is ViewGroup) {
            view.background = null
            view.setPadding(0, 0, 0, 0)
        }
    }
}
