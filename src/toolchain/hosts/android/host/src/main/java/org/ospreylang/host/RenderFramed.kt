package org.ospreylang.host

import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.PixelFormat
import android.view.Gravity
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView

// Implements [UI-COMPONENTS-FRAMED] as the SwiftUI host's Layout.swift draws
// a framed node: the application's insets, dimensions and surfaces, applied
// mechanically, with the frame's own metrics inherited by its body.

object RenderFramed {
    /** Writes the framed node into its box. */
    fun draw(renderer: ToldRenderer, box: FrameLayout, tree: Tree, path: String,
             theme: ThemeResolver, inherited: Inherited) {
        val style = tree["style"]
        val inner = inherited.merging(style)
        val body = renderer.node(tree["body"] ?: Tree.Null, "$path/body", theme, inner)
        held(renderer, box, body, style)
        inset(renderer, box, style)
    }

    /** The body inside its frame: scrolling when it asks to, seated as the frame aligns it. */
    private fun held(renderer: ToldRenderer, box: FrameLayout, body: android.view.View, style: Tree?) {
        if (style?.flag("scrollY") == true) {
            val scroller = box.getChildAt(0) as? ScrollView
                ?: ScrollView(box.context).also { box.addView(it, 0, FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)) }
            if (scroller.getChildAt(0) !== body) {
                scroller.removeAllViews()
                scroller.addView(body, ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            }
        } else if (box.getChildAt(0) !== body) {
            box.removeAllViews()
            box.addView(body, FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, seat(style)))
        }
    }

    /** How the body sits on the cross and main axes the frame spells. */
    private fun seat(style: Tree?): Int {
        var gravity = Gravity.TOP or Gravity.START
        style ?: return gravity
        gravity = cross(style.text("cross"), gravity)
        return main(style.text("align"), gravity)
    }

    private fun cross(spelled: String, gravity: Int): Int = when (spelled) {
        "center" -> gravity and Gravity.HORIZONTAL_GRAVITY_MASK.inv() or Gravity.CENTER_HORIZONTAL
        "end" -> gravity and Gravity.HORIZONTAL_GRAVITY_MASK.inv() or Gravity.END
        else -> gravity
    }

    private fun main(spelled: String, gravity: Int): Int = when (spelled) {
        "center" -> gravity and Gravity.VERTICAL_GRAVITY_MASK.inv() or Gravity.CENTER_VERTICAL
        "end" -> gravity and Gravity.VERTICAL_GRAVITY_MASK.inv() or Gravity.BOTTOM
        else -> gravity
    }

    /** The frame's insets, extents, fill and line, and its fill of its parent. */
    private fun inset(renderer: ToldRenderer, box: FrameLayout, style: Tree?) {
        style ?: return
        box.setPadding(style.count("left"), style.count("top"), style.count("right"), style.count("bottom"))
        sized(box, style)
        dress(renderer, box, style)
    }

    /** A frame's fixed extents, its hug, and its fill of the page when it grows. */
    private fun sized(box: FrameLayout, style: Tree) {
        val width = style.count("width")
        val height = style.count("height")
        val grows = style.flag("grow")
        val params = LinearLayout.LayoutParams(
            if (grows) ViewGroup.LayoutParams.MATCH_PARENT
            else if (width > 0) dp(box.context, width.toDouble()) else ViewGroup.LayoutParams.WRAP_CONTENT,
            if (grows) 0 else if (height > 0) dp(box.context, height.toDouble())
            else ViewGroup.LayoutParams.WRAP_CONTENT)
        if (grows) params.weight = 1f
        box.layoutParams = params
    }

    /** The frame's surface and outline: a rounded fill and line, or one edge's line alone. */
    private fun dress(renderer: ToldRenderer, box: FrameLayout, style: Tree) {
        val face = renderer.theme()
        val fill = face.colour(style.text("fill"))
        val line = face.colour(style.text("line"))
        val radius = dp(box.context, style.count("radius").toDouble()).toFloat()
        box.background = when {
            line != null && style.text("lineEdge").isEmpty() ->
                surfaceOf(fill, radius, null).apply { setStroke(dp(box.context, 1.0), line) }
            fill != null -> surfaceOf(fill, radius, null)
            else -> null
        }
        box.foreground = if (line != null && style.text("lineEdge").isNotEmpty())
            Edge(style.text("lineEdge"), line, dp(box.context, 1.0))
        else null
    }
}

/** A frame's outline on one edge, as Layout.swift's single-edge lines draw. */
private class Edge(private val edge: String, ink: Int, thick: Int) : android.graphics.drawable.Drawable() {
    private val paint = android.graphics.Paint().apply {
        color = ink
        strokeWidth = thick.toFloat()
    }

    override fun draw(canvas: Canvas) {
        val w = bounds.width().toFloat()
        val h = bounds.height().toFloat()
        when (edge) {
            "top" -> canvas.drawLine(0f, 0f, w, 0f, paint)
            "bottom" -> canvas.drawLine(0f, h, w, h, paint)
            "left" -> canvas.drawLine(0f, 0f, 0f, h, paint)
            else -> canvas.drawLine(w, 0f, w, h, paint)
        }
    }

    override fun setAlpha(alpha: Int) = Unit
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT
    override fun setColorFilter(filter: ColorFilter?) = Unit
}
