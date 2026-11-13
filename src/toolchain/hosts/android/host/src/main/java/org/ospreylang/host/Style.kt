package org.ospreylang.host

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.view.View
import android.widget.TextView

// Implements [UI-THEME] and [UI-COMPONENTS-FRAMED] for the told renderer: the
// theme's tokens against the current appearance, and the typography and
// control metrics a frame passes down its subtree. Every value comes from the
// application; an unknown token draws nothing rather than a colour of this
// host's choosing. The host holds no style of its own.

/** The theme's tokens resolved for one appearance. */
class ThemeResolver(val theme: Theme?, private val dark: Boolean) {
    /** `#RRGGBB` as a colour; anything else is unresolved. */
    fun colour(name: String): Int? {
        val token = theme?.tokens?.firstOrNull { it.name == name } ?: return null
        val hex = (if (dark) token.dark else token.light).removePrefix("#")
        if (hex.length != 6) return null
        return hex.toLongOrNull(16)?.let { (0xFF000000 or it).toInt() } ?: return null
    }

    /** The theme's spacing scale, 8dp at a step it does not name. */
    fun spacing(step: Int): Int = theme?.spacing?.getOrNull(step) ?: 8

    /** Text resolves the `text` token; anything unresolved is plain. */
    fun text(): Int = colour("text") ?: 0xFF000000.toInt()
    fun accent(): Int = colour("accent") ?: 0xFF6750A4.toInt()

    fun tone(name: String): Int = when (name) {
        "primary" -> text()
        "secondary" -> colour("secondary") ?: Color.DKGRAY
        "accent" -> accent()
        "good" -> colour("good") ?: 0xFF2E7D32.toInt()
        "caution" -> colour("caution") ?: 0xFFB08800.toInt()
        "danger" -> colour("danger") ?: 0xFFB00020.toInt()
        else -> colour(name) ?: Color.DKGRAY
    }
}

/** What a frame's subtree inherits ([UI-COMPONENTS-FRAMED]). */
class Inherited(
    val gap: Int? = null, val family: String = "", val size: Int = 0, val lineHeight: Double = 0.0,
    val italic: Boolean = false, val hug: Boolean = false, val controlHeight: Int = 0,
    val controlPadding: Int = -1, val symbolSize: Int = 0, val quiet: Boolean = false,
    val iconOnly: Boolean = false, val trailingSymbols: Boolean = false,
    val flatRows: Boolean = false, val rowsFromTop: Boolean = false) {

    fun merging(tree: Tree?): Inherited {
        val style = tree as? Tree.Obj ?: return this
        val members = style.members
        val face = members.containsKey("font") || (members["textSize"]?.int ?: 0) > 0
        return Inherited(
            gap = style.count("gap").takeIf { it > 0 } ?: gap,
            family = style.text("font").ifEmpty { family },
            size = style.count("textSize").takeIf { it > 0 } ?: size,
            lineHeight = style.real("lineHeight").takeIf { it > 0 } ?: lineHeight,
            italic = if (face) style.flag("italic") else italic,
            hug = hug || style.flag("hug"),
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

/** A role's platform type: size in sp, weight and family. */
object Type {
    fun sp(role: String, inherited: Inherited): Float = when {
        inherited.size > 0 -> inherited.size.toFloat()
        else -> when (role) {
            "display" -> 34f; "large" -> 28f; "title" -> 20f; "heading" -> 16f; "lead" -> 15f
            "headline" -> 13f; "callout" -> 12f; "caption" -> 11f; "eyebrow" -> 11f; "mono" -> 12f
            else -> 13f
        }
    }

    fun face(inherited: Inherited, bold: Boolean): Typeface {
        val weight = if (bold) Typeface.BOLD else Typeface.NORMAL
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

/** One control's shared dress: the platform metrics every control wears. */
fun controlPadding(context: Context, inherited: Inherited): Int {
    if (inherited.controlPadding >= 0) return dp(context, inherited.controlPadding.toDouble())
    return dp(context, 12.0)
}

fun controlHeight(context: Context, inherited: Inherited): Int {
    if (inherited.controlHeight > 0) return dp(context, inherited.controlHeight.toDouble())
    return dp(context, 44.0)
}

fun dp(context: Context, points: Double): Int =
    (points * context.resources.displayMetrics.density).toInt()
