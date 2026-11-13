package org.ospreylang.host

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

// Implements [UI-COMPONENTS-FRAMED], [UI-COMPONENTS-SPLITTER] and
// [UI-REGIONS] for the Android host: navigation and page structure. A framed
// node applies the application's insets, dimensions and surfaces
// mechanically; a split's first pane stands on a surface of its own; an
// aside's splitter previews its width while it is dragged and delivers the
// width it lands on to the binding the frame issued.

object RenderPages {
    /** One new view of a page kind, or null when it is no page. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "section" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "screen" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "split", "aside" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        "tabs" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "dialog" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "rows" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "context-menu" -> FrameLayout(context)
        else -> null
    }

    /** Writes `tree` into its page view; false when it is no page. */
    fun update(renderer: ToldRenderer, view: View, tree: Tree, path: String,
               theme: ThemeResolver, inherited: Inherited): Boolean = when (tree.text("kind")) {
        "section" -> section(renderer, view as LinearLayout, tree, path, theme, inherited)
        "screen" -> screen(renderer, view as LinearLayout, tree, path, theme, inherited)
        "split" -> split(renderer, view as LinearLayout, tree, path, theme, inherited)
        "aside" -> aside(renderer, view as LinearLayout, tree, path, theme, inherited)
        "tabs" -> tabs(renderer, view as LinearLayout, tree, path, theme, inherited)
        "dialog" -> dialog(renderer, view as LinearLayout, tree, path, theme, inherited)
        "context-menu" -> contextMenu(renderer, view as ViewGroup, tree, path, theme, inherited)
        "rows" -> rows(renderer, view as LinearLayout, tree, path, theme, inherited)
        else -> false
    }

    /** Application-authored insets, dimensions and surfaces, drawn
     * mechanically, with the frame's own metrics inherited by its body. */
    fun framed(renderer: ToldRenderer, box: FrameLayout, tree: Tree, path: String,
               theme: ThemeResolver, inherited: Inherited): Boolean {
        val style = tree["style"]
        val inner = inherited.merging(style)
        val body = renderer.node(tree["body"] ?: Tree.Null, "$path/body", theme, inner)
        held(renderer, box, body, style)
        return true
    }

    /** The body inside its frame: scrolling when it asks to, inset by the
     * frame's edges, on its surface with its line and fixed extents. */
    private fun held(renderer: ToldRenderer, box: FrameLayout, body: View, style: Tree?) {
        if (style?.flag("scrollY") == true) {
            val scroller = box.getChildAt(0) as? ScrollView ?: ScrollView(box.context).also { box.addView(it, 0) }
            if (scroller.getChildAt(0) !== body) {
                scroller.removeAllViews()
                scroller.addView(body)
            }
        } else if (box.getChildAt(0) !== body) {
            box.removeAllViews()
            box.addView(body)
        }
        style ?: return
        box.setPadding(style.count("left"), style.count("top"), style.count("right"), style.count("bottom"))
        sized(renderer, box, style)
    }

    /** A frame's fixed extents, surface and line, and its fill of the page. */
    private fun sized(renderer: ToldRenderer, box: FrameLayout, style: Tree) {
        val width = style.count("width")
        val height = style.count("height")
        if (width > 0 || height > 0)
            box.layoutParams = LinearLayout.LayoutParams(
                if (width > 0) dp(box.context, width.toDouble()) else ViewGroup.LayoutParams.WRAP_CONTENT,
                if (height > 0) dp(box.context, height.toDouble()) else ViewGroup.LayoutParams.WRAP_CONTENT)
        val face = renderer.theme()
        val fill = face.colour(style.text("fill"))
        val line = face.colour(style.text("line"))
        if (fill != null || line != null)
            box.background = surfaceOf(fill, style.count("radius").toFloat(), line)
        if (style.flag("grow"))
            box.layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT,
                                                          LinearLayout.LayoutParams.MATCH_PARENT, 1f)
    }

    /** A bordered surface headed by its title, with its footer beneath. */
    private fun section(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                        theme: ThemeResolver, inherited: Inherited): Boolean {
        val heading = tree.text("title").isNotEmpty()
        val footer = tree.text("footer").isNotEmpty()
        renderer.children(box, tree, path, theme, inherited, first = (if (heading) 1 else 0) + (if (footer) 1 else 0), gap = theme.spacing(1))
        if (heading) words(box.getChildAt(0) as TextView, tree.text("title"), "headline", theme, bold = true)
        if (footer) words(box.getChildAt(box.childCount - 1) as TextView, tree.text("footer"), "caption", theme)
        box.setPadding(theme.spacing(2), theme.spacing(2), theme.spacing(2), theme.spacing(2))
        box.background = surfaceOf(theme.colour("surface") ?: 0x11808080, 16f, theme.colour("divider"))
        return true
    }

    private fun words(view: TextView, text: String, role: String,
                      theme: ThemeResolver, bold: Boolean = false) {
        view.text = text
        view.textSize = Type.sp(role, Inherited())
        view.setTypeface(Type.face(Inherited(), bold))
        view.setTextColor(if (role == "caption") theme.colour("secondary") ?: Color.DKGRAY else theme.text())
    }

    /** A page: its toolbar row, then its scrolling content. */
    private fun screen(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(LinearLayout(box.context).apply { orientation = LinearLayout.HORIZONTAL })
            box.addView(ScrollView(box.context).apply {
                addView(LinearLayout(context).apply { orientation = LinearLayout.VERTICAL })
            })
        }
        toolbar(renderer, box.getChildAt(0) as LinearLayout, tree, path, theme, inherited)
        val page = (box.getChildAt(1) as ScrollView).getChildAt(0) as LinearLayout
        renderer.children(page, tree, path, theme, inherited, gap = theme.spacing(1))
        return true
    }

    private fun toolbar(renderer: ToldRenderer, strip: LinearLayout, tree: Tree, path: String,
                        theme: ThemeResolver, inherited: Inherited) {
        val tools = tree.list("toolbar")
        while (strip.childCount > tools.size) strip.removeViewAt(strip.childCount - 1)
        tools.forEachIndexed { place, tool ->
            val view = renderer.node(tool, "$path/tool/$place", theme, inherited)
            if (strip.getChildAt(place) !== view) {
                strip.removeView(view)
                strip.addView(view, place)
            }
            if (place > 0) renderer.spaced(view, theme.spacing(1), across = true)
        }
    }

    /** Navigation columns: the first pane on the theme's sidebar surface. */
    private fun split(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                      theme: ThemeResolver, inherited: Inherited): Boolean {
        val panes = tree.list("children")
        while (box.childCount > panes.size) box.removeViewAt(box.childCount - 1)
        panes.forEachIndexed { place, pane ->
            val view = renderer.node(pane, "$path/$place", theme, inherited)
            if (box.getChildAt(place) !== view) {
                box.removeView(view)
                box.addView(view, place)
            }
            if (place == 0 && panes.size > 1) {
                view.layoutParams = LinearLayout.LayoutParams(dp(box.context, 264.0), ViewGroup.LayoutParams.MATCH_PARENT)
                view.setBackgroundColor(theme.colour("sidebar") ?: 0x14000000)
            }
        }
        return true
    }

    /** Two columns divided by a draggable splitter; the drag's end delivers
     * the column's new width to the binding the frame issued. */
    private fun aside(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                      theme: ThemeResolver, inherited: Inherited): Boolean {
        build(renderer, box, tree, path, theme, inherited)
        val column = box.getChildAt(0) as ViewGroup
        val width = tree.count("width").coerceIn(tree.count("least"), maxOf(tree.count("most"), tree.count("least")))
        column.layoutParams = LinearLayout.LayoutParams(dp(box.context, width.toDouble()), ViewGroup.LayoutParams.MATCH_PARENT)
        return true
    }

    /** The aside's column, splitter and pane, in the order its side spells. */
    private fun build(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                      theme: ThemeResolver, inherited: Inherited) {
        if (box.childCount == 3) return
        box.removeAllViews()
        val column = FrameLayout(renderer.context)
        val pane = FrameLayout(renderer.context)
        val ordered = if (tree.text("side") == "trailing") listOf(pane, splitter(renderer, tree, theme), column)
        else listOf(column, splitter(renderer, tree, theme), pane)
        box.addView(ordered[0], LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.MATCH_PARENT))
        box.addView(ordered[1], LinearLayout.LayoutParams(dp(renderer.context, 8.0), ViewGroup.LayoutParams.MATCH_PARENT))
        box.addView(ordered[2], LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT).apply { weight = 1f })
        place(renderer, ordered[0] as FrameLayout, tree["aside"] ?: Tree.Null, "$path/aside", theme, inherited)
        place(renderer, ordered[2] as FrameLayout, tree["pane"] ?: Tree.Null, "$path/pane", theme, inherited)
    }

    private fun place(renderer: ToldRenderer, box: FrameLayout, node: Tree, path: String,
                      theme: ThemeResolver, inherited: Inherited) {
        val view = renderer.node(node, path, theme, inherited)
        if (box.getChildAt(0) !== view) {
            box.removeAllViews()
            box.addView(view)
        }
    }

    /** The splitter: a touch drags the column's preview width inside the
     * limits the frame sent, and lifting delivers the width it lands on. */
    @SuppressLint("ClickableViewAccessibility")
    private fun splitter(renderer: ToldRenderer, tree: Tree, theme: ThemeResolver): View =
        View(renderer.context).apply {
            setBackgroundColor(theme.accent())
            setOnTouchListener { _, motion -> dragged(renderer, this, tree, motion) }
        }

    /** One touch of the splitter: a drag previews the column's width, a lift
     * delivers the width it lands on to the binding the frame issued. */
    private fun dragged(renderer: ToldRenderer, handle: View, tree: Tree, motion: MotionEvent): Boolean {
        var start = 0.0
        var from = tree.count("width")
        return when (motion.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                start = motion.rawX.toDouble(); from = tree.count("width"); true
            }
            MotionEvent.ACTION_MOVE -> {
                val column = (handle.parent as LinearLayout).getChildAt(0)
                column.layoutParams = LinearLayout.LayoutParams(
                    dp(handle.context, clamped(tree, from, start, motion).toDouble()), ViewGroup.LayoutParams.MATCH_PARENT)
                true
            }
            MotionEvent.ACTION_UP -> {
                renderer.send(tree.count("event"), Tree.Num(clamped(tree, from, start, motion).toDouble()))
                true
            }
            else -> false
        }
    }

    private fun clamped(tree: Tree, from: Int, start: Double, motion: MotionEvent): Int {
        val trailing = tree.text("side") == "trailing"
        val moved = (if (trailing) start - motion.rawX else motion.rawX - start).toInt()
        return (from + moved).coerceIn(tree.count("least"), maxOf(tree.count("most"), tree.count("least")))
    }

    /** One tab of many: a row of tab presses and the chosen page. */
    private fun tabs(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                     theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(LinearLayout(box.context).apply { orientation = LinearLayout.HORIZONTAL })
            box.addView(FrameLayout(box.context))
        }
        faces(renderer, box.getChildAt(0) as LinearLayout, tree, theme)
        val selected = tree.text("selected")
        val page = tree.list("tabs").firstOrNull { it.text("key") == selected }?.get("child") ?: Tree.Null
        place(renderer, box.getChildAt(1) as FrameLayout, page, "$path/page/$selected", theme, inherited)
        return true
    }

    private fun faces(renderer: ToldRenderer, strip: LinearLayout, tree: Tree, theme: ThemeResolver) {
        val tabs = tree.list("tabs")
        while (strip.childCount > tabs.size) strip.removeViewAt(strip.childCount - 1)
        tabs.forEachIndexed { index, tab ->
            val face = (strip.getChildAt(index) as? TextView) ?: TextView(strip.context).also { strip.addView(it) }
            face.text = tab.text("label").ifEmpty { tab.text("key") }
            face.setTextColor(if (tab.text("key") == tree.text("selected")) theme.accent() else theme.text())
            face.setPadding(dp(strip.context, 16.0), dp(strip.context, 12.0), dp(strip.context, 16.0), dp(strip.context, 12.0))
            face.setOnClickListener { renderer.send(tree.count("event"), Tree.Str(tab.text("key"))) }
        }
    }

    /** A modal card: its title, its body and its own dismiss press. */
    private fun dialog(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(renderer.words().apply { gravity = Gravity.END })
            box.addView(renderer.words())
        }
        (box.getChildAt(0) as TextView).apply {
            text = "✕"
            setOnClickListener { renderer.send(tree.count("event"), Tree.Null) }
        }
        words(box.getChildAt(1) as TextView, tree.text("title"), "title", theme, bold = true)
        renderer.children(box, tree, path, theme, inherited, first = 2, gap = theme.spacing(1))
        box.setPadding(theme.spacing(2), theme.spacing(2), theme.spacing(2), theme.spacing(2))
        box.background = surfaceOf(theme.colour("surface") ?: Color.WHITE, 16f, theme.colour("divider"))
        return true
    }

    /** Secondary actions: the body drawn, the items on its context menu. */
    @SuppressLint("ClickableViewAccessibility")
    private fun contextMenu(renderer: ToldRenderer, box: ViewGroup, tree: Tree, path: String,
                            theme: ThemeResolver, inherited: Inherited): Boolean {
        val body = renderer.node(tree["body"] ?: Tree.Null, "$path/body", theme, inherited)
        if (box.getChildAt(0) !== body) {
            box.removeAllViews()
            box.addView(body)
            body.setOnLongClickListener { subject ->
                val menu = android.widget.PopupMenu(subject.context, subject)
                tree.list("items").forEach { item ->
                    menu.menu.add(item.text("label").ifEmpty { item.text("title") })
                }
                menu.show()
                true
            }
        }
        return true
    }

    /** Selectable rows: a group's heading once, each entry's cell, the chosen
     * row selected; choosing one delivers its key to the binding. */
    private fun rows(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                     theme: ThemeResolver, inherited: Inherited): Boolean {
        val wanted = mutableListOf<View>()
        var lastGroup = "\uFFFF"
        tree.list("rows").forEachIndexed { index, entry ->
            val group = entry.text("group")
            if (group.isNotEmpty() && group != lastGroup)
                wanted.add(heading(renderer, group, theme))
            lastGroup = group
            wanted.add(row(renderer, entry, tree, "$path/$index/cell", theme, inherited))
        }
        seat(box, wanted)
        return true
    }

    private fun heading(renderer: ToldRenderer, group: String, theme: ThemeResolver): TextView =
        TextView(renderer.context).apply {
            text = group
            textSize = Type.sp("caption", Inherited())
            setTextColor(theme.colour("secondary") ?: Color.DKGRAY)
            setPadding(0, dp(context, 8.0), 0, dp(context, 4.0))
        }

    /** One entry's cell, chosen cells on the accent surface; choosing one
     * delivers its key to the binding the frame issued. */
    private fun row(renderer: ToldRenderer, entry: Tree, tree: Tree, path: String,
                    theme: ThemeResolver, inherited: Inherited): View {
        val cell = renderer.node(entry["child"] ?: Tree.Null, path, theme, inherited)
        val chosen = entry.text("key") == tree.text("selected")
        cell.setBackgroundColor(if (chosen) theme.colour("accentFill") ?: 0x14000000 else Color.TRANSPARENT)
        cell.setOnClickListener { renderer.send(tree.count("event"), Tree.Str(entry.text("key"))) }
        return cell
    }

    /** The box shows `wanted` in order. */
    private fun seat(box: LinearLayout, wanted: List<View>) {
        while (box.childCount > wanted.size) box.removeViewAt(box.childCount - 1)
        wanted.forEachIndexed { place, view ->
            if (box.getChildAt(place) !== view) {
                box.removeView(view)
                box.addView(view, place)
            }
        }
    }
}
