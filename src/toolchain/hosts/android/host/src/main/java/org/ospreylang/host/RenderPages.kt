package org.ospreylang.host

import android.annotation.SuppressLint
import android.app.Dialog
import android.content.Context
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.TextView

// Implements [UI-COMPONENTS-FRAMED], [UI-COMPONENTS-SPLITTER] and
// [UI-REGIONS] for the Android host: navigation and page structure, as the
// SwiftUI host's Layout.swift and Controls.swift draw them. A split's first
// pane stands on the theme's sidebar surface; an aside's splitter previews
// its width while it is dragged and delivers the width it lands on.

object RenderPages {
    /** One new view of a page kind, or null when it is no page. */
    fun make(context: Context, kind: String): View? = when (kind) {
        "section", "screen", "rows" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        "split", "aside" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
        "tabs" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
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
        "dialog" -> dialog(renderer, view, tree, path, theme, inherited)
        "context-menu" -> contextMenu(renderer, view as ViewGroup, tree, path, theme, inherited)
        "rows" -> rows(renderer, view as LinearLayout, tree, path, theme, inherited)
        else -> false
    }

    /** A group headed by its title, its footer beneath: no surface of its own. The title
     * keeps the first seat and the footer the last, each one view reused across frames. */
    private fun section(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                        theme: ThemeResolver, inherited: Inherited): Boolean {
        val heading = tree.text("title").isNotEmpty()
        val footer = tree.text("footer").isNotEmpty()
        val head = own(box, "section-title")
        val foot = own(box, "section-footer")
        listOfNotNull(head, foot).forEach { box.removeView(it) }
        if (heading) box.addView(head ?: tagged(box, "section-title"), 0)
        renderer.children(box, tree, path, theme, inherited, first = if (heading) 1 else 0, gap = dp(box.context, 8.0))
        if (heading) words(box.getChildAt(0) as TextView, tree.text("title"), "eyebrow", theme, bold = true)
        if (footer) {
            val tail = foot ?: tagged(box, "section-footer")
            box.addView(tail)
            words(tail, tree.text("footer"), "caption", theme)
        }
        return true
    }

    /** The section's own text view tagged `tag` among its direct children, never a nested section's. */
    private fun own(box: LinearLayout, tag: String): TextView? =
        (0 until box.childCount).map(box::getChildAt).firstOrNull { it.tag == tag } as? TextView

    private fun tagged(box: LinearLayout, tag: String) = TextView(box.context).apply { this.tag = tag }

    private fun words(view: TextView, text: String, role: String,
                      theme: ThemeResolver, bold: Boolean = false) {
        view.text = text
        view.textSize = Type.sp(role, Inherited())
        view.setTypeface(Type.face(Inherited(), if (bold) "headline" else role))
        view.setTextColor(if (role == "caption" || role == "eyebrow") theme.secondary() else theme.text())
    }

    /** A page: its heading with its toolbar beside it, then its content. */
    private fun screen(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(LinearLayout(box.context).apply { orientation = LinearLayout.HORIZONTAL })
        }
        headed(renderer, box.getChildAt(0) as LinearLayout, tree, path, theme, inherited)
        renderer.children(box, tree, path, theme, inherited, first = 1, gap = dp(box.context, 12.0))
        return true
    }

    /** The heading row: the title with its subtitle beneath, the toolbar beside them. */
    private fun headed(renderer: ToldRenderer, strip: LinearLayout, tree: Tree, path: String,
                       theme: ThemeResolver, inherited: Inherited) {
        if (strip.childCount == 0) {
            strip.addView(LinearLayout(strip.context).apply { orientation = LinearLayout.VERTICAL },
                          LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            strip.addView(LinearLayout(strip.context).apply { orientation = LinearLayout.HORIZONTAL })
        }
        val headed = strip.getChildAt(0) as LinearLayout
        if (headed.childCount == 0) {
            headed.addView(TextView(headed.context))
            headed.addView(TextView(headed.context))
            headed.setPadding(0, 0, 0, dp(headed.context, 2.0))
        }
        words(headed.getChildAt(0) as TextView, tree.text("title"), "title", theme, bold = true)
        words(headed.getChildAt(1) as TextView, tree.text("subtitle"), "caption", theme)
        headed.getChildAt(1).visibility = if (tree.text("subtitle").isEmpty()) View.GONE else View.VISIBLE
        toolbar(renderer, strip.getChildAt(1) as LinearLayout, tree, path, theme, inherited)
    }

    private fun toolbar(renderer: ToldRenderer, tools: LinearLayout, tree: Tree, path: String,
                        theme: ThemeResolver, inherited: Inherited) {
        tools.gravity = Gravity.CENTER_VERTICAL
        val bar = tree.list("toolbar")
        while (tools.childCount > bar.size) tools.removeViewAt(tools.childCount - 1)
        bar.forEachIndexed { place, tool ->
            val view = renderer.node(tool, "$path/tool/$place", theme, inherited)
            if (tools.getChildAt(place) !== view) {
                tools.removeView(view)
                tools.addView(view, place, LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            }
        }
    }

    /** Navigation columns: the first pane on the theme's sidebar surface, the rest filling. */
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
            view.layoutParams = if (place == 0 && panes.size > 1)
                LinearLayout.LayoutParams(dp(box.context, 264.0), ViewGroup.LayoutParams.MATCH_PARENT)
            else LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT).apply { weight = 1f }
            if (place == 0 && panes.size > 1)
                view.setBackgroundColor(theme.colour("sidebar") ?: theme.overlay(0.05))
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
        val handle = splitter(renderer, tree, theme)
        val ordered = if (tree.text("side") == "trailing") listOf(pane, handle, column)
        else listOf(column, handle, pane)
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
            box.addView(view, FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }
    }

    /** The splitter: a divider-coloured strip inside its drag area. */
    @SuppressLint("ClickableViewAccessibility")
    private fun splitter(renderer: ToldRenderer, tree: Tree, theme: ThemeResolver): View =
        View(renderer.context).apply {
            background = surfaceOf(theme.colour("divider") ?: theme.overlay(0.15), 0f, null)
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

    /** One tab of many: a row of tab faces and the chosen page beneath. */
    private fun tabs(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                     theme: ThemeResolver, inherited: Inherited): Boolean {
        if (box.childCount == 0) {
            box.addView(LinearLayout(box.context).apply { orientation = LinearLayout.HORIZONTAL })
            box.addView(FrameLayout(box.context), LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
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
            val face = (strip.getChildAt(index) as? TextView) ?: TextView(strip.context).also {
                strip.addView(it, LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            }
            face.text = tab.text("label").ifEmpty { tab.text("key") }
            face.textSize = Type.sp("body", Inherited())
            face.setTypeface(Type.face(Inherited(), if (tab.text("key") == tree.text("selected")) "headline" else "body"))
            face.setTextColor(if (tab.text("key") == tree.text("selected")) theme.accent() else theme.secondary())
            face.setPadding(dp(strip.context, 16.0), dp(strip.context, 12.0), dp(strip.context, 16.0), dp(strip.context, 12.0))
            face.setOnClickListener { renderer.send(tree.count("event"), Tree.Str(tab.text("key"))) }
        }
    }

    /** A modal card the platform presents; its only view takes no room, as the SwiftUI sheet does. */
    private fun dialog(renderer: ToldRenderer, view: View, tree: Tree, path: String,
                       theme: ThemeResolver, inherited: Inherited): Boolean {
        val held = view.tag as? Dialog
        if (held != null && !held.isShowing) view.tag = null
        if (view.tag == null) {
            val card = Dialog(view.context)
            card.setTitle(tree.text("title"))
            card.setOnCancelListener { renderer.send(tree.count("event"), Tree.Null) }
            card.setContentView(LinearLayout(view.context).apply { orientation = LinearLayout.VERTICAL })
            card.show()
            view.tag = card
        }
        body(renderer, view.tag as Dialog, tree, path, theme, inherited)
        return true
    }

    /** The dialog's body, drawn once into the card the platform presented. */
    private fun body(renderer: ToldRenderer, card: Dialog, tree: Tree, path: String,
                     theme: ThemeResolver, inherited: Inherited) {
        val content = card.findViewById<ViewGroup>(android.R.id.content) ?: return
        val box = content.getChildAt(0) as? LinearLayout ?: return
        if (box.childCount == 0)
            box.addView(renderer.node(tree["body"] ?: Tree.Null, "$path/body", theme, inherited),
                        LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    }

    /** Secondary actions: the body drawn, the items on its context menu. */
    @SuppressLint("ClickableViewAccessibility")
    private fun contextMenu(renderer: ToldRenderer, box: ViewGroup, tree: Tree, path: String,
                            theme: ThemeResolver, inherited: Inherited): Boolean {
        val body = renderer.node(tree["body"] ?: Tree.Null, "$path/body", theme, inherited)
        if (box.getChildAt(0) !== body) {
            box.removeAllViews()
            box.addView(body, FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            body.setOnLongClickListener { subject ->
                val menu = PopupMenu(subject.context, subject)
                tree.list("items").forEach { item ->
                    menu.menu.add(item.text("label").ifEmpty { item.text("title") })
                        .setOnMenuItemClickListener {
                            renderer.send(item.count("event"), Tree.Null)
                            true
                        }
                }
                menu.show()
                true
            }
        }
        return true
    }

    /** Selectable rows: a flat list, or the card list a `flatRows` frame authors. */
    private fun rows(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                     theme: ThemeResolver, inherited: Inherited): Boolean {
        if (inherited.flatRows) authored(renderer, box, tree, path, theme, inherited)
        else flat(renderer, box, tree, path, theme, inherited)
        return true
    }

    /** A flat list of rows, each choosing its key; the chosen row tinted. */
    private fun flat(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                     theme: ThemeResolver, inherited: Inherited) {
        val entries = tree.list("rows")
        while (box.childCount > entries.size) box.removeViewAt(box.childCount - 1)
        entries.forEachIndexed { index, entry ->
            val row = cardRow(renderer, box, index, entry, tree, "$path/$index", theme, inherited,
                              Gravity.CENTER_VERTICAL, 12, dp(box.context, 2.0))
            row.background = surfaceOf(if (entry.text("key") == tree.text("selected")) theme.overlay(0.10) else null,
                                       dp(row.context, 7.0).toFloat(), null)
        }
    }

    /** The list an application authored: consecutive rows that name one group under its heading. */
    private fun authored(renderer: ToldRenderer, box: LinearLayout, tree: Tree, path: String,
                         theme: ThemeResolver, inherited: Inherited) {
        val entries = tree.list("rows")
        val wanted = mutableListOf<Pair<Boolean, Int>>()
        var lastGroup = "\uFFFF"
        entries.forEachIndexed { index, entry ->
            val group = entry.text("group")
            if (group.isNotEmpty() && group != lastGroup) wanted.add(true to index)
            lastGroup = group
            wanted.add(false to index)
        }
        while (box.childCount > wanted.size) box.removeViewAt(box.childCount - 1)
        wanted.forEachIndexed { seat, (heading, index) ->
            if (heading) headingRow(renderer, box, seat, entries[index], theme)
            else {
                val row = cardRow(renderer, box, seat, entries[index], tree, "$path/$index", theme, inherited,
                                  Gravity.TOP, 22, dp(box.context, (inherited.gap ?: theme.spacing(2)).toDouble()))
                row.setPadding(0, 0, 0, 0)
                val selection = theme.colour("selection")
                row.background = if (entries[index].text("key") == tree.text("selected") && selection != null)
                    surfaceOf(selection, dp(row.context, 3.0).toFloat(), null) else null
            }
        }
    }

    private fun headingRow(renderer: ToldRenderer, box: LinearLayout, seat: Int, entry: Tree,
                           theme: ThemeResolver) {
        val words = (box.getChildAt(seat) as? TextView) ?: TextView(box.context).also {
            box.addView(it, seat, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        words.text = entry.text("group")
        words.textSize = Type.sp("eyebrow", Inherited())
        words.setTypeface(Type.face(Inherited(), "eyebrow"))
        words.setTextColor(theme.secondary())
        words.setPadding(0, dp(words.context, 8.0), 0, 0)
    }

    /** One row of the list: its symbol, its cell, the selection tint the frame spells. */
    private fun cardRow(renderer: ToldRenderer, box: LinearLayout, seat: Int, entry: Tree,
                        tree: Tree, path: String, theme: ThemeResolver, inherited: Inherited,
                        gravity: Int, symbol: Int, gap: Int): LinearLayout {
        val cell = renderer.node(entry["child"] ?: Tree.Null, "$path/cell", theme, inherited)
        val row = (box.getChildAt(seat) as? LinearLayout) ?: LinearLayout(box.context).also { fresh ->
            fresh.orientation = LinearLayout.HORIZONTAL
            fresh.gravity = gravity
            fresh.setPadding(dp(fresh.context, 8.0), dp(fresh.context, 6.0), dp(fresh.context, 8.0), dp(fresh.context, 6.0))
            box.addView(fresh, seat, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        if (row.getChildAt(1) !== cell) {
            while (row.childCount > 0) row.removeViewAt(0)
            row.addView(ImageView(row.context), LinearLayout.LayoutParams(
                if (symbol > 12) dp(row.context, 30.0) else ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT))
            row.addView(cell, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }
        mark(row, entry, theme, symbol)
        row.setOnClickListener { renderer.send(tree.count("event"), Tree.Str(entry.text("key"))) }
        val params = row.layoutParams as? LinearLayout.LayoutParams
        if (params != null) params.topMargin = if (seat > 0) gap else 0
        row.layoutParams = params
        return row
    }

    /** The row's symbol, tinted with the secondary ink. */
    private fun mark(row: LinearLayout, entry: Tree, theme: ThemeResolver, size: Int) {
        val symbol = row.getChildAt(0) as? ImageView ?: return
        symbol.setImageDrawable(null)
        if (entry.text("symbol").isNotEmpty())
            symbol.setImageDrawable(Symbols.drawable(row.context, entry.text("symbol"), theme.secondary(), size))
        symbol.setPadding(0, 0, dp(row.context, 8.0), 0)
    }
}
