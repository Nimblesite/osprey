package org.ospreylang.host

// Implements [UI-MACHINE-SELF-TEST] on Android: the steps of an application's self-test.json read
// against the told regions, the same reading the SwiftUI host's SelfTestRegions makes, so one
// script plays alike on every host. A step names only what a screen reader announces.

/** One step of a script: a verb and its words. */
data class JourneyStep(val verb: String, val words: List<String>) {
    companion object {
        /** `{"steps": [{"verb": "word"}, {"verb": ["word", "word"]}]}`. */
        fun parse(text: String): List<JourneyStep> {
            val steps = org.json.JSONObject(text).getJSONArray("steps")
            return (0 until steps.length()).map { index ->
                val step = steps.getJSONObject(index)
                require(step.length() == 1) { "A step is one verb: $step" }
                val verb = step.keys().next()
                val value = step.get(verb)
                val words = if (value is org.json.JSONArray) (0 until value.length()).map { value.getString(it) } else listOf(value.toString())
                JourneyStep(verb, words)
            }
        }
    }
}

/** What a step needs of the regions as they stand. */
sealed class Attempt {
    data class Waiting(val reason: String) : Attempt()
    /** The control event to deliver, or none when the step already holds. */
    data class Sending(val touch: Touch?) : Attempt()
    data class Recording(val name: String) : Attempt()
    data class Sleeping(val seconds: Double) : Attempt()
}

/** The regions as words and controls; a region no slot draws is not on show. */
class JourneyRegions(regions: Map<String, Tree>) {
    val nodes: List<Tree> = reached(regions)

    private fun reached(all: Map<String, Tree>): List<Tree> {
        val root = all["root"] ?: return all.keys.sorted().mapNotNull { all[it] }
        val keys = mutableListOf("root")
        fun follow(node: Tree) {
            val key = node.text("key")
            if (node.text("kind") == "slot" && all[key] != null && key !in keys) {
                keys.add(key)
                follow(all[key] ?: Tree.Null)
            }
            inside(node).forEach(::follow)
        }
        follow(root)
        return keys.sorted().mapNotNull { all[it] }
    }

    fun shows(words: String) = nodes.any { holds(it, words) }

    private fun control(wanted: (Tree) -> Boolean): Tree? =
        nodes.firstNotNullOfOrNull { find(it) { node -> node["event"]?.int != null && wanted(node) } }

    private fun named(label: String) = control { it.text("label") == label || it.text("title") == label }

    private fun invocable(words: String) =
        named(words) ?: control { it.text("label").contains(words) || it.text("title").contains(words) }

    private fun enabled(node: Tree) = node["enabled"]?.bit ?: true

    private fun send(node: Tree, words: String) = Attempt.Sending(Touch.control(node.count("event").toLong(), words))

    /** A choice of a tab bar, row list or picker; [picker], when given, names the one control to choose in. */
    private fun chooser(words: String, picker: String?): Attempt.Sending? {
        for (exact in listOf(true, false)) for (field in listOf("tabs", "rows", "choices")) {
            val wanted = { row: Tree ->
                holds(row.without("child"), words) && (!exact || row.text("label") == words ||
                    row.text("label").endsWith(" · $words") || row.text("key") == words)
            }
            val node = control { node -> (picker == null || node.text("label") == picker || node.text("title") == picker) &&
                node.list(field).any(wanted) } ?: continue
            return send(node, node.list(field).first(wanted).text("key"))
        }
        return null
    }

    fun attempt(step: JourneyStep): Attempt {
        fun word(index: Int) = step.words.getOrNull(index) ?: throw IllegalArgumentException("${step.verb} needs ${index + 1} words")
        return when (step.verb) {
            "wait" -> if (shows(word(0))) Attempt.Sending(null) else Attempt.Waiting("nothing shows \"${word(0)}\"")
            "gone" -> if (shows(word(0))) Attempt.Waiting("\"${word(0)}\" still shows") else Attempt.Sending(null)
            "invoke" -> invocable(word(0))?.takeIf(::enabled)?.let { send(it, if (it.text("kind") == "disclosure") (!it.flag("expanded")).toString() else "") }
                ?: Attempt.Waiting("no enabled control is named \"${word(0)}\"")
            "toggle" -> named(word(0))?.takeIf(::enabled)?.let { send(it, (!it.flag("on")).toString()) }
                ?: Attempt.Waiting("no enabled switch is named \"${word(0)}\"")
            "type" -> named(word(0))?.let { send(it, word(1)) } ?: Attempt.Waiting("no field is named \"${word(0)}\"")
            "choose" -> if (step.words.size == 2) chooser(word(1), word(0)) ?: Attempt.Waiting("nothing named \"${word(0)}\" offers \"${word(1)}\"")
                else chooser(word(0), null) ?: Attempt.Waiting("nothing offers \"${word(0)}\"")
            "shot" -> Attempt.Recording(word(0))
            "sleep" -> Attempt.Sleeping(word(0).toDoubleOrNull() ?: throw IllegalArgumentException("sleep needs a number of seconds"))
            else -> throw IllegalArgumentException("Unknown step ${step.verb}")
        }
    }

    /** The tree as an agent reads it: one line for each control, then the words on show. */
    val outline: String
        get() {
            val lines = mutableListOf<String>()
            fun walk(node: Tree) {
                val name = node.text("label").ifEmpty { node.text("title") }
                if (node["event"]?.int != null) lines.add("${node.text("kind")} \"$name\"${if (enabled(node)) "" else " disabled"}")
                else if (node.text("text").isNotEmpty()) lines.add("text \"${node.text("text")}\"")
                inside(node).forEach(::walk)
            }
            nodes.forEach(::walk)
            return lines.joinToString("\n", postfix = "\n")
        }

    private companion object {
        /** What a closed disclosure holds is not on show, and a tab bar shows every tab's name but only the chosen tab's page. */
        fun inside(node: Tree): List<Tree> = when {
            node.text("kind") == "disclosure" && !node.flag("expanded") -> emptyList()
            node.text("kind") == "tabs" -> node.list("tabs").map { if (it.text("key") == node.text("selected")) it else it.without("child") }
            else -> node.objectMembers?.values?.flatMap { it.array ?: listOf(it) }?.filter { it.objectMembers != null } ?: emptyList()
        }

        fun holds(node: Tree, words: String): Boolean {
            val own = node.objectMembers?.values?.any { value ->
                value.string?.contains(words) == true || value.array?.any { it.string?.contains(words) == true } == true
            } ?: false
            return own || inside(node).any { holds(it, words) }
        }

        fun find(node: Tree, wanted: (Tree) -> Boolean): Tree? {
            if (wanted(node)) return node
            return inside(node).firstNotNullOfOrNull { find(it, wanted) }
        }
    }
}

/** The record without its member `name`; anything else unchanged. */
fun Tree.without(name: String): Tree = (this as? Tree.Obj)?.let { Tree.Obj(it.members - name) } ?: this
