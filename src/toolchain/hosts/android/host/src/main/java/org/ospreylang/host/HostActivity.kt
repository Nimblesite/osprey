package org.ospreylang.host

import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.io.File

// Implements [NATIVE-HOSTS-CONTRACT] and [NATIVE-SHM-NODES]: the entry point.
// It opens the application's library over the shared frame and shows the root
// region with this host's own views, redrawing after every event the
// application answered. No application state, screen or policy lives here.

class HostActivity : Activity() {
    private lateinit var store: Store
    private lateinit var root: LinearLayout
    private lateinit var page: LinearLayout
    private lateinit var strip: TextView
    private lateinit var plain: PlainRenderer
    private lateinit var told: ToldRenderer
    private var choosing: Pair<Ask, (Outcome) -> Unit>? = null
    private var toldPage = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        store = Store(adapters())
        if (intent.getBooleanExtra("view_check", false)) {
            ViewCheck.run(this, store)
            return
        }
        buildPage()
        store.onFrame = { redraw() }
        store.start(arguments(), dark(), "android", filesDir.path,
                    java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000L)
    }

    /** The host's page: the application's view over a diagnostic strip. */
    private fun buildPage() {
        page = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        strip = TextView(this).apply {
            setPadding(24, 12, 24, 12)
            textSize = 12f
        }
        root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(ScrollView(context).apply {
                isFillViewport = true
                addView(page)
            }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))
            addView(strip)
        }
        setContentView(root)
        plain = PlainRenderer(this, store)
        told = ToldRenderer(this, store)
    }

    /** The frame the application last drew, shown again: a told region keeps
     * its views so fields hold their caret and focus across redraws. */
    private fun redraw() {
        told.dark = dark()
        if (store.told) {
            dressToldPage()
            told.render(page)
        } else {
            dressPlainPage()
            plain.render(page)
        }
        strip.text = store.failure ?: store.diagnostics.lastOrNull() ?: ""
        strip.visibility = if (strip.text.isEmpty()) TextView.GONE else TextView.VISIBLE
        strip.setTextColor(told.theme().secondary())
        strip.setBackgroundColor(told.theme().overlay(0.05))
    }

    /** A told page wears the theme's canvas, centred as SwiftUI places a short
     * region, with the system bars the same canvas and contrasting icons. */
    private fun dressToldPage() {
        if (!toldPage) {
            page.removeAllViews()
            toldPage = true
        }
        val canvas = told.theme().canvas(this)
        page.setBackgroundColor(canvas)
        root.setBackgroundColor(canvas)
        page.gravity = Gravity.CENTER_VERTICAL
        window.statusBarColor = canvas
        window.navigationBarColor = canvas
        val light = if (dark()) 0 else android.view.View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
        if (Build.VERSION.SDK_INT >= 30) {
            window.insetsController?.setSystemBarsAppearance(
                if (dark()) 0 else android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS,
                android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS)
        } else window.decorView.systemUiVisibility = light
    }

    /** A plain page keeps the window the platform gave it, drawn from the top. */
    private fun dressPlainPage() {
        if (toldPage) {
            page.removeAllViews()
            told.clear()
            toldPage = false
        }
        page.background = null
        page.gravity = Gravity.NO_GRAVITY
    }

    private fun dark(): Boolean =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    /** The launch arguments, as the application gathers them in order. */
    private fun arguments(): List<String> = intent.getStringArrayListExtra("osprey_arguments") ?: emptyList()

    override fun onConfigurationChanged(configuration: Configuration) {
        super.onConfigurationChanged(configuration)
        store.appear(dark())
    }

    /** The asks this host answers; the rest complete with a typed refusal. */
    private fun adapters(): List<Adapter> = listOf(
        StoreAdapter(File(filesDir, "store")),
        HttpAdapter(),
        HostCallAdapter(applicationContext),
        ReadTextsAdapter(File(filesDir, "imports")),
        MeasureAdapter(),
        ChooseAdapter(this::chooseFile))

    /** Opens the Storage Access Framework for one choose-file ask. */
    private fun chooseFile(ask: Ask, done: (Outcome) -> Unit) {
        runOnUiThread { present(ask, done) }
    }

    private fun present(ask: Ask, done: (Outcome) -> Unit) {
        if (choosing != null) {
            done(Outcome.Refused("A file picker is already open"))
            return
        }
        choosing = ask to done
        val wanted = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = mimeOf(ask.texts(1).firstOrNull() ?: "")
            putExtra(Intent.EXTRA_LOCAL_ONLY, true)
        }
        startActivityForResult(wanted, PICK)
    }

    /** A chooser's type name: a suffix the platform knows, or anything at all. */
    private fun mimeOf(named: String): String =
        android.webkit.MimeTypeMap.getSingleton()
            .getMimeTypeFromExtension(named.removePrefix(".").lowercase()) ?: "*/*"

    /** The chosen document, copied into the host's import folder and answered
     * with the copy's bare path; cancelling completes the ask as cancelled. */
    override fun onActivityResult(request: Int, result: Int, data: Intent?) {
        super.onActivityResult(request, result, data)
        if (request != PICK) return
        val held = choosing ?: return
        choosing = null
        val uri = data?.data
        if (result != RESULT_OK || uri == null) {
            held.second(Outcome.Refused("Cancelled"))
            return
        }
        Thread {
            val outcome = imported(uri)
            runOnUiThread { held.second(outcome) }
        }.start()
    }

    private fun imported(uri: Uri): Outcome = try {
        val folder = File(filesDir, "imports/${java.util.UUID.randomUUID()}")
        folder.mkdirs()
        // The provider names the document; only its last path segment is kept, so it stays in the folder.
        val name = File(queryName(uri)).name.takeUnless { it.isEmpty() || it == "." || it == ".." } ?: "document"
        val target = File(folder, name)
        contentResolver.openInputStream(uri)?.use { input ->
            target.outputStream().use { output -> input.copyTo(output) }
        } ?: throw IllegalStateException("The document could not be opened")
        Outcome.Bare(target.absolutePath)
    } catch (error: Exception) {
        Outcome.Refused(error.message ?: "The document could not be imported")
    }

    private fun queryName(uri: Uri): String =
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val at = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (at >= 0 && cursor.moveToFirst()) cursor.getString(at) else ""
        } ?: ""

    override fun onDestroy() {
        store.close()
        super.onDestroy()
    }

    companion object {
        private const val PICK = 4201
    }
}
