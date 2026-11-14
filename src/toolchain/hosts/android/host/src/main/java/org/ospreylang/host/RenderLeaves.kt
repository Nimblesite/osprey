package org.ospreylang.host

import android.content.Context
import android.graphics.Paint
import android.text.SpannableString
import android.text.Spanned
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.Space
import android.widget.TextView

// Implements [UI-COMPONENTS]: content nodes, drawn with the platform's own
// views as the SwiftUI host's Leaves.swift draws them. Words are styled by
// role and tone through the theme, and status always carries words as well as
// colour. Kinds this host does not draw are visible refusals, listed in the
// host's README.

object RenderLeaves {
    /** One new view of a leaf kind, or null when it is no leaf. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "text", "rich-text", "label", "badge", "link" -> TextView(context)
        "icon" -> ImageView(context)
        "pair" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        "empty" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        else -> null
    }

    /** Writes `tree` into its leaf view; false when it is no leaf. */
    fun update(renderer: ToldRenderer, view: View, tree: Tree,
               theme: ThemeResolver, inherited: Inherited): Boolean = when (tree.text("kind")) {
        "text" -> plain(view as TextView, tree, theme, inherited)
        "rich-text" -> rich(view as TextView, tree, theme, inherited)
        "label" -> led(view as TextView, tree, theme, inherited)
        "icon" -> icon(renderer, view as ImageView, tree, inherited)
        "badge" -> badge(view as TextView, tree, theme, inherited)
        "pair" -> pair(view as LinearLayout, tree, theme, inherited)
        "link" -> link(view as TextView, tree, theme, inherited)
        "empty" -> empty(renderer, view as LinearLayout, tree, theme, inherited)
        else -> false
    }

    /** A role's words, styled by tone; an eyebrow is upper-case. */
    private fun plain(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.text = tree.text("role").let { role ->
            if (role == "eyebrow") tree.text("text").uppercase() else tree.text("text")
        }
        return true
    }

    /** Inline runs wrap as one paragraph, preserving emphasis, ink tokens and slants. */
    private fun rich(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        val runs = tree.list("runs")
        val joined = SpannableString(runs.joinToString("") { it.text("text") })
        var at = 0
        for (run in runs) {
            val words = run.text("text")
            when (run.text("emphasis")) {
                "strong" -> joined.setSpan(StyleSpan(android.graphics.Typeface.BOLD), at, at + words.length,
                                           Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                "code" -> {
                    joined.setSpan(TypefaceSpan("monospace"), at, at + words.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    theme.colour("codeFill")?.let { fill ->
                        joined.setSpan(BackgroundColorSpan(fill), at, at + words.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    }
                }
                else -> if (run.flag("italic"))
                    joined.setSpan(StyleSpan(android.graphics.Typeface.ITALIC), at, at + words.length,
                                   Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
            if (run.text("ink").isNotEmpty()) theme.colour(run.text("ink"))?.let { ink ->
                joined.setSpan(ForegroundColorSpan(ink), at, at + words.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
            at += words.length
        }
        view.text = joined
        return true
    }

    /** Text led by a symbol. */
    private fun led(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.text = tree.text("label")
        lead(view, tree.text("symbol"), symbolSize(inherited, 12))
        return true
    }

    /** A decorative symbol, hidden from accessibility. */
    private fun icon(renderer: ToldRenderer, view: ImageView, tree: Tree, inherited: Inherited): Boolean {
        val size = symbolSize(inherited, 14)
        view.layoutParams = LinearLayout.LayoutParams(dp(view.context, size.toDouble()), dp(view.context, size.toDouble()))
        view.setImageDrawable(glyph(renderer, view.context, tree.text("symbol"),
                                    renderer.theme().text(), size))
        view.contentDescription = ""
        return true
    }

    /** A status badge: words on the tone's soft surface, never colour alone. */
    private fun badge(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        view.textSize = 11f
        view.setTypeface(Type.face(inherited, "headline"))
        view.setTextColor(theme.tone(tree.text("tone")))
        view.text = tree.text("label")
        lead(view, tree.text("symbol"), 9)
        view.setPadding(dp(view.context, 8.0), dp(view.context, 3.0), dp(view.context, 8.0), dp(view.context, 3.0))
        view.background = surfaceOf(theme.tone(tree.text("tone")).softly(0.14), view.lineHeight / 2f, null)
        return true
    }

    /** A label and its value on one line, the label in the secondary ink. */
    private fun pair(box: LinearLayout, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.gravity = Gravity.CENTER_VERTICAL or Gravity.START
            box.addView(TextView(box.context))
            box.addView(Space(box.context).apply { minimumWidth = dp(box.context, 8.0) },
                        LinearLayout.LayoutParams(0, 1).apply { weight = 1f })
            box.addView(TextView(box.context))
        }
        val label = box.getChildAt(0) as TextView
        label.text = tree.text("label")
        label.textSize = Type.sp("body", inherited)
        label.setTypeface(Type.face(inherited, "body"))
        label.setTextColor(theme.secondary())
        val value = box.getChildAt(2) as TextView
        value.text = tree.text("value")
        value.textSize = Type.sp("body", inherited)
        value.setTypeface(Type.face(inherited, "body"))
        value.setTextColor(theme.tone("primary"))
        return true
    }

    /** A labelled https page the host opens in the browser; anything else is a refusal. */
    private fun link(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        if (!tree.text("url").startsWith("https://")) {
            view.text = "This host does not draw link to ${tree.text("url")} nodes"
            view.textSize = 12f
            view.setTextColor(0xFFB00020.toInt())
            return true
        }
        style(view, tree, theme, inherited)
        view.text = tree.text("label")
        view.paintFlags = view.paintFlags or Paint.UNDERLINE_TEXT_FLAG
        view.setTextColor(theme.accent())
        view.setOnClickListener {
            view.context.startActivity(android.content.Intent(
                android.content.Intent.ACTION_VIEW, android.net.Uri.parse(tree.text("url"))))
        }
        return true
    }

    /** An empty state: a symbol, a title and a line of detail, centred. */
    private fun empty(renderer: ToldRenderer, box: LinearLayout, tree: Tree,
                      theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.gravity = Gravity.CENTER
            box.addView(ImageView(box.context))
            box.addView(TextView(box.context))
            box.addView(TextView(box.context))
        }
        (box.getChildAt(0) as ImageView).setImageDrawable(
            glyph(renderer, box.context, tree.text("symbol"), theme.secondary(), 28))
        title(box.getChildAt(1) as TextView, tree.text("title"), theme)
        detail(box.getChildAt(2) as TextView, tree.text("detail"), theme)
        return true
    }

    private fun title(view: TextView, text: String, theme: ThemeResolver) {
        view.text = text
        view.textSize = Type.sp("headline", Inherited())
        view.setTypeface(Type.face(Inherited(), "headline"))
        view.setTextColor(theme.tone("primary"))
        view.gravity = Gravity.CENTER
    }

    private fun detail(view: TextView, text: String, theme: ThemeResolver) {
        view.text = text
        view.textSize = Type.sp("callout", Inherited())
        view.setTextColor(theme.secondary())
        view.gravity = Gravity.CENTER
    }

    /** The face every leaf of words wears: role size and weight, tone ink, tracking and leading. */
    fun style(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited) {
        val role = tree.text("role").ifEmpty { "body" }
        view.textSize = Type.sp(role, inherited)
        view.setTypeface(Type.face(inherited, role))
        val ink = if (inherited.inkName.isNotEmpty()) theme.colour(inherited.inkName) else null
        view.setTextColor(ink ?: theme.tone(tree.text("tone").ifEmpty { "primary" }))
        val tracking = if (inherited.tracking != 0.0) inherited.tracking
                       else if (role == "eyebrow") 0.6 else 0.0
        if (tracking != 0.0) view.letterSpacing = tracking.toFloat() / Type.sp(role, inherited)
        val size = if (inherited.size > 0) inherited.size else 13
        if (inherited.lineHeight > size)
            view.setLineSpacing(dp(view.context, inherited.lineHeight - size * 1.2).toFloat(), 1f)
    }

    /** One symbol leading words, tinted with them; a name without a glyph draws none. */
    fun lead(view: TextView, symbol: String, size: Int) {
        view.compoundDrawablePadding = dp(view.context, 6.0)
        if (symbol.isEmpty() || !Symbols.known(symbol)) {
            view.setCompoundDrawables(null, null, null, null)
            return
        }
        val glyph = Symbols.drawable(view.context, symbol, view.currentTextColor, size)
        view.setCompoundDrawables(glyph, null, null, null)
    }

    fun symbolSize(inherited: Inherited, fallback: Int): Int =
        if (inherited.symbolSize > 0) inherited.symbolSize else fallback

    /** The glyph for `name`, or a diagnostic the first time it is missing. */
    fun glyph(renderer: ToldRenderer, context: Context, name: String, tint: Int, size: Int) =
        Symbols.drawable(context, name, tint, size).also {
            if (it == null && name.isNotEmpty()) renderer.store.diagnoseOnce("no icon for symbol $name")
        }
}
