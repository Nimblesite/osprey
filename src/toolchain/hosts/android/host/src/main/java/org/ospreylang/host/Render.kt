package org.ospreylang.host

import android.content.Context
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout

// Implements [NATIVE-HOSTS-CONTRACT] and [UI-COMPONENTS] for the Android host:
// each told component kind has exactly one mapping to the platform's own
// views, as the SwiftUI host's Render.swift maps them, and a kind without one
// is a visible refusal and a diagnostic, never a fallback. A node keeps its
// view while its kind stays the same, so fields hold their text, caret and
// focus across the redraws their edits cause. Events carry only the control's
// ordinal and a bounded value.

class ToldRenderer(val context: Context, val store: Store) {
    private val holders = HashMap<String, View>()
    private val visited = mutableSetOf<String>()
    /** The appearance the frame is drawn for. */
    var dark = false

    /** The root region's tree, kept in `page` across frames and centred as
     * SwiftUI places a region shorter than its window. */
    fun render(page: ViewGroup) {
        val region = store.region("root") ?: return
        visited.clear()
        val root = node(region.node, "root", theme(), Inherited())
        if (page.childCount != 1 || page.getChildAt(0) !== root) {
            page.removeAllViews()
            page.addView(root, LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                if (expands(region.node)) ViewGroup.LayoutParams.MATCH_PARENT else ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        holders.keys.removeAll { it.substringBeforeLast('|') !in visited }
    }

    /** Drops every held view, when a plain frame takes the page back. */
    fun clear() = holders.clear()

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
                 inherited: Inherited, first: Int = 0, gap: Int = 0, across: Boolean = false,
                 fromTop: Boolean = false) {
        val kids = tree.list("children")
        while (parent.childCount - first > kids.size) parent.removeViewAt(parent.childCount - 1)
        kids.forEachIndexed { place, child ->
            val view = node(child, "$path/$place", theme, inherited)
            val seat = first + place
            if (parent.getChildAt(seat) !== view) {
                parent.removeView(view)
                parent.addView(view, seat)
            }
            seatView(parent, view, child, place, gap, across, fromTop)
        }
    }

    /** How a stack seats one child: SwiftUI's stacks lead their columns, hug
     * their rows, and let only flexible children take the leftover space. */
    private fun seatView(parent: ViewGroup, view: View, tree: Tree, place: Int, gap: Int,
                         across: Boolean, fromTop: Boolean) {
        val kind = tree.text("kind")
        if (kind == "framed") return
        if (parent !is LinearLayout) {
            // A layer overlaps its children from the top leading corner, as a ZStack does.
            val loose = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.TOP or Gravity.START)
            view.layoutParams = loose
            return
        }
        val flexible = expands(tree)
        val params = LinearLayout.LayoutParams(
            when {
                across && kind == "spacer" -> 0
                across -> ViewGroup.LayoutParams.WRAP_CONTENT
                fills(kind) -> ViewGroup.LayoutParams.MATCH_PARENT
                else -> ViewGroup.LayoutParams.WRAP_CONTENT
            },
            if (across || !flexible) ViewGroup.LayoutParams.WRAP_CONTENT else 0)
        if (flexible && !across) params.weight = 1f
        if (across && kind == "spacer") params.weight = 1f
        if (across) params.gravity = if (fromTop) Gravity.TOP else Gravity.CENTER_VERTICAL
        if (place > 0 && gap > 0) {
            if (across) params.leftMargin = gap else params.topMargin = gap
        }
        view.layoutParams = params
    }

    /** Whether a kind accepts the cross-axis proposal of a vertical stack. */
    private fun fills(kind: String): Boolean = kind in FILLS

    /** Whether a kind takes the leftover space of the axis it sits in. */
    private fun expands(tree: Tree): Boolean = when (tree.text("kind")) {
        "spacer", "empty", "screen", "split", "aside", "rows", "tabs", "layer", "scene", "themed" -> true
        "stack" -> tree.list("children").any { expands(it) }
        else -> false
    }

    /** One new view of the node kind. */
    fun make(kind: String): View = when (kind) {
        "stack" -> LinearLayout(context)
        "layer", "themed", "framed", "slot" -> FrameLayout(context)
        "spacer" -> android.widget.Space(context)
        "divider" -> View(context)
        "alert", "dialog" -> View(context)
        else -> RenderLeaves.make(context, kind)
            ?: RenderStatus.make(context, kind)
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
            "framed" -> RenderFramed.draw(this, view as FrameLayout, tree, path, theme, inherited)
            "slot" -> slot(view as FrameLayout, tree.text("key"), theme, inherited)
            else -> if (!RenderLeaves.update(this, view, tree, theme, inherited))
                if (!RenderStatus.update(this, view, tree, theme, inherited))
                    if (!RenderPages.update(this, view, tree, path, theme, inherited))
                        RenderControls.update(this, view, tree, path, theme, inherited)
        }
    }

    /** Children stacked one way, spaced by the frame's gap or the theme's own step. */
    private fun stack(box: LinearLayout, tree: Tree, path: String, theme: ThemeResolver, inherited: Inherited) {
        val across = tree.text("axis") == "horizontal"
        box.orientation = if (across) LinearLayout.HORIZONTAL else LinearLayout.VERTICAL
        box.gravity = if (across) Gravity.NO_GRAVITY else Gravity.START
        children(box, tree, path, theme, inherited, gap = inherited.gap ?: theme.spacing(1),
                 across = across, fromTop = inherited.rowsFromTop)
    }

    /** A scoped palette inherited by the whole subtree, filling the space it is given. */
    private fun themed(box: FrameLayout, tree: Tree, path: String, inherited: Inherited) {
        val held = ThemeResolver(Theme.of(tree["theme"]) ?: store.theme, dark)
        val body = node(tree["body"] ?: Tree.Null, "$path/body", held, inherited)
        if (box.getChildAt(0) !== body) {
            box.removeAllViews()
            box.addView(body, FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT, Gravity.TOP or Gravity.START))
        }
        val inset = held.spacing(1) * 3
        box.setPadding(inset, inset, inset, inset)
        held.colour("background")?.let { box.setBackgroundColor(it) }
    }

    /** A placeholder the host fills with the region named `key`. */
    private fun slot(box: FrameLayout, key: String, theme: ThemeResolver, inherited: Inherited) {
        val region = store.region(key)
        val filled = region?.let { node(it.node, "region:$key", theme, inherited) }
            ?: refusal(context, "missing region $key")
        if (box.getChildAt(0) !== filled) {
            box.removeAllViews()
            box.addView(filled, FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
    }

    /** Delivers a control event: the ordinal the frame issued and its value. */
    fun send(ordinal: Int, value: Tree) {
        store.send(Touch.control(ordinal.toLong(), value.words))
    }

    companion object {
        /** Kinds that take a vertical stack's width, as SwiftUI's flexible leaves do. */
        private val FILLS = setOf("field", "editor", "divider", "spacer", "empty", "stack", "layer",
                                  "themed", "framed", "slot", "section", "screen", "split", "aside",
                                  "tabs", "rows", "meter", "segments")
    }
}
