package org.ospreylang.host

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView

// Implements [UI-COMPONENTS]: status and media nodes, drawn with the
// platform's own views as the SwiftUI host's Leaves.swift draws them. Status
// always carries its words, and a swatch shows both of its faces.

object RenderStatus {
    /** One new view of a status kind, or null when it is no status. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "progress" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        "meter" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "segments" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        "swatch" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "image" -> ImageView(context)
        "scene" -> FrameLayout(context)
        else -> null
    }

    /** Writes `tree` into its status view; false when it is no status. */
    fun update(renderer: ToldRenderer, view: View, tree: Tree,
               theme: ThemeResolver, inherited: Inherited): Boolean = when (tree.text("kind")) {
        "progress" -> progress(view as LinearLayout, tree, theme, inherited)
        "meter" -> meter(view as LinearLayout, tree, theme, inherited)
        "segments" -> segments(view as LinearLayout, tree, theme)
        "swatch" -> swatch(view as LinearLayout, tree, theme)
        "image" -> picture(renderer, view as ImageView, tree)
        "scene" -> scene(renderer, view as FrameLayout, tree)
        "divider" -> rule(view, theme)
        else -> false
    }

    /** The system hairline, on the theme's divider token where it names one. */
    private fun rule(view: View, theme: ThemeResolver): Boolean {
        view.setBackgroundColor(theme.colour("divider") ?: theme.overlay(0.20))
        view.layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 1)
        return true
    }

    /** Work with no measured end: a spinner, its words and its detail. */
    private fun progress(box: LinearLayout, tree: Tree,
                         theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.gravity = android.view.Gravity.CENTER_VERTICAL
            box.addView(ProgressBar(box.context, null, android.R.attr.progressBarStyleSmall),
                        LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            box.addView(TextView(box.context))
            box.addView(TextView(box.context))
        }
        words(box.getChildAt(1) as TextView, tree.text("label"), theme, inherited, secondary = true)
        words(box.getChildAt(2) as TextView, tree.text("detail"), theme, inherited, secondary = false)
        spacing(box)
        return true
    }

    private fun words(view: TextView, text: String, theme: ThemeResolver, inherited: Inherited,
                      secondary: Boolean) {
        view.text = text
        view.textSize = Type.sp(if (secondary) "body" else "callout", inherited)
        view.setTextColor(if (secondary) theme.secondary() else theme.tone("primary"))
    }

    private fun spacing(box: LinearLayout) {
        for (seat in 0 until box.childCount) {
            val params = box.getChildAt(seat).layoutParams as? LinearLayout.LayoutParams ?: continue
            params.leftMargin = if (seat > 0) dp(box.context, 8.0) else 0
            box.getChildAt(seat).layoutParams = params
        }
    }

    /** A bar filled to `value` of `total`, read out with its label. */
    private fun meter(box: LinearLayout, tree: Tree, theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(TextView(box.context))
            box.addView(android.widget.Space(box.context))
            box.addView(Bar(box.context))
        }
        val words = box.getChildAt(0) as TextView
        words.text = tree.text("label")
        words.textSize = Type.sp("callout", inherited)
        words.setTextColor(theme.secondary())
        words.visibility = if (tree.text("label").isEmpty()) View.GONE else View.VISIBLE
        (box.getChildAt(1) as android.widget.Space).layoutParams =
            LinearLayout.LayoutParams(1, dp(box.context, 5.0))
        val bar = box.getChildAt(2) as Bar
        bar.paints(theme.tone(tree.text("tone").ifEmpty { "accent" }).softly(0.16),
                   theme.colour(tree.text("tone").ifEmpty { "accent" }) ?: theme.accent())
        bar.share(tree.count("value").toDouble() / maxOf(tree.count("total"), 1).toDouble())
        return true
    }

    /** A segmented status strip: each tone one equally wide cell. */
    private fun segments(box: LinearLayout, tree: Tree, theme: ThemeResolver): Boolean {
        val tones = tree.list("tones")
        while (box.childCount > tones.size) box.removeViewAt(box.childCount - 1)
        tones.forEachIndexed { index, tone ->
            val cell = box.getChildAt(index) as? View ?: View(box.context).also {
                box.addView(it, LinearLayout.LayoutParams(0, dp(box.context, 5.0), 1f))
            }
            cell.background = surfaceOf(theme.tone(tone.string ?: ""), dp(box.context, 3.0).toFloat(), null)
        }
        box.contentDescription = tree.text("label")
        return true
    }

    /** One theme token, its two faces beside its name. */
    private fun swatch(box: LinearLayout, tree: Tree, theme: ThemeResolver): Boolean {
        if (box.childCount == 0) {
            box.addView(Swatch(box.context), LinearLayout.LayoutParams(dp(box.context, 56.0), dp(box.context, 32.0)))
            box.addView(TextView(box.context))
        }
        (box.getChildAt(0) as Swatch).faces(tree.text("light"), tree.text("dark"))
        val words = box.getChildAt(1) as TextView
        words.text = tree.text("name")
        words.textSize = 10f
        words.setTextColor(theme.secondary())
        return true
    }

    /** The bundled or https picture the node names, at its authored aspect. */
    private fun picture(renderer: ToldRenderer, view: ImageView, tree: Tree): Boolean {
        view.scaleType = ImageView.ScaleType.FIT_CENTER
        view.contentDescription = tree.text("alt")
        Images.load(view.context, tree.text("source"), view)
        return true
    }

    /** A scene: its shapes arrive in the scene run, named by the record's row. */
    private fun scene(renderer: ToldRenderer, box: FrameLayout, tree: Tree): Boolean {
        val row = tree.count("sceneRow")
        if (box.childCount == 0) box.addView(SceneView(renderer.context, renderer.store, row, tree.count("event").toLong()))
        (box.getChildAt(0) as SceneView).refresh()
        return true
    }
}

/** A capsule track with its filled share, as the SwiftUI meter draws it. */
private class Bar(context: Context) : View(context) {
    private var track = 0
    private var fill = 0
    private var share = 0.0

    fun paints(track: Int, fill: Int) {
        this.track = track
        this.fill = fill
    }

    fun share(value: Double) {
        share = value.coerceIn(0.0, 1.0)
    }

    override fun onDraw(canvas: Canvas) {
        val paint = Paint()
        paint.color = track
        val radius = height / 2f
        canvas.drawRoundRect(0f, 0f, width.toFloat(), height.toFloat(), radius, radius, paint)
        paint.color = fill
        canvas.drawRoundRect(0f, 0f, maxOf(4 * resources.displayMetrics.density, width * share.toFloat()),
                             height.toFloat(), radius, radius, paint)
    }
}

/** A token's light and dark faces side by side, in one rounded card. */
private class Swatch(context: Context) : View(context) {
    private var light = 0
    private var dark = 0

    fun faces(lightText: String, darkText: String) {
        light = parse(lightText)
        dark = parse(darkText)
    }

    private fun parse(hex: String): Int {
        val digits = hex.removePrefix("#")
        return if (digits.length == 6) (0xFF000000 or (digits.toLongOrNull(16) ?: return 0)).toInt() else 0
    }

    override fun onDraw(canvas: Canvas) {
        val paint = Paint()
        val radius = dp(context, 6.0).toFloat()
        canvas.save()
        canvas.clipPath(android.graphics.Path().apply {
            addRoundRect(0f, 0f, width.toFloat(), height.toFloat(), radius, radius, android.graphics.Path.Direction.CW)
        })
        paint.color = light
        canvas.drawRect(0f, 0f, width / 2f, height.toFloat(), paint)
        paint.color = dark
        canvas.drawRect(width / 2f, 0f, width.toFloat(), height.toFloat(), paint)
        canvas.restore()
    }
}
