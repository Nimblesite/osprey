package org.ospreylang.host

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.DashPathEffect
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Shader
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import kotlin.math.abs
import kotlin.math.pow

// Implements [NATIVE-SHM-SCENE] for the Android host: the scene a canvas row
// claims, drawn with the platform's own canvas, and its pointer, key and
// measure events delivered as typed calls that name the canvas by its ordinal.
// A zoom of 0 fits the scene to its region whole.

class SceneView(context: Context, private val store: Store, private val row: Int,
                private val ordinal: Long) : View(context) {
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private var scale = 1.0f
    private var sentWidth = 0.0
    private var sentHeight = 0.0

    /** Reads the scene the frame gave this canvas and draws it again. */
    fun refresh() {
        invalidate()
    }

    override fun onSizeChanged(width: Int, height: Int, was: Int, were: Int) {
        if (isShown && (abs(width - sentWidth) >= 1.0 || abs(height - sentHeight) >= 1.0)) {
            sentWidth = width.toDouble()
            sentHeight = height.toDouble()
            store.send(Touch.viewed(ordinal, width / scale.toDouble(), height / scale.toDouble()))
        }
    }

    override fun onDraw(canvas: Canvas) {
        val scene = store.scene(row) ?: return
        scale = zoomOf(scene, width, height)
        canvas.save()
        canvas.scale(scale, scale)
        scene.shapes.forEach { draw(canvas, it, scene) }
        canvas.restore()
    }

    /** Window pixels per scene unit; 0 fits the scene whole. */
    private fun zoomOf(scene: Scene, width: Int, height: Int): Float {
        if (scene.zoom > 0) return scene.zoom.toFloat()
        if (scene.width <= 0 || scene.height <= 0 || width == 0 || height == 0) return 1f
        return minOf(width / scene.width, height / scene.height).toFloat()
    }

    private fun draw(canvas: Canvas, shape: Shape, scene: Scene) {
        when (shape) {
            is Shape.Rect -> {
                canvas.save()
                canvas.rotate(shape.angle.toFloat(), (shape.x + shape.w / 2).toFloat(), (shape.y + shape.h / 2).toFloat())
                fill.color = argb(shape.colour)
                canvas.drawRect(shape.x.toFloat(), shape.y.toFloat(), (shape.x + shape.w).toFloat(), (shape.y + shape.h).toFloat(), fill)
                canvas.restore()
            }
            is Shape.Disc -> {
                fill.color = argb(shape.colour)
                canvas.drawCircle(shape.x.toFloat(), shape.y.toFloat(), shape.radius.toFloat(), fill)
            }
            is Shape.Strip -> strip(canvas, shape)
            is Shape.Glyphs -> glyphs(canvas, shape.text, shape.x, shape.y, shape.size, shape.colour, 0)
            is Shape.Lettering -> glyphs(canvas, shape.text, shape.x, shape.y, shape.size, shape.colour, shape.anchor)
            is Shape.Sprite -> sprite(canvas, shape, scene)
            is Shape.Path -> path(canvas, shape)
        }
    }

    private fun strip(canvas: Canvas, shape: Shape.Strip) {
        stroke.color = argb(shape.colour)
        stroke.strokeWidth = shape.stroke.toFloat()
        val walked = Path()
        shape.points.firstOrNull()?.let { walked.moveTo(it.first.toFloat(), it.second.toFloat()) }
        shape.points.drop(1).forEach { walked.lineTo(it.first.toFloat(), it.second.toFloat()) }
        canvas.drawPath(walked, stroke)
    }

    private fun glyphs(canvas: Canvas, text: String, x: Double, y: Double, size: Double, colour: Long, anchor: Long) {
        if (text.isEmpty()) return
        fill.color = argb(colour)
        fill.textSize = size.toFloat()
        val at = x.toFloat() - when (anchor) {
            1L -> fill.measureText(text) / 2
            2L -> fill.measureText(text)
            else -> 0f
        }
        canvas.drawText(text, at, y.toFloat(), fill)
    }

    private fun sprite(canvas: Canvas, shape: Shape.Sprite, scene: Scene) {
        val source = scene.atlas.getOrNull(shape.atlas) ?: return
        Sprites.bitmap(source)?.let { held ->
            canvas.save()
            canvas.rotate(shape.angle.toFloat(), (shape.x + shape.w / 2).toFloat(), (shape.y + shape.h / 2).toFloat())
            canvas.drawBitmap(held, null, android.graphics.RectF(shape.x.toFloat(), shape.y.toFloat(),
                (shape.x + shape.w).toFloat(), (shape.y + shape.h).toFloat()), fill)
            canvas.restore()
        }
    }

    private fun path(canvas: Canvas, shape: Shape.Path) {
        val walked = walked(shape)
        val paint = paint(shape)
        when (val outline = shape.outline) {
            is Outline.Fill -> canvas.drawPath(walked, paint.apply { style = Paint.Style.FILL })
            is Outline.Stroke -> canvas.drawPath(walked, paint.apply {
                style = Paint.Style.STROKE
                strokeWidth = outline.width.toFloat()
                strokeJoin = if (outline.bevel) Paint.Join.ROUND else Paint.Join.MITER
                if (outline.dashes.isNotEmpty()) pathEffect = DashPathEffect(outline.dashes.map { it.toFloat() }.toFloatArray(), 0f)
            })
        }
    }

    /** One path's steps, in scene units, with its move-to origin applied.
     * The platform's path has no SVG arc, so an arc step lands as cubics. */
    private fun walked(shape: Shape.Path): Path {
        val path = Path()
        var x = shape.moved?.first?.toFloat() ?: 0f
        var y = shape.moved?.second?.toFloat() ?: 0f
        for (step in shape.steps) {
            addStep(path, x, y, step)
            x = lastX(step, x)
            y = lastY(step, y)
        }
        return path
    }

    private fun addStep(path: Path, x: Float, y: Float, step: Step) {
        when (step) {
            is Step.Move -> path.moveTo(step.x.toFloat(), step.y.toFloat())
            is Step.Line -> path.lineTo(step.x.toFloat(), step.y.toFloat())
            is Step.Curve -> path.quadTo(step.cx.toFloat(), step.cy.toFloat(), step.x.toFloat(), step.y.toFloat())
            is Step.Cubic -> path.cubicTo(step.x1.toFloat(), step.y1.toFloat(), step.x2.toFloat(),
                                          step.y2.toFloat(), step.x.toFloat(), step.y.toFloat())
            is Step.Arc -> arc(path, x, y, step)
            is Step.Polygon -> step.points.forEachIndexed { index, point ->
                if (index == 0) path.moveTo(point.first.toFloat(), point.second.toFloat())
                else path.lineTo(point.first.toFloat(), point.second.toFloat())
            }
            is Step.Close -> path.close()
        }
    }

    private fun lastX(step: Step, was: Float) = when (step) {
        is Step.Move -> step.x.toFloat(); is Step.Line -> step.x.toFloat(); is Step.Curve -> step.x.toFloat()
        is Step.Cubic -> step.x.toFloat(); is Step.Arc -> step.x.toFloat(); is Step.Close -> was
        is Step.Polygon -> step.points.lastOrNull()?.first?.toFloat() ?: was
    }

    private fun lastY(step: Step, was: Float) = when (step) {
        is Step.Move -> step.y.toFloat(); is Step.Line -> step.y.toFloat(); is Step.Curve -> step.y.toFloat()
        is Step.Cubic -> step.y.toFloat(); is Step.Arc -> step.y.toFloat(); is Step.Close -> was
        is Step.Polygon -> step.points.lastOrNull()?.second?.toFloat() ?: was
    }

    /** One SVG arc, in at most quarter-turn cubic pieces. */
    private fun arc(path: Path, x0: Float, y0: Float, step: Step.Arc) {
        val x2 = step.x.toFloat()
        val y2 = step.y.toFloat()
        var rx = kotlin.math.abs(step.rx.toFloat())
        var ry = kotlin.math.abs(step.ry.toFloat())
        if (rx == 0f || ry == 0f || (x0 == x2 && y0 == y2)) {
            path.lineTo(x2, y2)
            return
        }
        val phi = Math.toRadians(step.rotation.toDouble())
        val cosPhi = kotlin.math.cos(phi).toFloat()
        val sinPhi = kotlin.math.sin(phi).toFloat()
        val turned = turnedHalf(x0, y0, x2, y2, cosPhi, sinPhi)
        val grown = stretchOf(turned, rx, ry)
        rx *= grown
        ry *= grown
        val centre = centreOf(x0, y0, x2, y2, rx, ry, cosPhi, sinPhi, step, turned.first, turned.second)
        sweepOf(path, x0, y0, x2, y2, centre, rx, ry, cosPhi, sinPhi, step)
    }

    /** The endpoints' half-displacement, rotated out of the arc's own frame. */
    private fun turnedHalf(x0: Float, y0: Float, x2: Float, y2: Float,
                           cosPhi: Float, sinPhi: Float): Pair<Float, Float> {
        val halfX = (x0 - x2) / 2
        val halfY = (y0 - y2) / 2
        return (cosPhi * halfX + sinPhi * halfY) to (-sinPhi * halfX + cosPhi * halfY)
    }

    /** The radii grow to reach endpoints their original size cannot. */
    private fun stretchOf(turned: Pair<Float, Float>, rx: Float, ry: Float): Float {
        val stretch = (turned.first / rx).toDouble().pow(2) + (turned.second / ry).toDouble().pow(2)
        return if (stretch > 1.0) kotlin.math.sqrt(stretch).toFloat() else 1f
    }

    /** The arc's centre, from the endpoint-to-centre conversion. */
    private fun centreOf(x0: Float, y0: Float, x2: Float, y2: Float, rx: Float, ry: Float,
                         cosPhi: Float, sinPhi: Float, step: Step.Arc, x1: Float, y1: Float): Pair<Float, Float> {
        val numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
        val denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1
        val sign = if (step.large != step.sweep) 1f else -1f
        val held = if (denominator == 0f) 0f else numerator / denominator
        val multiple = sign * kotlin.math.sqrt(kotlin.math.max(0f, held))
        val cx = multiple * rx * y1 / ry
        val cy = -multiple * ry * x1 / rx
        return (cosPhi * cx - sinPhi * cy + (x0 + x2) / 2) to (sinPhi * cx + cosPhi * cy + (y0 + y2) / 2)
    }

    private fun sweepOf(path: Path, x0: Float, y0: Float, x2: Float, y2: Float,
                        centre: Pair<Float, Float>, rx: Float, ry: Float,
                        cosPhi: Float, sinPhi: Float, step: Step.Arc) {
        val ux = (x0 - centre.first) / rx
        val uy = (y0 - centre.second) / ry
        val vx = (x2 - centre.first) / rx
        val vy = (y2 - centre.second) / ry
        var theta = turning(1f, 0f, ux, uy)
        var delta = turning(ux, uy, -vx, -vy)
        val tau = 2.0 * Math.PI
        if (!step.sweep && delta > 0) delta -= tau
        if (step.sweep && delta < 0) delta += tau
        val pieces = kotlin.math.ceil(kotlin.math.abs(delta) / (Math.PI / 2)).toInt().coerceAtLeast(1)
        for (piece in 0 until pieces) {
            val from = theta + delta * piece / pieces
            val to = theta + delta * (piece + 1) / pieces
            cubic(path, centre, rx, ry, cosPhi, sinPhi, from, to)
        }
    }

    /** The signed angle from one unit vector to another. */
    private fun turning(ux: Double, uy: Double, vx: Double, vy: Double): Double {
        val dot = ux * vx + uy * vy
        val length = kotlin.math.sqrt(ux * ux + uy * uy) * kotlin.math.sqrt(vx * vx + vy * vy)
        val angle = kotlin.math.acos((dot / length).coerceIn(-1.0, 1.0))
        return if (ux * vy - uy * vx < 0) -angle else angle
    }

    private fun turning(ux: Float, uy: Float, vx: Float, vy: Float) =
        turning(ux.toDouble(), uy.toDouble(), vx.toDouble(), vy.toDouble())

    /** One cubic piece of the arc, between `from` and `to`. */
    private fun cubic(path: Path, centre: Pair<Float, Float>, rx: Float, ry: Float,
                      cosPhi: Float, sinPhi: Float, from: Double, to: Double) {
        val alpha = 4.0 / 3.0 * kotlin.math.tan((to - from) / 4)
        val at = pointAt(centre, rx, ry, cosPhi, sinPhi, from)
        val near = pointAt(centre, rx, ry, cosPhi, sinPhi, to)
        val slopeAt = slopeAt(rx, ry, cosPhi, sinPhi, from)
        val slopeNear = slopeAt(rx, ry, cosPhi, sinPhi, to)
        path.cubicTo(
            (at.first + alpha * slopeAt.first).toFloat(), (at.second + alpha * slopeAt.second).toFloat(),
            (near.first - alpha * slopeNear.first).toFloat(), (near.second - alpha * slopeNear.second).toFloat(),
            near.first.toFloat(), near.second.toFloat())
    }

    private fun pointAt(centre: Pair<Float, Float>, rx: Float, ry: Float, cosPhi: Float,
                        sinPhi: Float, angle: Double): Pair<Double, Double> {
        val x = rx * kotlin.math.cos(angle)
        val y = ry * kotlin.math.sin(angle)
        return ((cosPhi * x - sinPhi * y) + centre.first).toDouble() to
            ((sinPhi * x + cosPhi * y) + centre.second).toDouble()
    }

    private fun slopeAt(rx: Float, ry: Float, cosPhi: Float, sinPhi: Float, angle: Double): Pair<Double, Double> {
        val x = -rx * kotlin.math.sin(angle)
        val y = ry * kotlin.math.cos(angle)
        return (cosPhi * x - sinPhi * y).toDouble() to (sinPhi * x + cosPhi * y).toDouble()
    }

    /** A path's paint: solid tint or a two-stop linear gradient. */
    private fun paint(shape: Shape.Path): android.graphics.Paint {
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG)
        when (val painted = shape.paint) {
            is PaintStyle.Solid -> paint.color = tint(painted.tint)
            is PaintStyle.Linear -> paint.shader = gradient(painted)
        }
        return paint
    }

    private fun gradient(painted: PaintStyle.Linear): Shader {
        val radians = Math.toRadians(painted.angle)
        val dx = kotlin.math.cos(radians).toFloat() * 512f
        val dy = kotlin.math.sin(radians).toFloat() * 512f
        return LinearGradient(256f - dx, 256f - dy, 256f + dx, 256f + dy,
                              tint(painted.from), tint(painted.to), Shader.TileMode.CLAMP)
    }

    private fun tint(value: Tint): Int = when (value) {
        is Tint.Rgb -> Color.argb((value.a * 255).toInt().coerceIn(0, 255), (value.r * 255).toInt().coerceIn(0, 255),
                                  (value.g * 255).toInt().coerceIn(0, 255), (value.b * 255).toInt().coerceIn(0, 255))
        is Tint.Hsl -> {
            val hue = (value.h * 360).toFloat()
            val sat = value.s.toFloat().coerceIn(0f, 1f)
            val light = value.l.toFloat().coerceIn(0f, 1f)
            android.graphics.Color.HSVToColor((value.a * 255).toInt().coerceIn(0, 255), floatArrayOf(hue, sat, light))
        }
    }

    /** A packed `0xRRGGBBAA` colour. */
    private fun argb(packed: Long): Int =
        Color.argb((packed and 0xFF).toInt(), ((packed shr 24) and 0xFF).toInt(),
                   ((packed shr 16) and 0xFF).toInt(), ((packed shr 8) and 0xFF).toInt())

    override fun onTouchEvent(motion: MotionEvent): Boolean {
        val x = motion.x / scale.toDouble()
        val y = motion.y / scale.toDouble()
        when (motion.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE ->
                store.send(Touch.pointer(ordinal, x, y, true, false))
            MotionEvent.ACTION_UP ->
                store.send(Touch.pointer(ordinal, x, y, false, false))
            else -> return false
        }
        return true
    }

    override fun onKeyDown(code: Int, event: KeyEvent): Boolean {
        store.send(Touch.key(ordinal, event.displayLabel.toString().ifEmpty { keyCodeName(code) }, true))
        return true
    }

    override fun onKeyUp(code: Int, event: KeyEvent): Boolean {
        store.send(Touch.key(ordinal, event.displayLabel.toString().ifEmpty { keyCodeName(code) }, false))
        return true
    }

    private fun keyCodeName(code: Int) = KeyEvent.keyCodeToString(code).removePrefix("KEYCODE_").lowercase()
}

/** Atlas images by source, decoded once. */
object Sprites {
    private val bitmaps = HashMap<String, android.graphics.Bitmap?>()

    fun bitmap(source: String): android.graphics.Bitmap? {
        bitmaps[source]?.let { return it }
        if (bitmaps.containsKey(source)) return null
        Thread {
            val held = try {
                java.io.File(source).inputStream().use { android.graphics.BitmapFactory.decodeStream(it) }
            } catch (error: Exception) {
                null
            }
            bitmaps[source] = held
        }.start()
        return null
    }
}
