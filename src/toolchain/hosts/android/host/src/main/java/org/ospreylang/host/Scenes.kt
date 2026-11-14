package org.ospreylang.host


import java.nio.ByteBuffer

// Implements [NATIVE-SHM-SCENE] reading, as the SwiftUI host's
// SharedSceneRows.swift does: scene rows in order, the next one taken on
// demand, every count checked against what is left, every code against what
// the host knows and every float for finiteness. The codes are native.td's.

/** A canvas's scene: extent, zoom, atlas, shapes and accessibility elements. */
class Scene(val width: Double, val height: Double, val zoom: Double,
            val atlas: List<String>, val shapes: List<Shape>, val access: List<Element>)

// The scene row codes native.td declares.
private const val POINT = 7L
private const val PATH_STEP = 8L
private const val OUTLINED = 9L
private const val PAINTED = 10L
private const val TINTED = 11L
private const val SCENE = 20L
private const val ATLAS = 21L
private const val ELEMENT = 22L

/** One element of a scene's accessibility tree, bounds in scene units. */
data class Element(val role: Long, val label: String, val x: Double, val y: Double,
                   val w: Double, val h: Double, val value: Double,
                   val min: Double, val max: Double, val selected: Boolean)

/** A tint in the space the application gave it. */
sealed class Tint {
    class Rgb(val r: Double, val g: Double, val b: Double, val a: Double) : Tint()
    class Hsl(val h: Double, val s: Double, val l: Double, val a: Double) : Tint()
}

/** A solid tint or a two-stop linear gradient, angle in degrees. */
sealed class PaintStyle {
    class Solid(val tint: Tint) : PaintStyle()
    class Linear(val angle: Double, val from: Tint, val fromAt: Double,
                 val to: Tint, val toAt: Double) : PaintStyle()
}

/** Fill, or stroke with a width, bevel joins and an optional dash pattern. */
sealed class Outline {
    class Fill : Outline()
    class Stroke(val width: Double, val bevel: Boolean, val dashes: List<Double>) : Outline()
}

/** One path-builder command in scene units. */
sealed class Step {
    class Move(val x: Double, val y: Double) : Step()
    class Line(val x: Double, val y: Double) : Step()
    class Curve(val x: Double, val y: Double, val cx: Double, val cy: Double) : Step()
    class Cubic(val x: Double, val y: Double, val x1: Double, val y1: Double,
                val x2: Double, val y2: Double) : Step()
    class Arc(val rx: Double, val ry: Double, val rotation: Double, val large: Boolean,
              val sweep: Boolean, val x: Double, val y: Double) : Step()
    class Polygon(val points: List<Pair<Double, Double>>, val closed: Boolean) : Step()
    class Close : Step()
}

/** A primitive in scene units. */
sealed class Shape {
    class Rect(val x: Double, val y: Double, val w: Double, val h: Double,
               val angle: Double, val colour: Long) : Shape()
    class Disc(val x: Double, val y: Double, val radius: Double, val colour: Long) : Shape()
    class Strip(val points: List<Pair<Double, Double>>, val colour: Long, val stroke: Double) : Shape()
    class Sprite(val atlas: Int, val x: Double, val y: Double, val w: Double,
                 val h: Double, val angle: Double, val tint: Long) : Shape()
    class Glyphs(val text: String, val x: Double, val y: Double, val size: Double, val colour: Long) : Shape()
    class Lettering(val text: String, val x: Double, val y: Double, val size: Double,
                    val colour: Long, val anchor: Long, val weight: Double) : Shape()
    class Path(val steps: List<Step>, val outline: Outline, val paint: PaintStyle,
               val moved: Triple<Double, Double, Double>?) : Shape()
}

/** One scene while its rows are still arriving. */
private class Building(val width: Double, val height: Double, val zoom: Double) {
    val atlas = mutableListOf<String>()
    val shapes = mutableListOf<Shape>()
    val access = mutableListOf<Element>()
}

/** SharedScene rows in order, with the next one taken on demand. */
private class Walker(private val rows: ByteBuffer, private val run: TextRun) {
    var next = 0
    var count = 0
    var current: Int? = null

    val left get() = count - next

    /** The next row, whatever code it carries; its values must be finite. */
    fun next(): ShapeRow {
        if (next >= count) throw RowRefusal("a scene row that runs past its run")
        val row = ShapeRow(rows)
        next++
        row.finite()
        return row
    }

    /** The next row, which must have `code`; its values must be finite. */
    fun take(code: Long): ShapeRow {
        val row = next()
        if (row.code != code) throw RowRefusal("scene row ${next - 1} should have code $code")
        return row
    }

    /** A count that must stay inside the rows that follow. */
    fun inside(value: Long): Int {
        if (value < 0 || value > left) throw RowRefusal("a scene count that runs past its rows")
        return value.toInt()
    }

    fun points(many: Long): List<Pair<Double, Double>> =
        (0 until inside(many)).map { val row = take(POINT); row.values[0] to row.values[1] }

    fun text(row: ShapeRow) = run.words(row.textOffset, row.textLength)
}

/** One SceneShape, read at its offsets. */
class ShapeRow(buffer: ByteBuffer) {
    val code = buffer.long
    val index = buffer.long
    val values = DoubleArray(5) { buffer.double }
    val colour = buffer.long
    val depth = buffer.long
    val textOffset = buffer.long
    val textLength = buffer.long

    fun finite() {
        if (values.any { !it.isFinite() }) throw RowRefusal("a number that is not finite")
    }
}

/** Reads the scene run: each canvas or told scene record by its row. */
object Scenes {
    fun read(shapes: ByteBuffer, run: TextRun, canvases: List<Int>): Map<Int, Scene> {
        if (shapes.remaining() % Limits.SHAPE_STRIDE != 0)
            throw RowRefusal("a run that is not a whole number of rows")
        shapes.order(java.nio.ByteOrder.LITTLE_ENDIAN)
        val walker = Walker(shapes, run)
        walker.count = shapes.remaining() / Limits.SHAPE_STRIDE
        return scenes(walker, canvases)
    }

    /** Every scene, by the canvas row its marker names. A marker's `index`
     * must name a canvas row no scene has claimed; the rows after it belong
     * to that canvas. */
    private fun scenes(walker: Walker, canvases: List<Int>): Map<Int, Scene> {
        val all = mutableMapOf<Int, Building>()
        while (walker.next < walker.count) sceneRow(walker, walker.next(), canvases, all)
        return all.mapValues { scene(it.value.width, it.value.height, it.value.zoom, it.value) }
    }

    /** One scene row: a marker claims a canvas, or the row joins its scene. */
    private fun sceneRow(walker: Walker, head: ShapeRow, canvases: List<Int>, all: MutableMap<Int, Building>) {
        if (head.code == SCENE) {
            val row = head.index.toInt()
            if (head.index < 0 || row !in canvases || all.containsKey(row))
                throw RowRefusal("a scene that names no canvas, or one twice")
            all[row] = Building(Rows.extent(head.values[0]), Rows.extent(head.values[1]), Rows.extent(head.values[2]))
            walker.current = row
            return
        }
        val current = all[walker.current ?: throw RowRefusal("a scene row before any scene")]!!
        when (head.code) {
            ATLAS -> current.atlas.add(walker.text(head))
            ELEMENT -> current.access.addAll(element(walker, head))
            else -> current.shapes.add(shape(walker, head))
        }
    }

    /** One accessible element, with its range row; role 4 announces only. */
    private fun element(walker: Walker, head: ShapeRow): List<Element> {
        val range = walker.take(POINT)
        if (head.index !in 0..4) throw RowRefusal("an unknown accessible role")
        if (head.index == 4L) return emptyList()
        return listOf(Element(head.index, walker.text(head), head.values[0], head.values[1],
            head.values[2], head.values[3], head.values[4], range.values[0], range.values[1],
            head.colour and 1L != 0L))
    }

    /** One shape head row, with whatever rows follow it. */
    private fun shape(walker: Walker, head: ShapeRow): Shape = when (head.code) {
        0L -> Shape.Rect(head.values[0], head.values[1], head.values[2], head.values[3],
                         head.values[4], Rows.colour(head.colour))
        1L -> Shape.Disc(head.values[0], head.values[1], head.values[2], Rows.colour(head.colour))
        2L -> Shape.Strip(walker.points(head.index), Rows.colour(head.colour), head.values[2])
        3L -> {
            if (head.index < 0) throw RowRefusal("a sprite index below zero")
            Shape.Sprite(head.index.toInt(), head.values[0], head.values[1], head.values[2],
                         head.values[3], head.values[4], Rows.colour(head.colour))
        }
        4L -> Shape.Glyphs(walker.text(head), head.values[0], head.values[1],
                           head.values[2], Rows.colour(head.colour))
        5L -> path(walker, head)
        6L -> {
            if (head.index !in 0..2) throw RowRefusal("an unknown lettering anchor")
            Shape.Lettering(walker.text(head), head.values[0], head.values[1], head.values[2],
                            Rows.colour(head.colour), head.index, head.values[3])
        }
        else -> throw RowRefusal("an unknown scene code ${head.code}")
    }

    /** A path head row's steps, then its outline and paint. */
    private fun path(walker: Walker, head: ShapeRow): Shape.Path {
        val steps = (0 until walker.inside(head.index)).map { step(walker) }
        val moved = Triple(head.values[0], head.values[1], head.values[2])
        return Shape.Path(steps, outline(walker), paint(walker),
            moved.takeIf { it.first != 0.0 || it.second != 0.0 || it.third != 1.0 })
    }

    /** One path-step row, whose `index` says which step it is. */
    private fun step(walker: Walker): Step {
        val row = walker.take(PATH_STEP)
        val v = row.values
        return when (row.index) {
            0L -> Step.Move(v[0], v[1])
            1L -> Step.Line(v[0], v[1])
            2L -> Step.Curve(v[0], v[1], v[2], v[3])
            3L -> Step.Cubic(v[0], v[1], v[2], v[3], v[4], walker.take(POINT).values[0])
            4L -> Step.Arc(v[0], v[1], v[2], row.colour and 1L != 0L,
                           row.colour and 2L != 0L, v[3], v[4])
            5L -> Step.Polygon(walker.points(row.depth), row.colour and 1L != 0L)
            6L -> Step.Close()
            else -> throw RowRefusal("an unknown path step")
        }
    }

    /** One outline row, with its dash points. */
    private fun outline(walker: Walker): Outline {
        val row = walker.take(OUTLINED)
        return when (row.index) {
            0L -> Outline.Fill()
            1L -> Outline.Stroke(row.values[0], row.colour and 1L != 0L,
                                 walker.points(row.depth).map { it.first })
            else -> throw RowRefusal("an unknown outline")
        }
    }

    /** One paint row, solid or linear, with the tints that follow it. */
    private fun paint(walker: Walker): PaintStyle {
        val row = walker.take(PAINTED)
        return when (row.index) {
            0L -> PaintStyle.Solid(tint(walker))
            1L -> PaintStyle.Linear(row.values[0], tint(walker), row.values[1], tint(walker), row.values[2])
            else -> throw RowRefusal("an unknown paint")
        }
    }

    /** One tint row, whose `index` says RGB or HSL. */
    private fun tint(walker: Walker): Tint {
        val row = walker.take(TINTED)
        return when (row.index) {
            0L -> Tint.Rgb(row.values[0], row.values[1], row.values[2], row.values[3])
            1L -> Tint.Hsl(row.values[0], row.values[1], row.values[2], row.values[3])
            else -> throw RowRefusal("an unknown tint")
        }
    }

    private fun scene(width: Double, height: Double, zoom: Double, built: Building) =
        Scene(width, height, zoom, built.atlas.toList(), built.shapes.toList(), built.access.toList())
}
