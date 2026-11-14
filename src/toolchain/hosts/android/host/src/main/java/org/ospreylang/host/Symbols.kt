package org.ospreylang.host

import android.content.Context
import android.graphics.drawable.Drawable

// Implements [UI-COMPONENTS-SYMBOLS] for the Android host: the framework's
// icon names as this host's own monochrome vector glyphs, tinted with the ink
// of the words they sit beside. A name this host has no glyph for draws none
// and the caller says so; symbols are never colour emoji.

object Symbols {
    /** Every symbol name src/osprey/packages/native/src/symbol.ospml spells, as this host's own glyph. */
    private val glyphs = mapOf(
        "add" to R.drawable.sym_add, "delete" to R.drawable.sym_delete, "dismiss" to R.drawable.sym_dismiss,
        "check" to R.drawable.sym_check, "warning" to R.drawable.sym_warning, "info" to R.drawable.sym_info,
        "question" to R.drawable.sym_question, "search" to R.drawable.sym_search, "settings" to R.drawable.sym_settings,
        "star" to R.drawable.sym_star, "play" to R.drawable.sym_play, "pause" to R.drawable.sym_pause,
        "stop" to R.drawable.sym_stop, "refresh" to R.drawable.sym_refresh, "folder" to R.drawable.sym_folder,
        "document" to R.drawable.sym_document, "package" to R.drawable.sym_package, "inbox" to R.drawable.sym_inbox,
        "globe" to R.drawable.sym_globe, "external" to R.drawable.sym_external, "lock" to R.drawable.sym_lock,
        "shield" to R.drawable.sym_shield, "branch" to R.drawable.sym_branch, "tag" to R.drawable.sym_tag,
        "clock" to R.drawable.sym_clock, "person" to R.drawable.sym_person, "cursor" to R.drawable.sym_cursor,
        "chevron" to R.drawable.sym_chevron, "send" to R.drawable.sym_send, "terminal" to R.drawable.sym_terminal,
        "edit" to R.drawable.sym_edit, "panel" to R.drawable.sym_panel, "sparkle" to R.drawable.sym_sparkle)

    /** Whether this host has a glyph for `name`. */
    fun known(name: String): Boolean = glyphs.containsKey(name)

    /** One monochrome vector glyph `size` points square, tinted `tint`; null when this host has none. */
    fun drawable(context: Context, name: String, tint: Int, size: Int): Drawable? {
        val held = glyphs[name] ?: return null
        val glyph = context.resources.getDrawable(held, context.theme).mutate()
        glyph.setTint(tint)
        val square = dp(context, size.toDouble())
        glyph.setBounds(0, 0, square, square)
        return glyph
    }
}
