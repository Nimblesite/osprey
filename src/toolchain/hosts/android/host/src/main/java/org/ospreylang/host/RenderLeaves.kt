package org.ospreylang.host

import android.content.Context
import android.graphics.Color
import android.text.SpannableString
import android.text.Spanned
import android.text.style.StyleSpan
import android.text.style.BackgroundColorSpan
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
// Implements [UI-COMPONENTS]: content nodes, drawn with the platform's own
// views. Text is selectable where the platform makes it so, colours are theme
// tokens, and status always carries words as well as colour. The kinds this
// host does not draw are visible refusals, listed in the host's README.

object RenderLeaves {
    /** One new view of a leaf kind, or null when it is no leaf. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "text", "rich-text", "label", "badge", "pair", "link",
        "segments", "swatch", "empty", "icon" -> TextView(context)
        "progress", "meter" -> LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(context))
            addView(ProgressBar(context, null, android.R.attr.progressBarStyleHorizontal))
        }
        "image" -> ImageView(context)
        "scene" -> android.widget.FrameLayout(context)
        else -> null
    }

    /** Writes `tree` into its leaf view; false when it is no leaf. */
    fun update(context: Context, view: View, tree: Tree, theme: ThemeResolver,
               inherited: Inherited, store: Store): Boolean = when (tree.text("kind")) {
        "text" -> plain(view as TextView, tree, theme, inherited)
        "rich-text" -> rich(view as TextView, tree, theme, inherited)
        "label" -> led(view as TextView, tree, theme, inherited)
        "icon" -> icon(view as TextView, tree, inherited)
        "badge" -> badge(view as TextView, tree, theme)
        "pair" -> pair(view as TextView, tree, theme, inherited)
        "link" -> link(view as TextView, tree, theme, inherited)
        "progress" -> progress(view as LinearLayout, tree, theme, inherited)
        "meter" -> meter(view as LinearLayout, tree, theme, inherited)
        "segments" -> segments(view as TextView, tree, theme)
        "swatch" -> swatch(view as TextView, tree, theme)
        "empty" -> empty(view as TextView, tree, theme, inherited)
        "image" -> picture(view as ImageView, tree)
        "scene" -> scene(context, view as android.widget.FrameLayout, tree, store)
        else -> false
    }

    /** A role's words, styled by tone; an eyebrow is upper-case. */
    private fun plain(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.text = tree.text("role").let { role -> if (role == "eyebrow") tree.text("text").uppercase() else tree.text("text") }
        return true
    }

    /** Inline runs wrap as one paragraph, preserving emphasis and ink tokens. */
    private fun rich(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        val joined = SpannableString(tree.list("runs").joinToString("") { it.text("text") })
        var at = 0
        for (run in tree.list("runs")) {
            val words = run.text("text")
            when (run.text("emphasis")) {
                "strong" -> joined.setSpan(StyleSpan(android.graphics.Typeface.BOLD), at, at + words.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                "code" -> joined.setSpan(BackgroundColorSpan(theme.colour("codeFill") ?: 0x20808080), at, at + words.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
            at += words.length
        }
        view.text = joined
        return true
    }

    /** Text led by a symbol. */
    private fun led(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.text = ledWords(tree, inherited)
        return true
    }

    private fun ledWords(tree: Tree, inherited: Inherited): String {
        val symbol = tree.text("symbol")
        val glyph = Symbols.glyph(symbol)
        if (symbol.isNotEmpty() && glyph == null) return tree.text("label")
        return if (glyph != null) "$glyph ${tree.text("label")}" else tree.text("label")
    }

    private fun icon(view: TextView, tree: Tree, inherited: Inherited): Boolean {
        view.textSize = symbolSize(inherited)
        view.text = Symbols.glyph(tree.text("symbol")) ?: ""
        return true
    }

    /** A status badge: words on the tone's surface, never colour alone. */
    private fun badge(view: TextView, tree: Tree, theme: ThemeResolver): Boolean {
        view.text = ledWords(tree, Inherited())
        view.textSize = 12f
        view.setTextColor(Color.WHITE)
        view.setPadding(24, 6, 24, 6)
        view.background = surfaceOf(theme.tone(tree.text("tone")), 24f, null)
        return true
    }

    /** A label and its value on one line. */
    private fun pair(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.text = "${tree.text("label")}: ${tree.text("value")}"
        return true
    }

    /** A labelled https page the host opens in the browser. */
    private fun link(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.text = tree.text("label")
        view.paintFlags = view.paintFlags or android.graphics.Paint.UNDERLINE_TEXT_FLAG
        view.setTextColor(theme.accent())
        view.setOnClickListener {
            view.context.startActivity(android.content.Intent(
                android.content.Intent.ACTION_VIEW, android.net.Uri.parse(tree.text("url"))))
        }
        return true
    }

    /** Work with no measured end: a spinner and its words. */
    private fun progress(box: LinearLayout, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        val words = box.getChildAt(0) as TextView
        style(words, tree, theme, inherited)
        words.text = listOfNotNull(tree.text("label").ifEmpty { null }, tree.text("detail").ifEmpty { null }).joinToString(" — ")
        val bar = box.getChildAt(1) as ProgressBar
        bar.isIndeterminate = true
        return true
    }

    /** A bar filled to amount out of total, read out with its label. */
    private fun meter(box: LinearLayout, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        val words = box.getChildAt(0) as TextView
        style(words, tree, theme, inherited)
        val total = maxOf(tree.count("total"), 1)
        val part = tree.count("value").coerceIn(0, total)
        words.text = "${tree.text("label")} $part/$total"
        val bar = box.getChildAt(1) as ProgressBar
        bar.isIndeterminate = false
        bar.max = total
        bar.progress = part
        return true
    }

    /** A segmented status strip: each tone one word-long cell. */
    private fun segments(view: TextView, tree: Tree, theme: ThemeResolver): Boolean {
        view.textSize = 12f
        view.text = tree.text("label")
        view.setTextColor(theme.text())
        return true
    }

    /** One theme token, its two faces beside its name. */
    private fun swatch(view: TextView, tree: Tree, theme: ThemeResolver): Boolean {
        view.text = tree.text("name")
        view.textSize = 12f
        view.setTextColor(theme.text())
        return true
    }

    /** An empty state: a symbol, a title and a line of detail. */
    private fun empty(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        style(view, tree, theme, inherited)
        view.gravity = Gravity.CENTER
        view.text = listOf(tree.text("title"), tree.text("detail")).filter { it.isNotEmpty() }
            .joinToString("\n")
        return true
    }

    /** The picture the node names, by path or by https URL. */
    private fun picture(view: ImageView, tree: Tree): Boolean {
        view.scaleType = ImageView.ScaleType.FIT_CENTER
        Images.load(tree.text("source"), view)
        return true
    }

    /** A scene: its shapes arrive in the scene run, named by the record's row. */
    private fun scene(context: Context, box: android.widget.FrameLayout, tree: Tree, store: Store): Boolean {
        val row = tree.count("sceneRow")
        if (box.childCount == 0) box.addView(SceneView(context, store, row, tree.count("event").toLong()))
        (box.getChildAt(0) as SceneView).refresh()
        return true
    }

    private fun style(view: TextView, tree: Tree, theme: ThemeResolver, inherited: Inherited) {
        view.textSize = Type.sp(tree.text("role").ifEmpty { "body" }, inherited)
        view.setTypeface(Type.face(inherited, tree.text("role") in setOf("headline", "heading", "title")))
        view.setTextColor(theme.tone(tree.text("tone").ifEmpty { "primary" }))
    }

    private fun symbolSize(inherited: Inherited): Float =
        if (inherited.symbolSize > 0) inherited.symbolSize.toFloat() else 14f
}

