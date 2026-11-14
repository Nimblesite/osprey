package org.ospreylang.host

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import android.widget.Space
import android.widget.Switch
import android.widget.TextView
import java.io.File
import java.net.URL

// Implements [NATIVE-SHM-VIEW]: draws one checked frame by walking the
// controls from the root and drawing each control's children where its kind
// puts them, as the SwiftUI host's SharedView.swift and SharedLeaves.swift
// do. Every colour, size and space comes from the row. A row keeps its view
// while its kind stays the same, so a field holds its text, caret and focus
// across the redraws its own edits cause.

/** The direction a container lays its children out in. */
enum class Axis { VERTICAL, HORIZONTAL, NONE }

class PlainRenderer(private val context: Context, private val store: Store) {
    private companion object {
        val HUGGING = setOf(Kind.LABEL, Kind.PRESS, Kind.TOGGLE, Kind.PICKER, Kind.IMAGE)
    }

    private val density = context.resources.displayMetrics.density
    private val holders = HashMap<String, View>()

    /** Reconciles `page` with the frame the store holds. A root row fills the
     * window, as the application's root fills its window on every host. */
    fun render(page: ViewGroup) {
        reconcile(page, store.shown.roots, Axis.VERTICAL, 0.0)
        for (seat in 0 until page.childCount)
            page.getChildAt(seat).layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f)
    }

    /** One row's view, created for its kind and updated in place. */
    private fun row(index: Int, axis: Axis): View {
        val control = store.control(index) ?: return Space(context)
        val view = holders.getOrPut("$index:${control.kind}") { make(control.kind) }
        update(view, control, index, axis)
        return dress(view, control, axis)
    }

    /** The parent shows `kids` in order from seat `first`, spaced `gap` points apart. */
    private fun reconcile(parent: ViewGroup, kids: List<Int>, axis: Axis, gap: Double, first: Int = 0) {
        while (parent.childCount - first > kids.size) parent.removeViewAt(parent.childCount - 1)
        kids.forEachIndexed { place, index ->
            val view = row(index, axis)
            val seat = first + place
            if (parent.getChildAt(seat) !== view) {
                parent.removeView(view)
                parent.addView(view, seat)
            }
            if (axis == Axis.VERTICAL) view.hug(index)
            if (place > 0 && gap > 0) view.marginGap(axis, dp(gap))
        }
    }

    /** In a column a label, press, switch, picker or picture keeps its own
     * width at the leading edge; everything else takes the column's width. */
    private fun View.hug(index: Int) {
        val control = store.control(index) ?: return
        val params = layoutParams as? LinearLayout.LayoutParams ?: return
        if (control.fit.width > 0) return
        val hugs = control.kind in HUGGING && !control.has(Flags.CENTRED)
        params.width = if (hugs) ViewGroup.LayoutParams.WRAP_CONTENT else ViewGroup.LayoutParams.MATCH_PARENT
        layoutParams = params
    }

    private fun View.marginGap(axis: Axis, gap: Int) {
        val params = layoutParams as? LinearLayout.LayoutParams ?: return
        if (axis == Axis.HORIZONTAL) params.leftMargin = gap else params.topMargin = gap
        layoutParams = params
    }

    /** One new view of the row kind. */
    private fun make(kind: Kind): View = when (kind) {
        Kind.COLUMN, Kind.SHEET -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        Kind.LINE -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        Kind.LABEL, Kind.PRESS -> TextView(context)
        Kind.ENTRY, Kind.EDITOR -> EditText(context)
        Kind.GAP -> Space(context)
        Kind.SCROLL -> ScrollView(context).apply { addView(LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }) }
        Kind.LAYER -> FrameLayout(context)
        Kind.CANVAS -> FrameLayout(context)
        Kind.IMAGE -> FrameLayout(context).apply { addView(ImageView(context)) }
        Kind.TOGGLE -> Switch(context)
        Kind.PICKER -> LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(context))
            addView(HorizontalScrollView(context).apply { addView(LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }) })
        }
        Kind.MENU -> FrameLayout(context)
        Kind.RULE -> View(context)
        else -> TextView(context)
    }

    /** Writes `control` into the view the row keeps. */
    private fun update(view: View, control: RowControl, index: Int, axis: Axis) {
        when (control.kind) {
            Kind.COLUMN -> column(view as LinearLayout, control)
            Kind.SHEET -> sheet(view as LinearLayout, control)
            Kind.LINE -> line(view as LinearLayout, control)
            Kind.LABEL -> label(view as TextView, control)
            Kind.PRESS -> press(view as TextView, control)
            Kind.ENTRY -> entry(view as EditText, control, index)
            Kind.GAP -> gap(view, axis)
            Kind.SCROLL -> scroll(view as ScrollView, control)
            Kind.LAYER -> reconcile(view as FrameLayout, control.children, Axis.NONE, 0.0)
            Kind.TOGGLE -> toggle(view as Switch, control)
            Kind.PICKER -> picker(view as LinearLayout, control, index)
            Kind.RULE -> rule(view, control)
            Kind.IMAGE -> image(view as FrameLayout, control)
            Kind.CANVAS -> canvas(view as FrameLayout, control, index)
            Kind.EDITOR, Kind.MENU -> longer(view, control, index)
            else -> Unit
        }
    }

    private fun longer(view: View, control: RowControl, index: Int) {
        when (control.kind) {
            Kind.EDITOR -> editor(view as EditText, control, index)
            Kind.MENU -> menu(view as FrameLayout, index)
            else -> Unit
        }
    }

    private fun column(box: LinearLayout, control: RowControl) =
        reconcile(box, control.children, Axis.VERTICAL, control.look.spacing)

    /** A sheet draws its children under its own dismiss press, so the view
     * underneath stays visible; dismissing it delivers its action. */
    private fun sheet(box: LinearLayout, control: RowControl) {
        val head = box.getChildAt(0) as? TextView
        if (head?.tag != "osprey-dismiss") {
            box.removeAllViews()
            box.addView(TextView(context).apply {
                tag = "osprey-dismiss"
                text = "✕"
                gravity = Gravity.END
            })
        }
        (box.getChildAt(0) as TextView).setOnClickListener { deliver(control, "") }
        reconcile(box, control.children, Axis.VERTICAL, control.look.spacing, first = 1)
    }

    private fun line(box: LinearLayout, control: RowControl) =
        reconcile(box, control.children, Axis.HORIZONTAL, control.look.spacing)

    private fun scroll(scroller: ScrollView, control: RowControl) =
        column(scroller.getChildAt(0) as LinearLayout, control)

    private fun gap(view: View, axis: Axis) {
        view.layoutParams = LinearLayout.LayoutParams(0, 0).apply {
            weight = 1f
            if (axis == Axis.HORIZONTAL) width = 0 else height = 0
        }
    }

    /** A label or press led by the glyph its hint names. */
    private fun led(view: TextView, control: RowControl) {
        face(view, control)
        view.text = control.words
        view.setCompoundDrawables(null, null, null, null)
        if (control.hint.isEmpty()) return
        val tint = colour(control.look.ink) ?: view.currentTextColor
        val glyph = Symbols.drawable(view.context, control.hint, tint, 14)
        if (glyph == null) {
            store.diagnoseOnce("no icon for symbol ${control.hint}")
            return
        }
        view.compoundDrawablePadding = dp(6.0)
        view.setCompoundDrawables(glyph, null, null, null)
    }

    private fun label(view: TextView, control: RowControl) {
        led(view, control)
        view.gravity = if (control.has(Flags.CENTRED)) Gravity.CENTER else Gravity.START
    }

    private fun press(view: TextView, control: RowControl) {
        led(view, control)
        view.isEnabled = !control.has(Flags.DISABLED)
        view.alpha = if (control.has(Flags.DISABLED)) 0.5f else 1f
        view.setOnClickListener { deliver(control, "") }
    }

    /** A single-line field: every edit delivers the whole text. */
    private fun entry(view: EditText, control: RowControl, index: Int) {
        field(view, control, index) { field ->
            field.hint = control.hint
            field.inputType = if (control.has(Flags.SECRET))
                InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            else InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        }
        view.setOnEditorActionListener { _, _, _ ->
            if (control.has(Flags.ON_COMMIT)) deliver(control, store.words(index))
            false
        }
    }

    private fun editor(view: EditText, control: RowControl, index: Int) {
        field(view, control, index) { field ->
            field.hint = control.hint
            field.minHeight = dp(60.0)
            field.gravity = Gravity.TOP or Gravity.START
        }
    }

    /** The words binding: the store's current text, delivered on each edit.
     * The once-only preparation and the watcher land with the binding. */
    private fun field(view: EditText, control: RowControl, index: Int, prepare: (EditText) -> Unit) {
        view.isEnabled = !control.has(Flags.DISABLED)
        face(view, control)
        if (view.tag !== control.action) {
            view.tag = control.action
            prepare(view)
            view.addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(text: CharSequence, a: Int, b: Int, c: Int) = Unit
                override fun onTextChanged(text: CharSequence, a: Int, b: Int, c: Int) = Unit
                override fun afterTextChanged(text: Editable) {
                    if (view.isFocused && text.toString() != store.words(index)) deliver(control, text.toString())
                }
            })
        }
        val words = store.words(index)
        if (!view.isFocused && view.text.toString() != words) view.setText(words)
    }

    private fun toggle(view: Switch, control: RowControl) {
        face(view, control)
        view.text = control.words
        view.isEnabled = !control.has(Flags.DISABLED)
        if (view.tag !== control.action) {
            view.tag = control.action
            view.setOnCheckedChangeListener { _, on -> deliver(control, if (on) "true" else "false") }
        }
        view.isChecked = control.has(Flags.CHOSEN)
    }

    /** A picker: its words, then every option child as a press that delivers
     * the option's key, the chosen one outlined in the picker's ink. */
    private fun picker(box: LinearLayout, control: RowControl, index: Int) {
        face(box.getChildAt(0) as TextView, control)
        (box.getChildAt(0) as TextView).text = control.words
        val strip = (box.getChildAt(1) as HorizontalScrollView).getChildAt(0) as LinearLayout
        val kids = store.kids(index)
        while (strip.childCount > kids.size) strip.removeViewAt(strip.childCount - 1)
        kids.forEachIndexed { place, child ->
            val option = store.control(child) ?: return@forEachIndexed
            val chip = (strip.getChildAt(place) as? TextView) ?: TextView(context).also { strip.addView(it) }
            face(chip, option)
            chip.text = option.words
            chip.setPadding(dp(8.0), dp(4.0), dp(8.0), dp(4.0))
            chip.background = chipFill(option, option.has(Flags.CHOSEN), control)
            chip.setOnClickListener { deliver(control, option.hint) }
        }
    }

    private fun chipFill(option: RowControl, chosen: Boolean, picker: RowControl): GradientDrawable {
        val shape = GradientDrawable().apply { cornerRadius = dp(option.look.radius).toFloat() }
        if (option.look.fill != 0L) shape.setColor(colour(option.look.fill)!!)
        if (chosen) shape.setStroke(dp(1.0), colour(picker.look.ink) ?: Color.BLACK)
        return shape
    }

    private fun rule(view: View, control: RowControl) {
        view.setBackgroundColor(colour(control.look.ink) ?: Color.TRANSPARENT)
        view.layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, maxOf(dp(control.look.size), 1))
    }

    private fun image(view: FrameLayout, control: RowControl) {
        val picture = view.getChildAt(0) as ImageView
        picture.scaleType = ImageView.ScaleType.FIT_START
        Images.load(picture.context, control.words, picture)
    }

    private fun canvas(view: FrameLayout, control: RowControl, index: Int) {
        if (view.childCount == 0) view.addView(SceneView(context, store, index, control.action ?: -1L))
        (view.getChildAt(0) as SceneView).refresh()
    }

    /** A menu shows its first child and opens the rest as a long-press menu. */
    private fun menu(view: FrameLayout, index: Int) {
        val kids = store.kids(index)
        if (kids.isEmpty()) return
        val head = row(kids[0], Axis.NONE)
        if (view.getChildAt(0) !== head) {
            view.removeAllViews()
            view.addView(head)
            head.setOnLongClickListener { subject ->
                val menu = PopupMenu(context, subject)
                kids.drop(1).forEach { child ->
                    store.control(child)?.let { item ->
                        menu.menu.add(item.words).setOnMenuItemClickListener { deliver(item, ""); true }
                    }
                }
                menu.show()
                true
            }
        }
    }

    private fun deliver(control: RowControl, words: String) {
        if (control.action != null) store.send(Touch.control(control.action, words))
    }

    /** A row's padding, fixed extent, fill, corner, border and flex share. */
    private fun dress(view: View, control: RowControl, axis: Axis): View {
        if (control.kind == Kind.GAP) return view
        if (control.look.padding > 0) view.setPadding(dp(control.look.padding), dp(control.look.padding), dp(control.look.padding), dp(control.look.padding))
        if (control.look.fill != 0L || control.fit.border != 0L) view.background = surface(control)
        if (control.fit.width > 0 || control.fit.height > 0) view.layoutParams = fixed(control)
        if (control.fit.grow > 0 && axis != Axis.NONE) grow(view, axis, control.fit.grow)
        return view
    }

    private fun fixed(control: RowControl) = LinearLayout.LayoutParams(
        if (control.fit.width > 0) dp(control.fit.width) else ViewGroup.LayoutParams.WRAP_CONTENT,
        if (control.fit.height > 0) dp(control.fit.height) else ViewGroup.LayoutParams.WRAP_CONTENT)

    private fun grow(view: View, axis: Axis, share: Double) {
        val params = view.layoutParams as? LinearLayout.LayoutParams
            ?: LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        params.weight = share.toFloat()
        if (axis == Axis.HORIZONTAL) params.width = 0 else params.height = 0
        view.layoutParams = params
    }

    private fun surface(control: RowControl): GradientDrawable {
        val shape = GradientDrawable().apply { cornerRadius = dp(control.look.radius).toFloat() }
        if (control.look.fill != 0L) shape.setColor(colour(control.look.fill)!!)
        if (control.fit.border != 0L) shape.setStroke(dp(1.0), colour(control.fit.border)!!)
        return shape
    }

    /** The face the row asks for, or the system's own when it names none. */
    private fun face(view: TextView, control: RowControl) {
        // A text view's size is in scaled points already, so the row's points go in unconverted.
        if (control.look.size > 0) view.textSize = control.look.size.toFloat()
        val ink = colour(control.look.ink)
        if (ink != null) view.setTextColor(ink)
        view.setTypeface(baseFace(control), if (control.has(Flags.SLANTED)) Typeface.ITALIC else Typeface.NORMAL)
    }

    private fun baseFace(control: RowControl): Typeface {
        val heft = control.look.weight.toInt().coerceIn(100, 1000)
        if (control.has(Flags.FIXED_WIDTH))
            return Typeface.create(Typeface.MONOSPACE, if (heft >= 600) Typeface.BOLD else Typeface.NORMAL)
        if (Build.VERSION.SDK_INT >= 28)
            return Typeface.create(Typeface.DEFAULT, heft, control.has(Flags.SLANTED))
        return if (heft >= 600) Typeface.DEFAULT_BOLD else Typeface.DEFAULT
    }

    fun dp(points: Double): Int = (points * density).toInt()

    /** A packed `0xRRGGBBAA` colour; zero is no colour at all. */
    fun colour(packed: Long): Int? {
        if (packed == 0L) return null
        return Color.argb((packed and 0xFF).toInt(), ((packed shr 24) and 0xFF).toInt(),
                          ((packed shr 16) and 0xFF).toInt(), ((packed shr 8) and 0xFF).toInt())
    }
}

/** Pictures by asset path or URL, decoded off the main thread and held by source. */
object Images {
    private val cache = HashMap<String, android.graphics.Bitmap?>()

    fun load(context: android.content.Context, source: String, into: ImageView) {
        if (source.isEmpty()) return
        cache[source]?.let { held -> into.setImageBitmap(held); return }
        Thread {
            val bitmap = try {
                val stream = when {
                    source.startsWith("https://") -> URL(source).openStream()
                    else -> try {
                        context.assets.open(source)
                    } catch (missing: Exception) {
                        File(source).inputStream()
                    }
                }
                stream.use { BitmapFactory.decodeStream(it) }
            } catch (error: Exception) {
                null
            }
            cache[source] = bitmap
            into.post { if (bitmap != null) into.setImageBitmap(bitmap) }
        }.start()
    }
}
