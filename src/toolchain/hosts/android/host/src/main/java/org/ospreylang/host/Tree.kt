package org.ospreylang.host

// Implements [NATIVE-SHM-NODES] on the host: the component tree the
// application told, read from the checked rows of the shared frame, with the
// same member names the SwiftUI host's Tree.swift reads. A `field` row is one
// typed value, a `record` row a component or one of its members and a `list`
// row the items under one name. Nothing here is parsed from text: every value
// is a row the frame check already accepted.

/** One told value: text, a number, a flag, a list of items or a record. */
sealed class Tree {
    data object Null : Tree()
    data class Bit(val value: Boolean) : Tree()
    data class Num(val value: Double) : Tree()
    data class Str(val value: String) : Tree()
    data class Arr(val items: List<Tree>) : Tree()
    data class Obj(val members: Map<String, Tree>) : Tree()

    val string get() = (this as? Str)?.value
    val bit get() = (this as? Bit)?.value
    val int: Int? get() = (this as? Num)?.value?.takeIf { it.isFinite() && kotlin.math.abs(it) < 9e15 }?.toInt()
    val array get() = (this as? Arr)?.items
    val objectMembers get() = (this as? Obj)?.members

    operator fun get(key: String): Tree? = objectMembers?.get(key)
    fun text(key: String) = this[key]?.string ?: ""
    fun flag(key: String) = this[key]?.bit ?: false
    fun count(key: String) = this[key]?.int ?: 0
    fun real(key: String) = (this[key] as? Num)?.value ?: 0.0
    fun list(key: String) = this[key]?.array ?: emptyList()

    /** The words a control event carries for this value: text, a flag or a whole number. */
    val words: String
        get() = when (this) {
            is Str -> value
            is Bit -> if (value) "true" else "false"
            is Num -> if (value == kotlin.math.floor(value) && kotlin.math.abs(value) < 9e15)
                value.toInt().toString() else value.toString()
            else -> ""
        }
}

/** One colour role, `#RRGGBB` in each appearance ([UI-THEME]). */
data class ThemeToken(val name: String, val light: String, val dark: String)

/** The application's theme: colour tokens, a spacing scale and motion durations. */
data class Theme(val tokens: List<ThemeToken>, val spacing: List<Int>,
                 val feedback: Int, val settle: Int) {
    companion object {
        /** The theme a told record names; null when it is no record. */
        fun of(told: Tree?): Theme? {
            val members = told?.objectMembers ?: return null
            return Theme(
                told!!.list("tokens").map { ThemeToken(it.text("name"), it.text("light"), it.text("dark")) },
                told.list("spacing").mapNotNull { it.int },
                told.count("feedback"), told.count("settle"))
        }
    }
}

/** The frame a host draws: the theme and every region's component tree, by key. */
data class ToldFrame(val theme: Theme?, val regions: List<Pair<String, Tree>>)

/** One region the application told: its key, its revision and its tree. */
class Region(val key: String, var revision: Int, var node: Tree)

/** Reads the told rows of a checked frame into trees. */
object Told {
    /** The value of one row and the rows inside it. */
    fun tree(shown: Shown, index: Int): Tree {
        val row = shown.controls[index]
        return when (row.kind) {
            Kind.FIELD -> field(row)
            Kind.LIST -> Tree.Arr(row.children.map { tree(shown, it) })
            else -> record(shown, row, index)
        }
    }

    /** One told value; its weight says which of the five kinds it is. */
    private fun field(row: RowControl): Tree = when (row.look.weight) {
        1L -> Tree.Num(row.value.toDouble())
        2L -> Tree.Num(row.look.size)
        3L -> Tree.Bit(row.value != 0L)
        4L -> Tree.Num(-row.look.size)
        else -> Tree.Str(row.words)
    }

    /** A record: the rows that name it as their parent are its members. */
    private fun record(shown: Shown, row: RowControl, index: Int): Tree {
        val members = linkedMapOf<String, Tree>()
        for (child in row.children) {
            val name = shown.controls[child].hint
            if (name.isNotEmpty()) members[name] = tree(shown, child)
        }
        // A record a scene names carries its row as `sceneRow`, so the canvas finds the scene.
        if (shown.scenes.containsKey(index)) members["sceneRow"] = Tree.Num(index.toDouble())
        return Tree.Obj(members)
    }

    /** The frame's theme and regions, or null when the frame tells no
     * components: a frame of plain view rows, which the row view draws. */
    fun frame(shown: Shown): ToldFrame? {
        val told = shown.roots.filter { shown.controls[it].kind == Kind.RECORD }
        if (told.isEmpty()) return null
        var theme: Theme? = null
        val regions = mutableListOf<Pair<String, Tree>>()
        for (root in told) {
            val value = tree(shown, root)
            when (shown.controls[root].hint) {
                "theme" -> theme = Theme.of(value)
                "region" -> regions.add(value.text("key") to (value["node"] ?: Tree.Null))
            }
        }
        return ToldFrame(theme, regions)
    }
}
