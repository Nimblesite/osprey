package org.ospreylang.host

import android.content.Context
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

// Implements [UI-COMPONENTS] and [UI-EVENTS] for the Android host: controls.
// Only the text being composed lives here; every committed value goes to
// Osprey as a control event that names the ordinal the frame issued, and
// comes back as the node the next frame tells.

object RenderControls {
    /** One new view of a control kind, or null when it is no control. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "button", "toggle", "alert" -> TextView(context)
        "field", "editor" -> EditText(context)
        "picker" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "disclosure" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        else -> null
    }

    /** Writes `tree` into its control view; false when it is no control. */
    fun update(renderer: ToldRenderer, view: View, tree: Tree, path: String,
               theme: ThemeResolver, inherited: Inherited): Boolean = when (tree.text("kind")) {
        "button" -> button(renderer, view as TextView, tree, theme, inherited)
        "field", "editor" -> field(renderer, view as EditText, tree, theme, inherited)
        "toggle" -> toggle(renderer, view as TextView, tree, theme, inherited)
        "picker" -> picker(renderer, view as LinearLayout, tree, theme, inherited)
        "disclosure" -> disclosure(renderer, view as LinearLayout, tree, path, theme, inherited)
        "alert" -> alert(renderer, view as TextView, tree, theme, inherited)
        else -> false
    }

    /** A button: its label and symbol on the surface its role spells. */
    private fun button(renderer: ToldRenderer, view: TextView, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        view.text = face(tree, inherited)
        view.textSize = Type.sp("body", inherited)
        view.setTypeface(Type.face(inherited, false))
        view.gravity = Gravity.CENTER
        view.minHeight = controlHeight(view.context, inherited)
        view.setPadding(controlPadding(view.context, inherited), 0, controlPadding(view.context, inherited), 0)
        view.isEnabled = tree.flag("enabled")
        view.alpha = if (tree.flag("enabled")) 1f else 0.45f
        view.background = buttonSurface(tree, theme)
        view.setOnClickListener { renderer.send(tree.count("event"), Tree.Null) }
        return true
    }

    private fun face(tree: Tree, inherited: Inherited): String {
        val symbol = Symbols.glyph(tree.text("symbol"))
        return when {
            symbol != null && tree.text("label").isEmpty() -> symbol
            symbol != null -> "$symbol ${tree.text("label")}"
            else -> tree.text("label")
        }
    }

    /** A button's surface by role, on the theme's own tokens. */
    private fun buttonSurface(tree: Tree, theme: ThemeResolver) = when (tree.text("role")) {
        "filled", "prominent" -> surfaceOf(theme.accent(), 12f, null)
        "destructive" -> surfaceOf(theme.tone("danger"), 12f, null)
        else -> surfaceOf(0x12000000, 12f, null)
    }

    /** A text field that commits every edit; a secure field hides its words. */
    private fun field(renderer: ToldRenderer, view: EditText, tree: Tree,
                      theme: ThemeResolver, inherited: Inherited): Boolean {
        view.hint = tree.text("prompt")
        view.textSize = Type.sp("body", inherited)
        view.setTextColor(theme.text())
        if (tree.text("kind") == "editor") {
            view.minHeight = dp(view.context, 96.0)
            view.gravity = Gravity.TOP or Gravity.START
        }
        if (view.tag !== tree.count("event")) prepared(renderer, view, tree)
        if (!view.isFocused && view.text.toString() != tree.text("value")) view.setText(tree.text("value"))
        return true
    }

    /** The binding's once-only preparation: its keyboard and its watcher. */
    private fun prepared(renderer: ToldRenderer, view: EditText, tree: Tree) {
        view.tag = tree.count("event")
        view.inputType = if (tree.flag("secure"))
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
        else if (tree.text("kind") == "editor" || tree.flag("multiline"))
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
        else InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        view.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(text: CharSequence, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(text: CharSequence, a: Int, b: Int, c: Int) = Unit
            override fun afterTextChanged(text: Editable) {
                if (view.isFocused && text.toString() != tree.text("value"))
                    renderer.send(tree.count("event"), Tree.Str(text.toString()))
            }
        })
    }

    /** A switch: its label beside the platform's own switch words. */
    private fun toggle(renderer: ToldRenderer, view: TextView, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        view.text = "${tree.text("label")} — ${if (tree.flag("on")) "on" else "off"}"
        view.textSize = Type.sp("body", inherited)
        view.setTextColor(theme.text())
        view.setTypeface(Type.face(inherited, tree.flag("on")))
        if (view.tag !== tree.count("event")) {
            view.tag = tree.count("event")
            view.setOnClickListener { renderer.send(tree.count("event"), Tree.Bit(!tree.flag("on"))) }
        }
        return true
    }

    /** A choice among keys: one press per choice, the chosen one outlined. */
    private fun picker(renderer: ToldRenderer, box: LinearLayout, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(TextView(box.context))
            box.addView(android.widget.HorizontalScrollView(box.context).apply {
                addView(LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL })
            })
        }
        val label = box.getChildAt(0) as TextView
        label.text = tree.text("label")
        label.textSize = Type.sp("body", inherited)
        label.setTextColor(theme.text())
        chips(renderer, (box.getChildAt(1) as android.widget.HorizontalScrollView).getChildAt(0) as LinearLayout,
              tree, theme, inherited)
        return true
    }

    /** One press per choice, the chosen one outlined in the theme's accent. */
    private fun chips(renderer: ToldRenderer, strip: LinearLayout, tree: Tree,
                      theme: ThemeResolver, inherited: Inherited) {
        val choices = tree.list("choices")
        while (strip.childCount > choices.size) strip.removeViewAt(strip.childCount - 1)
        choices.forEachIndexed { index, choice ->
            val chip = (strip.getChildAt(index) as? TextView) ?: TextView(strip.context).also {
                strip.addView(it)
                renderer.spaced(it, dp(strip.context, 6.0), across = true)
            }
            chip(renderer, choice, chip, tree, theme, inherited)
        }
    }

    private fun chip(renderer: ToldRenderer, choice: Tree, chip: TextView, tree: Tree,
                      theme: ThemeResolver, inherited: Inherited) {
        chip.text = choice.text("label")
        chip.textSize = Type.sp("body", inherited)
        chip.setPadding(dp(chip.context, 12.0), dp(chip.context, 8.0), dp(chip.context, 12.0), dp(chip.context, 8.0))
        val chosen = choice.text("key") == tree.text("selected")
        chip.setTextColor(if (chosen) theme.accent() else theme.text())
        chip.background = surfaceOf(if (chosen) 0x14000000 else 0x08000000, 12f, if (chosen) theme.accent() else null)
        chip.setOnClickListener { renderer.send(tree.count("event"), Tree.Str(choice.text("key"))) }
    }

    /** Content revealed while `expanded` holds; toggling delivers a flag. */
    private fun disclosure(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                           theme: ThemeResolver, inherited: Inherited): Boolean {
        val head = box.getChildAt(0) as? TextView ?: TextView(box.context).also { box.addView(it, 0) }
        head.text = "${if (tree.flag("expanded")) "▾" else "▸"} ${tree.text("title")}"
        head.textSize = Type.sp("body", inherited)
        head.setTextColor(theme.text())
        head.setOnClickListener { renderer.send(tree.count("event"), Tree.Bit(!tree.flag("expanded"))) }
        renderer.children(box, tree, path, theme, inherited, first = 1, gap = theme.spacing(1))
        return true
    }

    /** An alert: its title and message, with its dismiss press. */
    private fun alert(renderer: ToldRenderer, view: TextView, tree: Tree,
                      theme: ThemeResolver, inherited: Inherited): Boolean {
        if (!tree.flag("visible")) {
            view.visibility = View.GONE
            return true
        }
        view.visibility = View.VISIBLE
        view.text = "${tree.text("title")}\n${tree.text("message")}"
        view.textSize = Type.sp("body", inherited)
        view.setTextColor(theme.text())
        view.setPadding(dp(view.context, 16.0), dp(view.context, 12.0), dp(view.context, 16.0), dp(view.context, 12.0))
        view.background = surfaceOf(theme.colour("surface") ?: 0x11808080, 12f, theme.colour("danger"))
        view.setOnClickListener { renderer.send(tree.count("event"), Tree.Null) }
        return true
    }
}
