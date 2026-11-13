package org.ospreylang.host

import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.Space
import android.widget.TextView

// Implements [NATIVE-HOSTS-CONTRACT] and [UI-COMPONENTS] for the Android host:
// each told component kind has exactly one mapping to the platform's own
// views, and a kind without one is a visible refusal and a diagnostic, never
// a fallback. A node keeps its view while its kind stays the same, so fields
// hold their text, caret and focus across the redraws their edits cause.
// Events carry only the control's ordinal and a bounded value.

class ToldRenderer(val context: Context, private val store: Store) {
    private val holders = HashMap<String, View>()
    private val visited = mutableSetOf<String>()
    /** The appearance the frame is drawn for. */
    var dark = false

    /** The root region's tree, drawn into `page`. */
    fun render(page: ViewGroup) {
        val region = store.region("root") ?: return
        visited.clear()
        page.removeAllViews()
        page.addView(node(region.node, "root", theme(), Inherited()))
        holders.keys.removeAll { it.substringBeforeLast('|') !in visited }
    }

    fun theme() = ThemeResolver(store.theme, dark)

    /** One node's view, made once for its kind and updated in place. */
    fun node(tree: Tree, path: String, theme: ThemeResolver, inherited: Inherited): View {
        val kind = tree.text("kind")
        visited.add(path)
        val view = holders.getOrPut("$path|$kind") { make(kind) }
        update(view, tree, path, theme, inherited)
        return view
    }

    /** The children of `tree` under `parent` from seat `first`, spaced `gap`. */
    fun children(parent: ViewGroup, tree: Tree, path: String, theme: ThemeResolver,
                 inherited: Inherited, first: Int = 0, gap: Int = 0, across: Boolean = false) {
        val kids = tree.list("children")
        while (parent.childCount - first > kids.size) parent.removeViewAt(parent.childCount - 1)
        kids.forEachIndexed { place, child ->
            val view = node(child, "$path/$place", theme, inherited)
            val seat = first + place
            if (parent.getChildAt(seat) !== view) {
                parent.removeView(view)
                parent.addView(view, seat)
            }
            if (place > 0 && gap > 0) spaced(view, gap, across)
        }
    }

    fun spaced(view: View, gap: Int, across: Boolean = false) {
        val params = view.layoutParams as? LinearLayout.LayoutParams ?: return
        if (across) params.leftMargin = gap else params.topMargin = gap
        view.layoutParams = params
    }

    /** One new view of the node kind. */
    fun make(kind: String): View = when (kind) {
        "stack" -> LinearLayout(context)
        "layer", "themed", "framed", "slot" -> FrameLayout(context)
        "spacer" -> Space(context)
        "divider" -> View(context)
        else -> RenderLeaves.make(context, kind)
            ?: RenderPages.make(context, kind)
            ?: RenderControls.make(context, kind)
            ?: refusal(context, kind)
    }

    /** Writes `tree` into the view the node keeps. */
    fun update(view: View, tree: Tree, path: String, theme: ThemeResolver, inherited: Inherited) {
        when (tree.text("kind")) {
            "stack" -> stack(view as LinearLayout, tree, path, theme, inherited)
            "layer" -> children(view as ViewGroup, tree, path, theme, inherited)
            "themed" -> themed(view as FrameLayout, tree, path, inherited)
            "framed" -> RenderPages.framed(this, view as FrameLayout, tree, path, theme, inherited)
            "slot" -> slot(view as FrameLayout, tree.text("key"), theme, inherited)
            else -> if (!RenderLeaves.update(context, view, tree, theme, inherited, store))
                if (!RenderPages.update(this, view, tree, path, theme, inherited))
                    RenderControls.update(this, view, tree, path, theme, inherited)
        }
    }

    /** Children stacked one way, spaced by the theme's own step. */
    private fun stack(box: LinearLayout, tree: Tree, path: String, theme: ThemeResolver, inherited: Inherited) {
        val across = tree.text("axis") == "horizontal"
        box.orientation = if (across) LinearLayout.HORIZONTAL else LinearLayout.VERTICAL
        children(box, tree, path, theme, inherited, gap = theme.spacing(1), across = across)
    }

    /** A scoped palette inherited by the whole subtree. */
    private fun themed(box: FrameLayout, tree: Tree, path: String, inherited: Inherited) {
        val held = ThemeResolver(Theme.of(tree["theme"]) ?: store.theme, dark)
        val body = node(tree["body"] ?: Tree.Null, "$path/body", held, inherited)
        if (box.getChildAt(0) !== body) {
            box.removeAllViews()
            box.addView(body)
        }
        val inset = theme().spacing(1) * 3
        box.setPadding(inset, inset, inset, inset)
        held.colour("background")?.let { box.setBackgroundColor(it) }
    }

    /** A placeholder the host fills with the region named `key`. */
    private fun slot(box: FrameLayout, key: String, theme: ThemeResolver, inherited: Inherited) {
        val region = store.region(key) ?: return
        val filled = node(region.node, "region:$key", theme, inherited)
        if (box.getChildAt(0) !== filled) {
            box.removeAllViews()
            box.addView(filled)
        }
    }

    /** Delivers a control event: the ordinal the frame issued and its value. */
    fun send(ordinal: Int, value: Tree) {
        store.send(Touch.control(ordinal.toLong(), value.words))
    }

    /** One text view, for the pages and controls that lead with words. */
    fun words(): TextView = TextView(context)
}
