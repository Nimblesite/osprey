package org.ospreylang.host

import android.content.Context
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.view.View
import android.widget.TextView

// Implements [UI-THEME] and [UI-COMPONENTS-FRAMED] for the told renderer: the
// theme's tokens against the current appearance, resolved exactly as the
// SwiftUI host's Style.swift resolves them, and the typography and control
// metrics a frame passes down its subtree. Every value comes from the
// application; an unknown token draws nothing rather than a colour of this
// host's choosing. The host holds no style of its own.

/** The theme's tokens resolved for one appearance. */
class ThemeResolver(val theme: Theme?, private val dark: Boolean) {
    /** `#RRGGBB` as a colour; anything else is unresolved. */
    fun colour(name: String): Int? {
        val token = theme?.tokens?.firstOrNull { it.name == name } ?: return null
        val hex = (if (dark) token.dark else token.light).removePrefix("#")
        if (hex.length != 6) return null
        return hex.toLongOrNull(16)?.let { (0xFF000000 or it).toInt() }
    }

    /** The page's canvas: the `background` token, else the system's own background. */
    fun canvas(context: Context): Int {
        val typed = context.obtainStyledAttributes(intArrayOf(android.R.attr.colorBackground))
        val fallback = typed.getColor(0, if (dark) 0xFF1C1C1E.toInt() else 0xFFFFFFFF.toInt())
        typed.recycle()
        return colour("background") ?: fallback
    }

    /** Text resolves the `text` token; unresolved text is the system ink. */
    fun text(): Int = colour("text") ?: if (dark) 0xFFEBEBF5.toInt() else 0xFF000000.toInt()

    /** The accent resolves its token; unresolved is the platform's own accent. */
    fun accent(): Int = colour("accent") ?: if (dark) 0xFF0A84FF.toInt() else 0xFF007AFF.toInt()

    /** The system secondary label colour, as SwiftUI's `.secondary`. */
    fun secondary(): Int =
        if (dark) 0x99EBEBF5.toInt() else 0x993C3C43.toInt()

    /** A tone as Style.swift resolves it: primary is the text ink, anything
     * else its own token, and an unresolved tone the secondary ink. */
    fun tone(name: String): Int =
        if (name == "primary") text() else colour(name) ?: secondary()

    /** White or black laid over the canvas at `share`, as Color.primary.opacity does. */
    fun overlay(share: Double): Int {
        val alpha = (share * 255).toInt().coerceIn(0, 255)
        return (alpha shl 24) or (if (dark) 0xFFFFFF else 0x000000)
    }

    /** The theme's spacing scale, 8dp at a step it does not name. */
    fun spacing(step: Int): Int = theme?.spacing?.getOrNull(step) ?: 8
}

/** What a frame's subtree inherits ([UI-COMPONENTS-FRAMED]). */
class Inherited(
    val gap: Int? = null, val family: String = "", val size: Int = 0, val lineHeight: Double = 0.0,
    val tracking: Double = 0.0, val italic: Boolean = false, val hug: Boolean = false,
    val inkName: String = "", val controlHeight: Int = 0, val controlPadding: Int = -1,
    val symbolSize: Int = 0, val quiet: Boolean = false, val iconOnly: Boolean = false,
    val trailingSymbols: Boolean = false, val flatRows: Boolean = false,
    val rowsFromTop: Boolean = false) {

    fun merging(tree: Tree?): Inherited {
        val style = tree as? Tree.Obj ?: return this
        val members = style.members
        val face = members.containsKey("font") || (members["textSize"]?.int ?: 0) > 0
        return Inherited(
            gap = style.count("gap").takeIf { it > 0 } ?: gap,
            family = style.text("font").ifEmpty { family },
            size = style.count("textSize").takeIf { it > 0 } ?: size,
            lineHeight = style.real("lineHeight").takeIf { it > 0 } ?: lineHeight,
            tracking = (members["tracking"] as? Tree.Num)?.value ?: tracking,
            italic = if (face) style.flag("italic") else italic,
            hug = hug || style.flag("hug"),
            inkName = style.text("ink").ifEmpty { inkName },
            controlHeight = style.count("controlHeight").takeIf { it > 0 } ?: controlHeight,
            controlPadding = style.count("controlPadding").takeIf { it >= 0 } ?: controlPadding,
            symbolSize = style.count("symbolSize").takeIf { it > 0 } ?: symbolSize,
            quiet = quiet || style.flag("quietControls"),
            iconOnly = iconOnly || style.flag("iconOnly"),
            trailingSymbols = trailingSymbols || style.flag("trailingSymbols"),
            flatRows = flatRows || style.flag("flatRows"),
            rowsFromTop = style.text("cross") == "start")
    }
}

/** A role's platform type, as Style.swift's Type table reads it. */
object Type {
    fun sp(role: String, inherited: Inherited): Float = when {
        inherited.size > 0 -> inherited.size.toFloat()
        else -> when (role) {
            "display" -> 34f; "large" -> 28f; "title" -> 20f; "heading" -> 16f; "lead" -> 15f
            "headline" -> 13f; "callout" -> 12f; "caption" -> 11f; "eyebrow" -> 11f; "mono" -> 12f
            else -> 13f
        }
    }

    /** The roles the platform draws semibold. */
    private val bold = setOf("display", "large", "title", "heading", "headline", "eyebrow")

    fun face(inherited: Inherited, role: String): Typeface {
        val weight = if (role in bold) Typeface.BOLD else Typeface.NORMAL
        val slant = if (inherited.italic) Typeface.ITALIC else Typeface.NORMAL
        return when (inherited.family) {
            "mono" -> Typeface.create(Typeface.MONOSPACE, weight or slant)
            "serif" -> Typeface.create(Typeface.SERIF, weight or slant)
            else -> Typeface.create(Typeface.DEFAULT, weight or slant)
        }
    }
}

/** [NATIVE-HOSTS-CONTRACT] A kind this host does not draw says so where it would have been. */
fun refusal(context: Context, kind: String): View =
    TextView(context).apply {
        text = "This host does not draw $kind nodes"
        textSize = 12f
        setTextColor(0xFFB00020.toInt())
        setPadding(24, 12, 24, 12)
        background = GradientDrawable().apply {
            setStroke(2, 0xFFB00020.toInt())
            cornerRadius = 8f
        }
    }

/** A surface the framed renderer paints: fill, corner, line. */
fun surfaceOf(fill: Int?, radius: Float, line: Int?): GradientDrawable =
    GradientDrawable().apply {
        cornerRadius = radius
        if (fill != null) setColor(fill)
        if (line != null) setStroke(2, line)
    }

/** One control's shared dress, the metrics every control wears. */
fun controlPadding(context: Context, inherited: Inherited): Int {
    if (inherited.controlPadding >= 0) return dp(context, inherited.controlPadding.toDouble())
    return dp(context, 12.0)
}

fun controlHeight(context: Context, inherited: Inherited): Int {
    if (inherited.controlHeight > 0) return dp(context, inherited.controlHeight.toDouble())
    return dp(context, 28.0)
}

/** The touch target a phone control keeps, as the SwiftUI host's iOS build does. */
fun touchHeight(context: Context, inherited: Inherited): Int =
    maxOf(controlHeight(context, inherited), dp(context, 44.0))

fun dp(context: Context, points: Double): Int =
    (points * context.resources.displayMetrics.density).toInt()

/** The same colour at `share` opacity, as SwiftUI's `Color.opacity` composes it. */
fun Int.softly(share: Double): Int {
    val alpha = ((this ushr 24) * share).toInt().coerceIn(0, 255)
    return (this and 0x00FFFFFF) or (alpha shl 24)
}
