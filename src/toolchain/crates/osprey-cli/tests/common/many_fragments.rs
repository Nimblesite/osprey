//! Source fragments the multi-shot suites compose their programs from.
//!
//! Most multi-shot programs open with the same declarations — `Choice` over
//! ints or strings, the `Trace`/`Journal`/`Audit` log effects — and search
//! under the same two-answer handler ([MULTI-HANDLE-MANY]). They are macros,
//! not constants, so every program stays a `const` built by `concat!` with
//! one spelling per fragment. Shared by `common/many_programs.rs`,
//! `staged_many.rs` and `staged_multiplicity.rs`.

/// `effect Choice { control many pick: fn(int) -> int }` — the integer
/// choice point most multi-shot programs branch on.
#[macro_export]
macro_rules! choice_int {
    () => {
        "effect Choice { control many pick: fn(int) -> int }"
    };
}

/// `Choice` over strings — the answers the mutable-capture programs print.
#[macro_export]
macro_rules! choice_string {
    () => {
        "effect Choice { control many pick: fn(int) -> string }"
    };
}

/// `effect Trace { replayable note: fn(string) -> Unit }` — the log effect a
/// replayed branch appends to.
#[macro_export]
macro_rules! trace_note {
    () => {
        "effect Trace { replayable note: fn(string) -> Unit }"
    };
}

/// `effect Journal { replayable note: fn(string) -> Unit }` — the log effect
/// the mutable-capture programs watch with.
#[macro_export]
macro_rules! journal_note {
    () => {
        "effect Journal { replayable note: fn(string) -> Unit }"
    };
}

/// `effect Audit { replayable close: fn(string) -> Unit }` — the finalizer's
/// channel out of a captured region.
#[macro_export]
macro_rules! audit_close {
    () => {
        "effect Audit { replayable close: fn(string) -> Unit }"
    };
}

/// The two-answer `Choice` handler at 8-space depth: one request, two
/// resumptions, `1` then `2`.
#[macro_export]
macro_rules! two_answers {
    () => {
        "        handle Choice {
            pick _bound => satAdd(resume(1), resume(2))
        }"
    };
}

/// `fn main` that searches `search` under [`two_answers`] and prints `all`.
#[macro_export]
macro_rules! two_answer_main {
    ($search:literal) => {
        concat!(
            "fn main() = {\n    let all = {\n",
            $crate::two_answers!(),
            "\n        ",
            $search,
            "\n    }\n    print(\"${all}\")\n}\n"
        )
    };
}

/// The appending log handler at 8-space depth: a `handle` of `$effect`
/// whose `$op` arm appends `text` to `log`.
#[macro_export]
macro_rules! log_handler {
    ($effect:literal, $op:literal) => {
        concat!(
            "        handle ",
            $effect,
            " {\n            ",
            $op,
            " text => { log = \"${log}${text}\" }\n        }"
        )
    };
}
