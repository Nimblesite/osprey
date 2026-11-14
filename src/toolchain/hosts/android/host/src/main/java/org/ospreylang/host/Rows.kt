package org.ospreylang.host

import java.nio.ByteBuffer

// Implements [NATIVE-SHM-VIEW] and [NATIVE-SHM-ASK] checking, exactly as the
// SwiftUI host's SharedRows.swift does: nothing from the application is
// trusted. Every kind must be known, every parent an earlier row, every text
// range inside the text run on UTF-8 boundaries, every float finite, every
// count inside the rows that follow it, and every ask code one this host
// knows. A frame that fails any check is refused whole.

/** The limits native_bridge.h spells. */
object Limits {
    const val SHAPES: Long = 1L shl 20
    const val VIEWS: Long = 1L shl 16
    const val TEXT: Long = 1L shl 24
    const val ASKS: Long = 1L shl 16
    const val VIEW_STRIDE = 152
    const val SHAPE_STRIDE = 88
    const val ASK_STRIDE = 32
}

/** What a row draws, by its kind ([NATIVE-SHM-VIEW]). */
enum class Kind(val row: Long, val told: Boolean = false) {
    COLUMN(0), LINE(1), LABEL(2), PRESS(3), ENTRY(4), GAP(5), SCROLL(6), LAYER(7),
    TOGGLE(8), PICKER(9), OPTION(10), RULE(11), IMAGE(12), CANVAS(13), EDITOR(14),
    MENU(15), SHEET(16), FIELD(33, true), RECORD(34, true), LIST(35, true);

    companion object {
        private val byRow = entries.associateBy { it.row }
        fun of(row: Long): Kind? = byRow[row]
    }
}

/** The `flags` bits a row's fit adds up ([NATIVE-SHM-VIEW]). */
object Flags {
    const val DISABLED: Long = 1
    const val CHOSEN: Long = 2
    const val SECRET: Long = 4
    const val FIXED_WIDTH: Long = 8
    const val SLANTED: Long = 16
    const val CENTRED: Long = 32
    const val ON_COMMIT: Long = 64
    const val ALL: Long = 127
}

/** How one control looks: `0xRRGGBBAA` ink and fill, sizes in points. */
data class Look(val ink: Long, val fill: Long, val size: Double, val padding: Double,
                val spacing: Double, val radius: Double, val weight: Long)

/** How one control sits: fixed extents (0 = natural), flex share, flags, border. */
data class Fit(val width: Double, val height: Double, val grow: Double,
               val flags: Long, val border: Long)

/** One checked control and the rows drawn inside it. */
data class RowControl(val kind: Kind, val parent: Long, val action: Long?, val words: String,
                      val hint: String, val look: Look, val fit: Fit, val value: Long) {
    val children = mutableListOf<Int>()
    val told get() = kind.told
    fun has(flag: Long) = fit.flags and flag != 0L
}

/** One field of an ask: its argument position, number and text. */
data class AskField(val field: Long, val number: Long, val text: String)

/** A capability request or a slice write: code, ticket and fields by position. */
class Ask(val code: Long, val ticket: String, val fields: List<AskField>) {
    val kind: String get() = if (code in 1..CAPABILITIES.size.toLong()) CAPABILITIES[(code - 1).toInt()] else ""
    fun texts(field: Long) = fields.filter { it.field == field }.map { it.text }
    fun text(field: Long) = texts(field).firstOrNull() ?: ""
    fun number(field: Long) = fields.firstOrNull { it.field == field }?.number ?: 0L
    fun flag(field: Long) = number(field) != 0L
    fun pairs(field: Long): List<Pair<String, String>> {
        val items = texts(field)
        return items.indices.step(2).map { at -> items[at] to (if (at + 1 < items.size) items[at + 1] else "") }
    }

    companion object {
        /** The capability names, by code from 1, as Requests::kind spells them. */
        val CAPABILITIES = listOf(
            "run", "list-directories", "read-plists", "read-texts", "fingerprint", "store-read",
            "store-write", "store-list", "store-delete", "http", "host-call", "running-applications",
            "choose-file", "choose-folder", "save-file", "measure-folders", "reveal", "open-settings",
            "cancel", "recognise", "animate", "process-start", "process-write", "process-read", "process-stop")
        const val SLICE_WRITE: Long = 100
        const val DIAGNOSTIC: Long = 101
    }
}

/** A checked frame: its controls (a row's index is its position), the rows
 * with no parent, each canvas's scene by its row, and its asks. */
class Shown(val controls: List<RowControl>, val roots: List<Int>,
            val scenes: Map<Int, Scene>, val asks: List<Ask>) {
    val requests get() = asks.filter { it.code in 1..25 }
    val slices get() = asks.filter { it.code == Ask.SLICE_WRITE }.map { it.ticket to it.text(1) }
    val diagnostics get() = asks.filter { it.code == Ask.DIAGNOSTIC }.map { it.ticket }

    companion object {
        val EMPTY = Shown(emptyList(), emptyList(), emptyMap(), emptyList())
    }
}

/** Why a frame was refused: one fact that did not hold. */
class RowRefusal(val why: String) : Exception(why)

/** Checks and reads one sealed frame's runs, little-endian as [NATIVE-SHM-LAYOUT] spells. */
object Rows {
    fun read(shapes: ByteBuffer, views: ByteBuffer, text: ByteBuffer, asks: ByteBuffer): Shown {
        val run = TextRun(text)
        val controls = controls(rows(views, Limits.VIEWS, Limits.VIEW_STRIDE), run)
        val canvases = controls.first.mapIndexed { index, control ->
            if (control.kind == Kind.CANVAS || control.kind == Kind.RECORD) index else null
        }.filterNotNull()
        return Shown(controls.first, controls.second, Scenes.read(shapes, run, canvases),
            this.asks(rows(asks, Limits.ASKS, Limits.ASK_STRIDE), run))
    }

    /** One run as whole rows, or the frame refuses. */
    private fun rows(buffer: ByteBuffer, cap: Long, stride: Int): ByteBuffer {
        if (buffer.remaining() % stride != 0)
            throw RowRefusal("a run that is not a whole number of rows")
        val count = (buffer.remaining() / stride).toLong()
        if (count > cap)
            throw RowRefusal("a run that passes its row limit")
        buffer.order(java.nio.ByteOrder.LITTLE_ENDIAN)
        buffer.limit((minOf(count, cap) * stride).toInt() + buffer.position())
        return buffer
    }

    /** The controls, each child listed under its parent, and the rows with no parent. */
    private fun controls(views: ByteBuffer, run: TextRun): Pair<List<RowControl>, List<Int>> {
        val all = mutableListOf<RowControl>()
        val roots = mutableListOf<Int>()
        val stride = Limits.VIEW_STRIDE
        while (views.remaining() >= stride) {
            val index = all.size
            val row = ViewRow(views)
            if (row.parent == -1L) {
                roots.add(index)
            } else {
                if (row.parent < 0 || row.parent >= index)
                    throw RowRefusal("row $index names a parent that is not an earlier row")
                all[row.parent.toInt()].children.add(index)
            }
            all.add(control(row, index, run))
        }
        return all to roots
    }

    /** One control, checked. Unknown kinds, unknown flags, a weight outside
     * 100 to 900, a negative extent or a non-finite number refuse the frame. */
    private fun control(row: ViewRow, index: Int, run: TextRun): RowControl {
        val kind = Kind.of(row.kind) ?: throw RowRefusal("row $index has an unknown kind ${row.kind}")
        if (kind.told) return told(kind, row, run, index)
        if (row.flags < 0 || row.flags and Flags.ALL.inv() != 0L)
            throw RowRefusal("row $index has unknown flags ${row.flags}")
        if (row.weight !in 100..900)
            throw RowRefusal("row $index has a font weight outside 100 to 900")
        return RowControl(kind, row.parent, row.action.takeIf { it >= 0 },
            run.words(row.textOffset, row.textLength), run.words(row.hintOffset, row.hintLength),
            Look(colour(row.ink), colour(row.fill), extent(row.size), extent(row.padding),
                extent(row.spacing), extent(row.radius), row.weight),
            Fit(extent(row.width), extent(row.height), extent(row.grow), row.flags, colour(row.border)),
            0)
    }

    /** One told row ([NATIVE-SHM-NODES]): its name is its hint, a field's value
     * is its words, its whole number or its real, and its weight says which. */
    private fun told(kind: Kind, row: ViewRow, run: TextRun, index: Int): RowControl {
        if (row.weight !in 0..4)
            throw RowRefusal("a told value of an unknown type ${row.weight}")
        return RowControl(kind, row.parent, null,
            run.words(row.textOffset, row.textLength), run.words(row.hintOffset, row.hintLength),
            Look(0, 0, extent(row.size), 0.0, 0.0, 0.0, row.weight),
            Fit(0.0, 0.0, 0.0, 0, 0), row.ink)
    }

    /** The asks: a field-0 row starts one, and the rows after it are its fields. */
    private fun asks(asks: ByteBuffer, run: TextRun): List<Ask> {
        val all = mutableListOf<MutableAsk>()
        while (asks.remaining() >= Limits.ASK_STRIDE) {
            val field = asks.long
            val number = asks.long
            val text = run.words(asks.long, asks.long)
            if (field == 0L) {
                all.add(MutableAsk(number, text))
            } else {
                if (field < 0 || all.isEmpty())
                    throw RowRefusal("an ask field before any ask, or below zero")
                all.last().fields.add(AskField(field, number, text))
            }
        }
        return all.map { ask ->
            if (ask.code !in 1..25 && ask.code != Ask.SLICE_WRITE && ask.code != Ask.DIAGNOSTIC)
                throw RowRefusal("an ask with an unknown capability code")
            Ask(ask.code, ask.ticket, ask.fields.toList())
        }
    }

    /** A size the host can lay out: finite and never negative. */
    fun extent(value: Double): Double {
        if (!value.isFinite() || value < 0) throw RowRefusal("an extent that is negative or not finite")
        return value
    }

    /** A packed `0xRRGGBBAA` colour. */
    fun colour(value: Long): Long {
        if (value < 0 || value > 0xFFFFFFFFL) throw RowRefusal("a colour outside 0xRRGGBBAA")
        return value
    }
}

/** One ask while its rows are still arriving. */
private class MutableAsk(val code: Long, val ticket: String) {
    val fields = mutableListOf<AskField>()
}

/** The frame's one text run, checked on every borrow. */
class TextRun(private val bytes: ByteBuffer) {
    init {
        bytes.order(java.nio.ByteOrder.LITTLE_ENDIAN)
        val size = bytes.remaining().toLong()
        if (size > Limits.TEXT) bytes.limit(bytes.position() + Limits.TEXT.toInt())
    }

    /** Text bytes `offset..offset + length` of the run, checked as UTF-8. */
    fun words(offset: Long, length: Long): String {
        if (offset < 0 || length < 0) throw RowRefusal("a text range below zero")
        val size = bytes.remaining().toLong()
        if (offset > size || length > size - offset) throw RowRefusal("a text range outside the frame")
        val copy = ByteArray(length.toInt())
        val at = bytes.position() + offset.toInt()
        for (index in copy.indices) copy[index] = bytes.get(at + index)
        return try {
            Charsets.UTF_8.newDecoder()
                .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
                .onUnmappableCharacter(java.nio.charset.CodingErrorAction.REPORT)
                .decode(java.nio.ByteBuffer.wrap(copy)).toString()
        } catch (error: Exception) {
            throw RowRefusal("text that is not UTF-8")
        }
    }
}

/** One ViewRow, read at its offsets ([NATIVE-SHM-LAYOUT]). */
class ViewRow(buffer: ByteBuffer) {
    val kind = buffer.long
    val parent = buffer.long
    val action = buffer.long
    val textOffset = buffer.long
    val textLength = buffer.long
    val ink = buffer.long
    val fill = buffer.long
    val size = buffer.double
    val padding = buffer.double
    val spacing = buffer.double
    val radius = buffer.double
    val weight = buffer.long
    val width = buffer.double
    val height = buffer.double
    val grow = buffer.double
    val flags = buffer.long
    val border = buffer.long
    val hintOffset = buffer.long
    val hintLength = buffer.long
}
