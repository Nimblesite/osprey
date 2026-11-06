package org.ospreylang.talon

import android.app.Activity
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.InputMethodManager
import android.widget.*
import org.json.JSONObject

/** Native Android widgets for the exact semantic view produced for the web app. */
internal class BankRenderer(private val activity: Activity, private val send: (JSONObject) -> Unit, private val settings: () -> Unit) {
    private val ink = Color.rgb(10, 36, 38)
    private val paper = Color.rgb(247, 247, 242)
    private val coral = Color.rgb(255, 107, 74)
    private val mint = Color.rgb(158, 226, 195)
    private val muted = Color.rgb(113, 137, 135)
    private val green = Color.rgb(30, 121, 90)
    private val line = Color.rgb(233, 236, 232)
    private val font = try { Typeface.createFromAsset(activity.assets, "fonts/InterVariable.ttf") } catch (_: Exception) { Typeface.create("sans-serif", Typeface.NORMAL) }
    val root = FrameLayout(activity).apply { setBackgroundColor(paper) }
    private val controls = linkedMapOf<String, View>()
    private val selectValues = mutableMapOf<String, List<String>>()
    private val drafts = linkedMapOf<String, String>()
    private val activeDrafts = mutableSetOf<String>()
    private val forms = mutableMapOf<String, LinkedHashMap<String, () -> String>>()
    private val handler = Handler(Looper.getMainLooper())
    private var inputEvent: Runnable? = null
    private var scroll: ScrollView? = null
    private var route = ""
    private var busy = false
    private var rebuilding = false
    private var modal = false
    private var restoredFocus = ""
    private var restoredScroll = 0
    private val tablet: Boolean get() = activity.resources.configuration.screenWidthDp >= 860

    fun snapshot(): String {
        controls.forEach { (key, view) ->
            if (view is EditText) drafts[key] = view.text.toString()
            if (view is Spinner) selectValues[key]?.getOrNull(view.selectedItemPosition)?.let { drafts[key] = it }
        }
        return JSONObject().put("drafts", JSONObject(drafts.toMap())).put("focus", focusKey()).put("scroll", scroll?.scrollY ?: 0).put("route", route).toString()
    }

    fun restore(source: String) {
        if (source.isBlank()) return
        val saved = JSONObject(source)
        val values = saved.optJSONObject("drafts") ?: JSONObject()
        values.keys().forEach { drafts[it] = values.optString(it) }
        restoredFocus = saved.optString("focus")
        restoredScroll = saved.optInt("scroll")
        route = saved.optString("route")
    }

    fun render(envelope: JSONObject) {
        val model = JSONObject(envelope.optString("model", "{}"))
        val nextRoute = model.optString("route")
        val sameRoute = nextRoute == route
        val closingModal = modal && model.optString("modal") != "open"
        if (!sameRoute || closingModal) {
            (activity.getSystemService(Activity.INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(root.windowToken, 0)
            root.clearFocus()
        }
        val oldFocus = if (sameRoute && !closingModal) focusKey().ifBlank { restoredFocus } else ""
        val oldSelection = (controls[oldFocus] as? EditText)?.selectionStart ?: -1
        val oldScroll = if (sameRoute) scroll?.scrollY ?: restoredScroll else 0
        snapshot()
        route = nextRoute; busy = model.optString("busy") == "true"; modal = model.optString("modal") == "open"
        rebuilding = true
        root.removeAllViews(); controls.clear(); selectValues.clear(); activeDrafts.clear(); forms.clear()
        val tree = envelope.optJSONObject("view") ?: JSONObject()
        val nodes = children(tree)
        val sidebar = nodes.firstOrNull { cls(it).contains("sidebar") }
        val main = nodes.firstOrNull { cls(it).contains("app-main") }
        val body = LinearLayout(activity).apply { orientation = LinearLayout.VERTICAL }
        val bodyParams = FrameLayout.LayoutParams(-1, -1)
        if (tablet) bodyParams.leftMargin = dp(238)
        root.addView(body, bodyParams)
        if (main != null) {
            children(main).forEach { node ->
                if (cls(node).contains("content")) {
                    scroll = ScrollView(activity).apply { isFillViewport = true; clipToPadding = false }
                    scroll!!.addView(renderNode(node), ViewGroup.LayoutParams(-1, -2))
                    body.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
                } else if (cls(node).contains("top-progress")) {
                    body.addView(ProgressBar(activity, null, android.R.attr.progressBarStyleHorizontal).apply { isIndeterminate = true }, LinearLayout.LayoutParams(-1, dp(3)))
                } else body.addView(renderNode(node), LinearLayout.LayoutParams(-1, -2))
            }
        }
        if (sidebar != null && (tablet || cls(sidebar).contains("open"))) {
            if (!tablet) {
                root.addView(View(activity).apply { setBackgroundColor(0x66071a1c); contentDescription = "Close navigation"; setOnClickListener { click("toggle-menu") } }, FrameLayout.LayoutParams(-1, -1))
            }
            val menu = ScrollView(activity).apply { setBackgroundColor(ink); isFillViewport = true; elevation = dp(12).toFloat() }
            val menuContent = renderNode(sidebar, dark = true) as LinearLayout
            menuContent.addView(button("Server connection", "text-button", true).apply { tag = "server-settings"; setOnClickListener { settings() } })
            menu.addView(menuContent)
            root.addView(menu, FrameLayout.LayoutParams(dp(if (tablet) 238 else minOf(300, activity.resources.configuration.screenWidthDp - 56)), -1))
        }
        nodes.firstOrNull { cls(it).contains("modal-backdrop") }?.let { node ->
            root.addView(FrameLayout(activity).apply { setBackgroundColor(0x99071a1c.toInt()); isClickable = true }, FrameLayout.LayoutParams(-1, -1))
            val sheet = ScrollView(activity).apply { isFillViewport = false; elevation = dp(20).toFloat(); background = background(Color.WHITE, 26); clipToOutline = true }
            children(node).firstOrNull()?.let { sheet.addView(renderNode(it)) }
            val width = if (tablet) dp(520) else -1
            root.addView(sheet, FrameLayout.LayoutParams(width, -2, if (tablet) Gravity.CENTER else Gravity.BOTTOM))
        }
        nodes.firstOrNull { cls(it).contains("toast") }?.let { node ->
            root.addView(renderNode(node, dark = true).apply { accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE; elevation = dp(30).toFloat() }, FrameLayout.LayoutParams(-1, -2, if (modal) Gravity.TOP else Gravity.BOTTOM).apply { setMargins(dp(16), dp(16), dp(16), dp(16)) })
        }
        // Hydration temporarily substitutes a loading view for the form. Keep
        // unfinished input until the real route returns, including restoration
        // after Android has recreated the process.
        if (model.optString("loading") != "true") drafts.keys.retainAll(activeDrafts)
        rebuilding = false; restoredFocus = ""; restoredScroll = 0
        root.post {
            scroll?.scrollTo(0, oldScroll)
            (controls[oldFocus] as? EditText)?.let { input ->
                input.requestFocus()
                input.setSelection(if (oldSelection >= 0) minOf(oldSelection, input.length()) else input.length())
            }
        }
    }

    fun command(command: JSONObject) {
        if (command.optString("kind") == "focus") root.post {
            (controls[command.optString("id")] as? EditText)?.let { input ->
                input.requestFocus()
                (activity.getSystemService(Activity.INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(input, InputMethodManager.SHOW_IMPLICIT)
            }
        }
        // Navigation has already updated the shared Osprey model. Android Back
        // dispatches a location event, so there is no second route implementation.
    }

    fun control(id: String): View? = controls[id]
    private fun click(id: String) = send(JSONObject().put("kind", "click").put("id", id))
    private fun focusKey() = controls.entries.firstOrNull { it.value is EditText && it.value.isFocused }?.key ?: ""
    private fun children(node: JSONObject): List<JSONObject> {
        val values = node.optJSONArray("children") ?: return emptyList()
        return (0 until values.length()).mapNotNull { values.optJSONObject(it) }
    }
    private fun props(node: JSONObject) = node.optJSONObject("props") ?: JSONObject()
    private fun cls(node: JSONObject) = props(node).optString("className").split(' ').filter { it.isNotBlank() }.toSet()
    private fun allText(node: JSONObject): String = (listOf(node.optString("text")) + children(node).map { allText(it) }).filter { it.isNotBlank() }.joinToString(" ")
    private fun accessibleText(node: JSONObject): String = if (props(node).optString("aria-hidden") == "true" || cls(node).any { it in setOf("button-icon", "nav-icon", "account-mark", "account-more") }) "" else (listOf(node.optString("text")) + children(node).map { accessibleText(it) }).filter { it.isNotBlank() }.joinToString(" ")

    private fun renderNode(node: JSONObject, form: String = "", dark: Boolean = false, label: String = ""): View {
        val p = props(node); val c = cls(node); val tag = node.optString("tag", "div")
        val id = p.optString("id")
        if (c.contains("security-orbit")) return decoration(true)
        if (p.optString("hidden") == "true" || c.any { it in setOf("orb", "notification-dot", "hero-art") } || (tablet && c.contains("mobile-menu"))) return View(activity).apply { visibility = View.GONE }
        if (tag == "input" || tag == "textarea") return input(node, form, label)
        if (tag == "select") return select(node, form, label)
        if (tag == "button" && !c.contains("account-card")) {
            var title = allText(node)
            if (c.contains("icon-button") || c.contains("toast-close") || (c.contains("top-open") && !tablet)) title = children(node).firstOrNull()?.let { allText(it) } ?: title
            return button(title, c.joinToString(" "), dark).apply {
                this.tag = id.ifBlank { if (p.optString("type") == "submit") "$form-button" else "" }
                contentDescription = p.optString("aria-label").ifBlank { accessibleText(node) }
                if (id.isNotBlank()) controls[id] = this
                val submit = p.optString("type") == "submit"
                isEnabled = p.optString("disabled") != "true" && (!submit || !busy)
                alpha = if (isEnabled) 1f else .45f
                setOnClickListener {
                    if (submit) {
                        val data = JSONObject()
                        forms[form]?.forEach { (name, read) -> data.put(name, read()) }
                        (activity.getSystemService(Activity.INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(windowToken, 0)
                        root.clearFocus()
                        send(JSONObject().put("kind", "submit").put("id", form).put("data", data.toString()))
                    } else event(node, "click")
                }
            }
        }
        val darkHere = dark || c.any { it in setOf("hero-card", "guidance-card", "security-hero", "sidebar", "toast") }
        val kids = children(node)
        if (kids.isEmpty()) {
            if (c.contains("skeleton")) return View(activity).apply { background = background(line, 20); minimumHeight = dp(if (c.contains("skeleton-hero")) 210 else 80) }
            return text(node.optString("text"), tag, c, darkHere).apply { if (id.isNotBlank()) { this.tag = id; controls[id] = this } }
        }
        val horizontal = c.any { it in horizontalClasses } || (tag == "li" && darkHere)
        val box = LinearLayout(activity).apply {
            orientation = if (horizontal) LinearLayout.HORIZONTAL else LinearLayout.VERTICAL
            gravity = if (horizontal) Gravity.CENTER_VERTICAL else Gravity.TOP
            clipChildren = false
        }
        box.tag = id.ifBlank { c.firstOrNull() }
        if (id.isNotBlank()) controls[id] = box
        val formId = if (tag == "form") id else form
        if (tag == "form") forms[formId] = linkedMapOf()
        val accessibleLabel = if (tag == "label") kids.firstOrNull()?.let { allText(it) }.orEmpty() else label
        val gap = when {
            c.any { it in setOf("page", "money-form", "modal-form") } -> 22
            c.any { it.endsWith("grid") || it in setOf("account-grid-wide", "section-block", "tip-list", "architecture-stack", "main-nav") } -> 14
            horizontal -> 10
            else -> 5
        }
        val rawText = node.optString("text")
        if (rawText.isNotBlank()) box.addView(text(rawText, tag, c, darkHere))
        kids.forEachIndexed { index, child ->
            val view = renderNode(child, formId, darkHere, accessibleLabel)
            val childClass = cls(child)
            val weight = horizontal && (childClass.any { it in flexibleClasses } || (index == 0 && c.any { it in setOf("modal-heading", "detail-heading", "section-heading") }) || (index == 1 && c.any { it in setOf("form-intro", "architecture-layer", "sidebar-profile", "modal-reassurance") }))
            val lp = LinearLayout.LayoutParams(if (horizontal) { if (weight) 0 else -2 } else -1, -2, if (weight) 1f else 0f)
            if (index > 0) { if (horizontal) lp.leftMargin = dp(gap) else lp.topMargin = dp(gap) }
            if (c.contains("segmented")) { lp.width = 0; lp.weight = 1f; lp.leftMargin = if (index > 0) dp(3) else 0 }
            if (c.contains("segmented") && childClass.contains("active") && view is Button) {
                view.background = background(Color.WHITE, 10); view.setTextColor(ink); view.elevation = dp(2).toFloat()
            }
            box.addView(view, lp)
        }
        styleBox(box, c, darkHere)
        if (c.contains("account-card")) {
            box.isClickable = true; box.isFocusable = true
            box.contentDescription = accessibleText(node)
            box.setOnClickListener { event(node, "click") }
        }
        if (c.contains("topbar")) box.setOnLongClickListener { settings(); true }
        if (c.contains("hero-card")) return FrameLayout(activity).apply {
            background = box.background; box.background = null
            addView(decoration(false), FrameLayout.LayoutParams(dp(130), dp(180), Gravity.END or Gravity.CENTER_VERTICAL))
            addView(box, FrameLayout.LayoutParams(-1, -2))
        }
        if (activity.resources.configuration.screenWidthDp >= 620 && c.any { it in setOf("account-grid", "stat-grid", "mini-stat-grid") }) {
            val columns = if (activity.resources.configuration.screenWidthDp >= 1180) 3 else 2
            val grid = GridLayout(activity).apply { columnCount = columns; useDefaultMargins = false; alignmentMode = GridLayout.ALIGN_BOUNDS }
            while (box.childCount > 0) {
                val child = box.getChildAt(0)
                box.removeView(child)
                val position = grid.childCount
                grid.addView(child, GridLayout.LayoutParams(GridLayout.spec(position / columns), GridLayout.spec(position % columns, 1f)).apply {
                    width = 0; height = -2
                    if (position % columns > 0) leftMargin = dp(14)
                    if (position >= columns) topMargin = dp(14)
                })
            }
            return grid
        }
        if (c.contains("filter-row") || c.contains("detail-actions")) return HorizontalScrollView(activity).apply { isHorizontalScrollBarEnabled = false; addView(box, ViewGroup.LayoutParams(-2, -2)) }
        return box
    }

    private fun input(node: JSONObject, form: String, label: String): EditText {
        val p = props(node); val id = p.optString("id", p.optString("name")); val name = p.optString("name")
        activeDrafts.add(id)
        return EditText(activity).apply {
            tag = id; controls[id] = this
            textSize = 15f; typeface = font; setTextColor(ink); setHintTextColor(muted)
            setPadding(dp(14), dp(11), dp(14), dp(11)); minHeight = dp(48)
            background = background(Color.WHITE, 13, Color.rgb(221, 227, 223))
            hint = p.optString("placeholder"); contentDescription = label.ifBlank { hint }
            isSingleLine = node.optString("tag") != "textarea"
            inputType = if (p.optString("inputMode") == "decimal") InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL else InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            setText(if (p.has("value")) p.optString("value") else drafts[id] ?: p.optString("defaultValue"))
            isEnabled = p.optString("disabled") != "true"
            if (form.isNotEmpty() && name.isNotEmpty()) forms.getOrPut(form) { linkedMapOf() }[name] = { text.toString() }
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                override fun afterTextChanged(value: Editable?) {
                    drafts[id] = value.toString()
                    if (!rebuilding && p.optString("event") in setOf("input", "change")) {
                        inputEvent?.let { handler.removeCallbacks(it) }
                        inputEvent = Runnable { event(node, p.optString("event"), value.toString()) }
                        handler.post(inputEvent!!)
                    }
                }
            })
        }
    }

    private fun select(node: JSONObject, form: String, label: String): Spinner {
        val p = props(node); val id = p.optString("id"); val name = p.optString("name"); val options = children(node)
        activeDrafts.add(id)
        val values = options.map { props(it).optString("value") }
        selectValues[id] = values
        val chosen = drafts[id] ?: p.optString("value", p.optString("defaultValue"))
        return Spinner(activity).apply {
            tag = id; controls[id] = this; contentDescription = label; minimumHeight = dp(50)
            background = background(Color.WHITE, 13, Color.rgb(221, 227, 223)); setPadding(dp(8), 0, dp(8), 0)
            adapter = object : ArrayAdapter<String>(activity, android.R.layout.simple_spinner_item, options.map { allText(it) }) {
                init { setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item) }
                override fun getView(position: Int, convertView: View?, parent: ViewGroup): View = (super.getView(position, convertView, parent) as TextView).apply { typeface = font; textSize = 14f; setTextColor(ink); maxLines = 2 }
            }
            val selection = values.indexOf(chosen).coerceAtLeast(0)
            setSelection(selection, false)
            drafts[id] = values.getOrElse(selection) { "" }
            if (form.isNotBlank() && name.isNotBlank()) forms.getOrPut(form) { linkedMapOf() }[name] = { values.getOrElse(selectedItemPosition) { "" } }
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onNothingSelected(parent: AdapterView<*>?) {}
                override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, rowId: Long) {
                    drafts[id] = values.getOrElse(position) { "" }
                    if (!rebuilding && p.optString("event") == "change") event(node, "change", drafts[id])
                }
            }
        }
    }

    private fun event(node: JSONObject, kind: String, value: String? = null) {
        val p = props(node)
        if (p.optString("event") != kind && p.optJSONObject("events")?.has(kind) != true) return
        val event = JSONObject().put("kind", kind)
        if (p.has("id")) event.put("id", p.getString("id"))
        if (p.has("name")) event.put("name", p.getString("name"))
        if (value != null || p.has("value")) event.put("value", value ?: p.getString("value"))
        when (val descriptor = p.optJSONObject("events")?.opt(kind)) {
            is String -> event.put("id", descriptor)
            is JSONObject -> descriptor.keys().forEach { event.put(it, descriptor.get(it)) }
        }
        send(event)
    }

    private fun text(value: String, tag: String, classes: Set<String>, dark: Boolean): TextView = TextView(activity).apply {
        text = value; setTextColor(if (dark) Color.WHITE else ink)
        textSize = when { tag == "h1" -> 30f; tag == "h2" -> 22f; tag == "h3" -> 17f; tag in setOf("small", "dt") -> 11f; else -> 14f }
        val bold = tag in setOf("h1", "h2", "h3", "strong", "dd") || classes.any { it in setOf("eyebrow", "field-label", "account-owner", "nav-section-label") }
        typeface = Typeface.create(font, if (bold) Typeface.BOLD else Typeface.NORMAL)
        includeFontPadding = false
        setLineSpacing(dp(3).toFloat(), 1f)
        if (tag in setOf("small", "p", "em", "dt")) setTextColor(if (dark) Color.rgb(173, 190, 183) else muted)
        if (classes.any { it in setOf("eyebrow", "nav-section-label") }) { textSize = 10f; letterSpacing = .14f; setTextColor(if ("eyebrow" in classes) Color.rgb(217, 77, 49) else if (dark) mint else muted) }
        if (classes.contains("account-balance")) textSize = 28f
        if (classes.contains("movement-amount")) {
            textSize = 13f; setTextColor(if (classes.contains("refused")) Color.rgb(200, 56, 82) else if (classes.contains("credit")) green else ink)
            if (classes.contains("refused")) paintFlags = paintFlags or Paint.STRIKE_THRU_TEXT_FLAG
        }
        if (classes.any { it in setOf("account-mark", "brand-mark", "stat-icon", "movement-icon", "form-icon", "tip-icon", "security-icon", "avatar", "toast-icon") }) {
            gravity = Gravity.CENTER; minWidth = dp(38); minHeight = dp(38); setPadding(dp(10), dp(10), dp(10), dp(10))
            background = background(if (classes.contains("brand-mark")) coral else if (dark) Color.rgb(30, 70, 65) else Color.rgb(229, 248, 239), 12)
            setTextColor(if (dark) mint else green)
            if (classes.contains("account-mark") && classes.contains("violet")) { background = background(Color.rgb(240, 237, 255), 12); setTextColor(Color.rgb(101, 86, 174)) }
            if (classes.contains("movement-icon") && classes.contains("debit")) { background = background(Color.rgb(255, 247, 223), 12); setTextColor(Color.rgb(150, 112, 30)) }
            if (classes.contains("movement-icon") && classes.contains("refused")) { background = background(Color.rgb(255, 240, 242), 12); setTextColor(Color.rgb(200, 56, 82)) }
        }
        if (classes.contains("status-pill")) { textSize = 11f; background = background(Color.rgb(229, 248, 239), 20); setTextColor(green); setPadding(dp(9), dp(7), dp(9), dp(7)) }
        if (classes.contains("account-more")) { gravity = Gravity.END; setTextColor(muted) }
        if (tag.startsWith("h") && tag.length == 2 && android.os.Build.VERSION.SDK_INT >= 28) setAccessibilityHeading(true)
    }

    private fun button(title: String, classes: String, dark: Boolean): Button = Button(activity).apply {
        val c = classes.split(' ').toSet()
        text = title; isAllCaps = false; typeface = Typeface.create(font, Typeface.BOLD); textSize = 13f
        minWidth = 0; minimumWidth = 0; minHeight = dp(44); minimumHeight = dp(44)
        gravity = if (c.contains("nav-item")) Gravity.CENTER_VERTICAL or Gravity.START else Gravity.CENTER
        val fill = when { c.contains("primary") -> coral; c.contains("nav-item") && c.contains("active") -> Color.rgb(36, 70, 65); c.contains("active") -> ink; c.contains("secondary") -> Color.WHITE; c.contains("ghost-on-dark") -> Color.rgb(33, 62, 61); else -> Color.TRANSPARENT }
        setTextColor(when { c.contains("primary") || c.contains("active") || dark || c.contains("ghost-on-dark") -> Color.WHITE; c.contains("text-button") -> green; else -> ink })
        background = background(fill, 12, if (c.contains("secondary")) line else Color.TRANSPARENT)
        setPadding(dp(if (c.contains("icon-button")) 10 else 12), dp(8), dp(if (c.contains("icon-button")) 10 else 12), dp(8))
        if (c.contains("icon-button")) { textSize = 21f; minWidth = dp(42) }
        if (c.contains("chip")) { textSize = 12f; setPadding(dp(9), dp(9), dp(9), dp(9)) }
    }

    private fun styleBox(box: LinearLayout, c: Set<String>, dark: Boolean) {
        val pad = when {
            c.contains("content") -> 20
            c.contains("sidebar") -> 22
            c.contains("topbar") -> 16
            c.any { it in setOf("hero-card", "security-hero") } -> 25
            c.any { it in setOf("card", "stat-card", "account-card", "mini-stat", "modal-form") } -> 22
            c.contains("toast") -> 15
            c.contains("segmented") -> 4
            c.any { it in setOf("modal-reassurance", "architecture-layer", "sidebar-profile") } -> 13
            else -> 0
        }
        box.setPadding(dp(pad), dp(pad), dp(pad), dp(pad))
        when {
            c.contains("toast") && c.contains("success") -> box.background = background(Color.rgb(21, 93, 72), 18)
            c.contains("toast") && c.contains("error") -> box.background = background(Color.rgb(136, 39, 59), 18)
            c.any { it in setOf("hero-card", "guidance-card", "security-hero", "toast") } -> box.background = GradientDrawable(GradientDrawable.Orientation.TL_BR, intArrayOf(Color.rgb(16, 46, 48), Color.rgb(7, 26, 28))).apply { cornerRadius = dp(if (c.contains("toast")) 18 else 25).toFloat() }
            c.contains("sidebar") -> box.setBackgroundColor(ink)
            c.contains("topbar") -> box.setBackgroundColor(paper)
            c.contains("account-card") -> {
                val tint = if (c.contains("selected")) Color.rgb(229, 248, 239) else Color.WHITE
                box.background = GradientDrawable(GradientDrawable.Orientation.TL_BR, intArrayOf(Color.WHITE, tint)).apply { cornerRadius = dp(20).toFloat(); setStroke(dp(if (c.contains("selected")) 2 else 1), if (c.contains("selected")) green else line) }
                box.minimumHeight = dp(164)
            }
            c.any { it in setOf("card", "stat-card", "mini-stat") } -> box.background = background(Color.WHITE, 20, line)
            c.any { it in setOf("modal-reassurance", "architecture-layer", "segmented", "detail-balance") } -> box.background = background(if (dark) Color.rgb(25, 56, 52) else Color.rgb(242, 244, 240), 12)
        }
        if (c.contains("hero-copy")) {
            for (index in 0 until box.childCount) (box.getChildAt(index) as? TextView)?.let { if (it.textSize / activity.resources.displayMetrics.scaledDensity >= 29f) { it.textSize = 43f; it.setTextColor(Color.WHITE) } }
        }
        if (c.contains("topbar-title")) {
            (box.getChildAt(0) as? TextView)?.apply { textSize = 8f; maxLines = 1 }
            (box.getChildAt(1) as? TextView)?.apply { textSize = 17f }
        }
        if (c.contains("brand-mark")) {
            box.background = background(coral, 13); box.gravity = Gravity.CENTER
            box.minimumWidth = dp(42); box.minimumHeight = dp(42)
            (box.getChildAt(0) as? TextView)?.apply { textSize = 24f; typeface = Typeface.create(font, Typeface.BOLD) }
        }
        if (c.contains("brand-copy")) (box.getChildAt(0) as? TextView)?.textSize = 23f
        if (c.contains("stat-copy")) (box.getChildAt(1) as? TextView)?.textSize = 23f
        if (c.contains("detail-balance")) { box.setPadding(dp(20), dp(24), dp(20), dp(24)); (box.getChildAt(1) as? TextView)?.textSize = 36f }
        if (c.contains("movement-row") || c.contains("security-control")) { box.setPadding(0, dp(14), 0, dp(14)); box.minimumHeight = dp(76) }
        if (c.contains("movement-meta")) box.gravity = Gravity.END
        if (c.contains("main-nav")) box.setPadding(0, dp(28), 0, dp(28))
        if (c.contains("empty-state")) { box.gravity = Gravity.CENTER; box.minimumHeight = dp(250); box.setPadding(dp(16), dp(28), dp(16), dp(28)) }
    }

    private fun decoration(security: Boolean): View = object : View(activity) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        init { minimumHeight = dp(if (security) 165 else 160); importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO }
        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            val x = width / 2f; val y = height / 2f
            paint.style = Paint.Style.STROKE; paint.strokeWidth = dp(1).toFloat(); paint.color = 0x359ee2c3
            canvas.drawCircle(x, y, dp(70).toFloat(), paint)
            canvas.drawCircle(x, y, dp(94).toFloat(), paint)
            paint.style = Paint.Style.FILL; paint.color = if (security) coral else 0x269ee2c3
            canvas.drawRoundRect(x - dp(32), y - dp(32), x + dp(32), y + dp(32), dp(20).toFloat(), dp(20).toFloat(), paint)
            paint.color = if (security) Color.WHITE else 0x45ffffff; paint.textSize = dp(32).toFloat(); paint.typeface = Typeface.create(font, Typeface.BOLD); paint.textAlign = Paint.Align.CENTER
            canvas.drawText("T", x, y + dp(11), paint)
        }
    }

    private fun background(color: Int, radius: Int, stroke: Int = Color.TRANSPARENT) = GradientDrawable().apply { setColor(color); cornerRadius = dp(radius).toFloat(); if (stroke != Color.TRANSPARENT) setStroke(dp(1), stroke) }
    private fun dp(value: Int) = (value * activity.resources.displayMetrics.density).toInt()
    companion object {
        private val horizontalClasses = setOf("topbar", "topbar-actions", "brand", "brand-mark", "hero-actions", "stat-card", "account-card-top", "movement-row", "mini-stat", "form-intro", "money-input", "segmented", "filter-row", "detail-actions", "modal-heading", "toast", "modal-reassurance", "shield-seal", "security-control", "architecture-layer", "sidebar-profile", "secure-chip")
        private val flexibleClasses = setOf("topbar-title", "brand-copy", "stat-copy", "movement-copy", "toast-copy", "security-copy", "field-control", "account-more")
    }
}
