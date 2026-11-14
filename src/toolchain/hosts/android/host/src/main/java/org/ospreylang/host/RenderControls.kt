package org.ospreylang.host

import android.app.AlertDialog
import android.content.Context
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.Switch
import android.widget.TextView

// Implements [UI-COMPONENTS] and [UI-EVENTS] for the Android host: controls,
// as the SwiftUI host's Controls.swift draws them. Only the text being
// composed lives here; every committed value goes to Osprey as a control
// event that names the ordinal the frame issued, and comes back as the node
// the next frame tells.

object RenderControls {
    /** One new view of a control kind, or null when it is no control. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "button", "picker" -> TextView(context)
        "field", "editor" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "toggle" -> Switch(context)
        "disclosure" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        else -> null
    }

    /** Writes `tree` into its control view; false when it is no control. */
    fun update(renderer: ToldRenderer, view: View, tree: Tree, path: String,
               theme: ThemeResolver, inherited: Inherited): Boolean = when (tree.text("kind")) {
        "button" -> button(renderer, view as TextView, tree, theme, inherited)
        "field" -> field(renderer, view as LinearLayout, tree, theme, inherited)
        "editor" -> editor(renderer, view as LinearLayout, tree, theme, inherited)
        "toggle" -> toggle(renderer, view as Switch, tree, theme, inherited)
        "picker" -> picker(renderer, view as TextView, tree, theme, inherited)
        "disclosure" -> disclosure(renderer, view as LinearLayout, tree, path, theme, inherited)
        "alert" -> alert(renderer, view, tree, theme)
        else -> false
    }

    /** A button: its label and symbol on the surface its role spells, hugging its content. */
    private fun button(renderer: ToldRenderer, view: TextView, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        val iconOnly = inherited.iconOnly && tree.text("symbol").isNotEmpty()
        view.text = if (iconOnly) "" else tree.text("label")
        view.textSize = Type.sp("body", inherited)
        view.setTypeface(Type.face(inherited, "body"))
        view.setTextColor(buttonInk(tree, theme))
        view.gravity = Gravity.CENTER
        view.maxLines = 1
        view.minHeight = touchHeight(view.context, inherited)
        val across = controlPadding(view.context, inherited)
        view.setPadding(across, 0, across, 0)
        view.isEnabled = tree.flag("enabled")
        view.alpha = if (tree.flag("enabled")) 1f else 0.45f
        view.background = surfaceOf(buttonFill(tree, theme), buttonCorner(view, inherited, iconOnly).toFloat(), null)
        view.compoundDrawablePadding = dp(view.context, 6.0)
        leadSymbol(view, tree, theme, inherited, iconOnly)
        view.contentDescription = tree.text("label")
        view.setOnClickListener { renderer.send(tree.count("event"), Tree.Null) }
        return true
    }

    /** The button's symbol, leading or trailing as the frame spells. */
    private fun leadSymbol(view: TextView, tree: Tree, theme: ThemeResolver,
                           inherited: Inherited, iconOnly: Boolean) {
        val name = tree.text("symbol")
        view.setCompoundDrawables(null, null, null, null)
        if (name.isEmpty() || !Symbols.known(name)) return
        val size = if (inherited.symbolSize > 0) inherited.symbolSize else if (iconOnly) 18 else 12
        val glyph = Symbols.drawable(view.context, name, buttonInk(tree, theme), size)
        if (inherited.trailingSymbols) view.setCompoundDrawables(null, null, glyph, null)
        else view.setCompoundDrawables(glyph, null, null, null)
    }

    /** A role's ink: filled buttons carry the on-accent ink, destructive the danger tone. */
    private fun buttonInk(tree: Tree, theme: ThemeResolver): Int = when (tree.text("role")) {
        "filled", "prominent" -> theme.colour("onAccent") ?: 0xFFFFFFFF.toInt()
        "destructive" -> theme.colour("danger") ?: 0xFFFF3B30.toInt()
        else -> theme.text()
    }

    /** A role's fill: filled buttons the accent, quiet and plain the soft primary. */
    private fun buttonFill(tree: Tree, theme: ThemeResolver): Int = when (tree.text("role")) {
        "filled", "prominent" -> theme.accent()
        else -> theme.overlay(0.07)
    }

    /** Plain buttons take 8-point corners; an icon-only button is round. */
    private fun buttonCorner(view: View, inherited: Inherited, iconOnly: Boolean): Int {
        if (iconOnly) return touchHeight(view.context, inherited) / 2
        return dp(view.context, 8.0)
    }

    /** A field: its label above a bordered box, with its prompt inside. */
    private fun field(renderer: ToldRenderer, box: LinearLayout, tree: Tree,
                      theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(TextView(box.context))
            box.addView(EditText(box.context))
        }
        val words = box.getChildAt(0) as TextView
        words.text = tree.text("label")
        words.textSize = Type.sp("caption", inherited)
        words.setTextColor(theme.secondary())
        val multiline = tree.flag("multiline")
        words.visibility = if (tree.text("label").isEmpty() || multiline) View.GONE else View.VISIBLE
        val input = box.getChildAt(1) as EditText
        input.hint = tree.text("prompt")
        input.textSize = Type.sp("body", inherited)
        input.setTextColor(theme.text())
        input.setHintTextColor(theme.secondary())
        dressed(renderer, input, tree, multiline, inherited)
        return true
    }

    /** The box itself: bordered on one line, plain when many, always 25 points tall. */
    private fun dressed(renderer: ToldRenderer, view: EditText, tree: Tree,
                        multiline: Boolean, inherited: Inherited) {
        view.minHeight = dp(view.context, 36.0)
        view.setPadding(dp(view.context, 10.0), 0, dp(view.context, 10.0), 0)
        if (multiline) {
            view.background = null
            view.gravity = Gravity.TOP or Gravity.START
            view.minLines = 1
            view.maxLines = 12
        } else {
            view.background = surfaceOf(null, dp(view.context, 6.0).toFloat(), null).apply {
                setStroke(dp(view.context, 1.0), renderer.theme().overlay(0.20))
            }
            view.gravity = Gravity.CENTER_VERTICAL or Gravity.START
        }
        if (view.tag !is Binding || (view.tag as Binding).event != tree.count("event"))
            view.tag = prepared(renderer, view, tree, multiline)
        val held = view.tag as Binding
        // As SwiftUI's FieldView: the frame's value is the truth when it changes —
        // even under the caret — and a submit field follows it every frame.
        if (!held.edits || tree.text("value") != held.echo) {
            held.echo = tree.text("value")
            if (view.text.toString() != held.echo) {
                held.settling = true
                view.setText(held.echo)
                held.settling = false
            }
        }
    }

    /** The binding's once-only preparation: its keyboard and its watcher. */
    private fun prepared(renderer: ToldRenderer, view: EditText, tree: Tree, multiline: Boolean): Binding {
        val edits = tree.text("commit") != "submit"
        val held = Binding(tree.count("event"), edits)
        view.inputType = when {
            tree.flag("secure") -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            multiline -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            else -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        }
        view.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(text: CharSequence, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(text: CharSequence, a: Int, b: Int, c: Int) = Unit
            override fun afterTextChanged(text: Editable) {
                if (!held.settling && edits && view.isFocused)
                    renderer.send(held.event, Tree.Str(text.toString()))
            }
        })
        view.setOnEditorActionListener { _, _, _ ->
            if (!edits) renderer.send(held.event, Tree.Str(view.text.toString()))
            false
        }
        return held
    }

    /** One field's binding, held by its view: the ordinal the frame issued, how it
     * commits, and the value the frame last settled on. */
    private class Binding(val event: Int, val edits: Boolean) {
        var settling = false
        var echo: String? = null
    }

    /** A free-form editor: its label above a plain box, its marks noted beneath. */
    private fun editor(renderer: ToldRenderer, box: LinearLayout, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        field(renderer, box, tree.membersOfEditor(), theme, inherited)
        val marks = tree.list("marks")
        val words = box.getChildAt(0) as TextView
        words.visibility = if (tree.text("label").isEmpty() || tree.flag("inline")) View.GONE else View.VISIBLE
        val input = box.getChildAt(1) as EditText
        input.minHeight = dp(box.context, if (tree.flag("inline")) 44.0 else 180.0)
        while (box.childCount > 2 + marks.size) box.removeViewAt(box.childCount - 1)
        marks.forEachIndexed { index, mark ->
            val note = (box.getChildAt(2 + index) as? TextView) ?: TextView(box.context).also {
                box.addView(it, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                                                          ViewGroup.LayoutParams.WRAP_CONTENT))
            }
            note.text = "Line ${mark.count("line")} ${mark.text("badge")} ${mark.text("note")}"
            note.textSize = Type.sp("caption", inherited)
            note.setTextColor(theme.secondary())
        }
        return true
    }

    /** An editor dressed as the field the SwiftUI editor keeps beside its notes. */
    private fun Tree.membersOfEditor(): Tree {
        if (this is Tree.Obj) {
            val members = members.toMutableMap()
            members["multiline"] = Tree.Bit(true)
            members["label"] = this["label"] ?: Tree.Str("")
            return Tree.Obj(members)
        }
        return this
    }

    /** A switch with its label beside it. */
    private fun toggle(renderer: ToldRenderer, view: Switch, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        view.text = tree.text("label")
        view.textSize = Type.sp("body", inherited)
        view.setTypeface(Type.face(inherited, "body"))
        view.setTextColor(theme.text())
        if (view.tag !== tree.count("event")) {
            view.tag = tree.count("event")
            view.setOnCheckedChangeListener { _, on ->
                renderer.send(tree.count("event"), Tree.Bit(on))
            }
        }
        view.isChecked = tree.flag("on")
        return true
    }

    /** A choice among keys: one menu, the chosen key outlined by its label. */
    private fun picker(renderer: ToldRenderer, view: TextView, tree: Tree,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        val choices = tree.list("choices")
        val chosen = choices.firstOrNull { it.text("key") == tree.text("selected") }
        view.text = chosen?.text("label") ?: tree.text("selected")
        view.textSize = Type.sp("body", inherited)
        view.setTextColor(theme.secondary())
        view.gravity = Gravity.CENTER
        view.minHeight = touchHeight(view.context, inherited)
        val across = dp(view.context, 8.0)
        view.setPadding(across, 0, across, 0)
        view.compoundDrawablePadding = dp(view.context, 4.0)
        Symbols.drawable(view.context, "chevron", theme.secondary(), 8)?.let {
            view.setCompoundDrawables(null, null, it, null)
        }
        view.contentDescription = tree.text("label")
        view.setOnClickListener { subject ->
            val menu = PopupMenu(subject.context, subject)
            choices.forEach { choice ->
                menu.menu.add(choice.text("label")).setOnMenuItemClickListener {
                    renderer.send(tree.count("event"), Tree.Str(choice.text("key")))
                    true
                }
            }
            menu.show()
        }
        return true
    }

    /** Content revealed while `expanded` holds; toggling delivers a flag. */
    private fun disclosure(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                           theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(LinearLayout(box.context).apply { orientation = LinearLayout.HORIZONTAL })
        }
        val head = box.getChildAt(0) as LinearLayout
        head.gravity = Gravity.CENTER_VERTICAL
        head.setOnClickListener { renderer.send(tree.count("event"), Tree.Bit(!tree.flag("expanded"))) }
        chevron(renderer, head, tree, theme, inherited)
        if (tree.flag("expanded")) {
            renderer.children(box, tree, path, theme, inherited, first = 1,
                              gap = inherited.gap ?: theme.spacing(1))
        } else {
            while (box.childCount > 1) box.removeViewAt(box.childCount - 1)
        }
        return true
    }

    /** The disclosure's head: its title in the secondary ink, chevron beside. */
    private fun chevron(renderer: ToldRenderer, head: LinearLayout, tree: Tree,
                        theme: ThemeResolver, inherited: Inherited) {
        if (head.childCount == 0) {
            head.addView(TextView(head.context))
            head.addView(android.widget.ImageView(head.context))
        }
        val words = head.getChildAt(0) as TextView
        words.text = tree.text("title")
        words.textSize = Type.sp("body", inherited)
        words.setTypeface(Type.face(inherited, "body"))
        words.setTextColor(if (tree.flag("expanded")) theme.text() else theme.secondary())
        val mark = head.getChildAt(1) as android.widget.ImageView
        mark.setImageDrawable(Symbols.drawable(head.context, "chevron", theme.secondary(), 8))
        mark.rotation = if (tree.flag("expanded")) 90f else 0f
        mark.setPadding(0, 0, dp(head.context, 2.0), 0)
    }

    /** An alert: no room while hidden; while visible the platform's own alert, with its dismiss. */
    private fun alert(renderer: ToldRenderer, view: View, tree: Tree, theme: ThemeResolver): Boolean {
        val shown = view.tag as? AlertDialog
        if (!tree.flag("visible")) {
            shown?.dismiss()
            view.tag = null
            return true
        }
        if (shown == null) {
            view.tag = AlertDialog.Builder(view.context)
                .setTitle(tree.text("title"))
                .setMessage(tree.text("message"))
                .setPositiveButton("OK") { dialog, _ ->
                    dialog.dismiss()
                    view.tag = null
                    renderer.send(tree.count("event"), Tree.Null)
                }
                .setOnCancelListener { view.tag = null }
                .show()
        }
        return true
    }
}
