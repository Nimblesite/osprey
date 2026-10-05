package org.ospreylang.talon

import android.content.Intent
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import android.graphics.Rect
import android.test.InstrumentationTestCase
import android.test.InstrumentationTestRunner
import android.view.View
import android.view.MotionEvent
import android.view.ViewGroup
import android.widget.EditText
import android.widget.Spinner
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** These tests drive actual widgets, JNI, shared Osprey effects and live SQLite. */
@Suppress("DEPRECATION")
class BankInstrumentedTest : InstrumentationTestCase() {
    private lateinit var app: MainActivity
    private val server: String get() = (instrumentation as InstrumentationTestRunner).arguments.getString("server_url") ?: BankHost.DEFAULT_URL

    override fun setUp() {
        super.setUp()
        app = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("server_url", server)) as MainActivity
        await("initial live data") { ::app.isInitialized && app.host.state.optString("loading") == "false" && app.host.pending == 0 }
        assertEquals("", ui { app.host.state.optString("noticeTone") })
    }

    override fun tearDown() {
        if (::app.isInitialized) ui { app.finish() }
        instrumentation.waitForIdleSync()
        super.tearDown()
    }

    fun testBankingWorkflowsUseNativeWidgetsAndLiveLedger() {
        assertTrue(ui { app.renderer.root is android.widget.FrameLayout })
        assertTrue(ui { views(app.renderer.root).none { it.javaClass.name.contains("WebView") } })
        assertTrue(ui { textContains("MANAGED PORTFOLIO") })
        screenshot("overview")
        val marker = "Android ${System.nanoTime()}"
        val from = openAccount("$marker 李 👩🏽‍💻")
        val to = openAccount("$marker destination")
        navigate("accounts")
        click("account-$from")
        assertEquals(from, ui { app.host.state.optString("selected") })
        assertTrue(ui { textContains("$marker 李 👩🏽‍💻") })
        screenshot("accounts")

        click("quick-deposit")
        assertTrue(ui { app.renderer.control("deposit-account") is Spinner })
        set("deposit-amount", "100.25")
        set("deposit-note", "$marker initial 💸")
        submit("submit-deposit")
        complete("Deposit complete")
        assertEquals(10025, account(from).getInt("cents"))
        screenshot("deposit")

        click("move-withdraw")
        set("withdraw-amount", "10.05")
        set("withdraw-note", "$marker withdrawal")
        submit("submit-withdraw")
        complete("Withdrawal complete")
        assertEquals(9020, account(from).getInt("cents"))

        click("move-transfer")
        select("transfer-to", to)
        set("transfer-amount", "20.10")
        set("transfer-note", "$marker transfer")
        submit("submit-transfer")
        complete("Transfer complete")
        assertEquals(7010, account(from).getInt("cents"))
        assertEquals(2010, account(to).getInt("cents"))

        click("move-withdraw")
        set("withdraw-amount", "99999.99")
        set("withdraw-note", "$marker refused")
        submit("submit-withdraw")
        complete("Operation refused")
        assertEquals(7010, account(from).getInt("cents"))
        assertEquals("error", ui { app.host.state.optString("noticeTone") })
        screenshot("refusal")

        navigate("activity")
        set("activity-search", marker)
        await("reactive search") { app.host.state.optString("search") == marker }
        click("filter-refused")
        assertTrue(ui { textContains("$marker refused") })
        assertFalse(ui { textContains("$marker withdrawal") })
        screenshot("activity")
        click("filter-in")
        assertTrue(ui { textContains("$marker initial 💸") })
        click("filter-out")
        assertTrue(ui { textContains("$marker withdrawal") })
        click("filter-all")
        set("activity-search", "definitely no matching journal ${System.nanoTime()}")
        await("empty search") { textContains("No matching activity") }
        ui { app.onBackPressed() }
        assertEquals("move", ui { app.host.state.optString("route") })
        navigate("security")
        screenshot("security")
        assertTrue(ui { textContains("Atomic") || textContains("Protected") || textContains("protected") })
    }

    fun testValidationModalAndDuplicateSubmissionProtection() {
        val before = accounts().length()
        click("topbar-open")
        await("owner focus command") { app.renderer.control("new-owner")?.hasFocus() == true }
        submit("submit-open-account")
        assertEquals("Owner name required", ui { app.host.state.optString("noticeTitle") })
        assertEquals(before, accounts().length())
        set("new-owner", "Draft must survive")
        click("dismiss-notice")
        assertEquals("Draft must survive", ui { (app.renderer.control("new-owner") as EditText).text.toString() })
        screenshot("open-account")
        ui { app.onBackPressed() }
        assertEquals("closed", ui { app.host.state.optString("modal") })

        val marker = "Single ${System.nanoTime()}"
        click("topbar-open"); set("new-owner", marker)
        ui {
            val button = findTag(app.renderer.root, "submit-open-account-button")!!
            button.performClick()
            // A queued second tap must not enqueue a second bank write.
            findTag(app.renderer.root, "submit-open-account-button")?.performClick()
        }
        complete("Account opened")
        assertEquals(before + 1, accounts().length())

        navigate("move"); click("move-deposit")
        set("deposit-amount", "0"); submit("submit-deposit")
        assertEquals("Check the deposit", ui { app.host.state.optString("noticeTitle") })
        set("deposit-amount", "1.001"); submit("submit-deposit")
        assertEquals("Check the deposit", ui { app.host.state.optString("noticeTitle") })
        click("move-transfer")
        val first = accounts().getJSONObject(0).getString("id")
        select("transfer-from", first); select("transfer-to", first)
        set("transfer-amount", "1"); submit("submit-transfer")
        assertEquals("Choose two accounts", ui { app.host.state.optString("noticeTitle") })
    }

    fun testRotationRetainsDraftSelectionFocusAndModel() {
        navigate("move"); click("move-transfer")
        set("transfer-amount", "27.42")
        set("transfer-note", "Rotation café 🦅")
        val accountId = accounts().getJSONObject(accounts().length() - 1).getString("id")
        select("transfer-to", accountId)
        click("topbar-refresh")
        await("refresh restores unfinished form") { app.host.state.optString("loading") == "false" && app.host.pending == 0 }
        assertEquals("27.42", ui { (app.renderer.control("transfer-amount") as EditText).text.toString() })
        assertEquals("Rotation café 🦅", ui { (app.renderer.control("transfer-note") as EditText).text.toString() })
        assertTrue(ui { (app.renderer.control("transfer-to") as Spinner).selectedItem.toString().contains("#${accountId}") })
        val oldHost = ui { app.host }
        val monitor = instrumentation.addMonitor(MainActivity::class.java.name, null, false)
        ui { (app.renderer.control("transfer-note") as EditText).requestFocus(); app.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE }
        val replacement = instrumentation.waitForMonitorWithTimeout(monitor, 10000)
        instrumentation.removeMonitor(monitor)
        assertNotNull("Rotation must recreate the Android activity", replacement)
        app = replacement as MainActivity
        instrumentation.waitForIdleSync()
        assertSame(oldHost, ui { app.host })
        assertEquals("move", ui { app.host.state.optString("route") })
        assertEquals("27.42", ui { (app.renderer.control("transfer-amount") as EditText).text.toString() })
        assertEquals("Rotation café 🦅", ui { (app.renderer.control("transfer-note") as EditText).text.toString() })
        await("restored input focus") { app.renderer.control("transfer-note")?.hasFocus() == true }
        assertTrue(ui { (app.renderer.control("transfer-to") as Spinner).selectedItem.toString().contains("#${accountId}") })
        screenshot("landscape")
    }

    fun testNativeTouchTargetsAndAccessibilityLabels() {
        tap("topbar-open")
        await("touch opened modal") { app.host.state.optString("modal") == "open" }
        assertEquals("Account owner", ui { app.renderer.control("new-owner")!!.contentDescription.toString() })
        assertEquals("Close", ui { app.renderer.control("close-modal")!!.contentDescription.toString() })
        tap("close-modal")
        await("touch closed modal") { app.host.state.optString("modal") == "closed" }
        tap("toggle-menu")
        await("touch opened navigation") { app.host.state.optString("menu") == "open" }
        tap("nav-accounts")
        assertEquals("accounts", ui { app.host.state.optString("route") })
        assertEquals("closed", ui { app.host.state.optString("menu") })
        tap("topbar-activity")
        assertEquals("activity", ui { app.host.state.optString("route") })
    }

    fun testOfflineStartupAndRefreshRecoverWithoutReplayingWrites() {
        ui { app.finish() }
        app = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("server_url", "http://127.0.0.1:1")) as MainActivity
        await("offline response", 20000) { app.host.state.optString("loading") == "false" && app.host.pending == 0 }
        assertEquals("Bank data unavailable", ui { app.host.state.optString("noticeTitle") })
        assertTrue(ui { textContains("No accounts yet") })
        click("topbar-refresh")
        await("offline refresh", 20000) { app.host.state.optString("loading") == "false" && app.host.pending == 0 }
        assertTrue(ui { app.renderer.control("topbar-open")?.isEnabled == true })
        screenshot("offline")
        ui { app.finish() }
        app = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("server_url", server)) as MainActivity
        await("online recovery") { app.host.state.optString("loading") == "false" && app.host.pending == 0 }
        assertEquals("", ui { app.host.state.optString("noticeTone") })
        assertEquals(accounts().length(), ui { JSONArray(app.host.state.getString("accounts")).length() })
    }

    fun testSavedPendingMutationRefreshesWithoutReplay() {
        val beforeAccounts = fetch("/api/accounts")
        val beforeJournal = fetch("/api/activity")
        val saved = ui { JSONObject(app.host.model).put("route", "move").put("moveMode", "transfer").put("busy", "true").put("search", "Saved café 🦅").toString() }
        val restored = ui { BankHost(server, saved).apply { attach({}, {}, { throw it }); startCommands(true) } }
        try {
            await("restored pending write") { restored.pending == 0 && restored.state.optString("loading") == "false" }
            assertEquals("false", ui { restored.state.optString("busy") })
            assertEquals("move", ui { restored.state.optString("route") })
            assertEquals("transfer", ui { restored.state.optString("moveMode") })
            assertEquals("Saved café 🦅", ui { restored.state.optString("search") })
            assertEquals(beforeAccounts, fetch("/api/accounts"))
            assertEquals(beforeJournal, fetch("/api/activity"))
        } finally { ui { restored.close() } }
    }

    private fun openAccount(owner: String): String {
        click("topbar-open"); set("new-owner", owner); submit("submit-open-account"); complete("Account opened")
        val rows = accounts()
        return (0 until rows.length()).map { rows.getJSONObject(it) }.first { it.getString("owner") == owner }.getString("id")
    }
    private fun complete(title: String) = await(title) { app.host.state.optString("noticeTitle") == title && app.host.pending == 0 && app.host.state.optString("busy") != "true" }
    private fun navigate(route: String) { if (ui { app.renderer.control("nav-$route") } == null) click("toggle-menu"); click("nav-$route"); assertEquals(route, ui { app.host.state.optString("route") }) }
    private fun click(id: String) = ui { val view = app.renderer.control(id) ?: fail("No widget $id"); assertTrue("Disabled widget $id", (view as View).isEnabled); assertTrue("No click handler $id", view.performClick()) }
    private fun set(id: String, value: String) = ui { val input = app.renderer.control(id) as? EditText ?: error("No input $id"); input.requestFocus(); input.setText(value); input.setSelection(input.length()) }
    private fun select(id: String, account: String) = ui { val spinner = app.renderer.control(id) as Spinner; val index = (0 until spinner.count).first { spinner.getItemAtPosition(it).toString().contains("#$account") }; spinner.setSelection(index) }
    private fun submit(id: String) = ui { val button = findTag(app.renderer.root, "$id-button") ?: error("No submit for $id"); assertTrue(button.performClick()) }
    private fun tap(id: String) {
        ui { val view = app.renderer.control(id) ?: error("No native touch target $id"); view.requestRectangleOnScreen(Rect(0, 0, view.width, view.height), true) }
        settleFrames()
        val point = ui { val view = app.renderer.control(id)!!; val location = IntArray(2); view.getLocationOnScreen(location); assertTrue("Touch target width $id", view.width > 0); assertTrue("Touch target height $id", view.height > 0); Pair(location[0] + view.width / 2f, location[1] + view.height / 2f) }
        val time = android.os.SystemClock.uptimeMillis()
        val down = MotionEvent.obtain(time, time, MotionEvent.ACTION_DOWN, point.first, point.second, 0)
        val up = MotionEvent.obtain(time, time + 80, MotionEvent.ACTION_UP, point.first, point.second, 0)
        instrumentation.sendPointerSync(down); instrumentation.sendPointerSync(up)
        down.recycle(); up.recycle(); settleFrames()
    }
    private fun accounts(): JSONArray = JSONArray(fetch("/api/accounts"))
    private fun account(id: String): JSONObject { val values = accounts(); return (0 until values.length()).map { values.getJSONObject(it) }.first { it.getString("id") == id } }
    private fun fetch(path: String): String { val request = URL(server + path).openConnection() as HttpURLConnection; request.connectTimeout = 10000; request.readTimeout = 10000; return try { request.inputStream.bufferedReader().use { it.readText() } } finally { request.disconnect() } }
    private fun textContains(value: String): Boolean = views(app.renderer.root).filterIsInstance<TextView>().any { it.text.contains(value) }
    private fun views(root: View): List<View> = listOf(root) + if (root is ViewGroup) (0 until root.childCount).flatMap { views(root.getChildAt(it)) } else emptyList()
    private fun findTag(root: View, tag: String) = views(root).firstOrNull { it.tag == tag }
    private fun screenshot(name: String) {
        settleFrames()
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
        val colors = mutableSetOf<Int>()
        for (y in bitmap.height / 8 until bitmap.height * 7 / 8 step maxOf(1, bitmap.height / 70)) {
            for (x in bitmap.width / 10 until bitmap.width * 9 / 10 step maxOf(1, bitmap.width / 50)) colors.add(bitmap.getPixel(x, y))
        }
        assertTrue("Screenshot $name captured a blank/uncommitted frame", colors.size > 12)
        val file = File(instrumentation.targetContext.filesDir, "screenshots/$name.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }
    private fun settleFrames() { instrumentation.waitForIdleSync(); val latch = java.util.concurrent.CountDownLatch(1); ui { app.renderer.root.postOnAnimation { app.renderer.root.postOnAnimation { latch.countDown() } } }; assertTrue("Native view did not draw", latch.await(5, java.util.concurrent.TimeUnit.SECONDS)); instrumentation.waitForIdleSync(); instrumentation.uiAutomation.waitForIdle(300, 5000) }
    private fun await(label: String, timeout: Long = 15000, predicate: () -> Boolean) { val end = System.currentTimeMillis() + timeout; while (System.currentTimeMillis() < end) { if (ui(predicate)) return; Thread.sleep(100) }; fail("Timed out: $label; model=" + ui { app.host.model }) }
    private fun <T> ui(action: () -> T): T { var result: T? = null; var failure: Throwable? = null; instrumentation.runOnMainSync { try { result = action() } catch (error: Throwable) { failure = error } }; failure?.let { throw it }; @Suppress("UNCHECKED_CAST") return result as T }
}
