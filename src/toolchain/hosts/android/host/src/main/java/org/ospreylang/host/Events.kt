package org.ospreylang.host

// Implements [NATIVE-SHM-EVENT]: one host event is one typed call — a kind,
// the control it names, two texts, two numbers and two flags. Nothing is
// encoded; what a control means is the application's, through the binding its
// number names.

data class Touch(val kind: Long, val ordinal: Long = 0, val words: String = "",
                 val detail: String = "", val x: Double = 0.0, val y: Double = 0.0,
                 val down: Boolean = false, val shift: Boolean = false) {
    companion object {
        /** The event kinds the application's ui::Hosting declares. */
        const val CONTROL: Long = 0
        const val KEY: Long = 1
        const val POINTER: Long = 2
        const val VIEWED: Long = 3
        const val POINT: Long = 4
        const val STROKE: Long = 5
        const val ENCLOSE: Long = 6
        const val LASSO: Long = 7
        const val COMPLETED: Long = 8
        const val PHASE: Long = 9
        const val FRAME: Long = 10
        const val APPEARANCE: Long = 11

        /** The control numbered `ordinal` delivering `words`: empty for a tap,
         * true/false for a switch, a key for a picker, text for a field. */
        fun control(ordinal: Long, words: String) = Touch(CONTROL, ordinal, words)

        /** A key going down or up over the scene numbered `ordinal`. */
        fun key(ordinal: Long, name: String, down: Boolean) =
            Touch(KEY, ordinal, name, down = down)

        /** The pointer over the scene numbered `ordinal`, in scene units. */
        fun pointer(ordinal: Long, x: Double, y: Double, down: Boolean, shift: Boolean) =
            Touch(POINTER, ordinal, x = x, y = y, down = down, shift = shift)

        /** The scene numbered `ordinal` measured in scene units. */
        fun viewed(ordinal: Long, width: Double, height: Double) =
            Touch(VIEWED, ordinal, x = width, y = height)

        /** The capability under `ticket` finished: its result text when it
         * succeeded, or the host's reason when it failed. */
        fun completed(ticket: String, result: String) =
            Touch(COMPLETED, words = ticket, detail = result, down = true)

        fun completedFailed(ticket: String, reason: String) =
            Touch(COMPLETED, words = ticket, detail = reason, down = false)

        /** A lifecycle phase, such as `active` or `closed`. */
        fun phase(name: String) = Touch(PHASE, words = name)

        /** One display frame, `elapsed` seconds after the last. */
        fun frame(elapsed: Double) = Touch(FRAME, x = elapsed)

        /** The system appearance changed. */
        fun appearance(dark: Boolean) = Touch(APPEARANCE, down = dark)
    }
}
