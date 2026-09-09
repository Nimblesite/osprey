//! Spec-driven assertions for **generic effects** ([EFFECTS-GENERIC-DECL],
//! [EFFECTS-GENERIC-INSTANTIATION], [EFFECTS-GENERIC-RUNTIME],
//! [EFFECTS-GENERIC-ROWS], docs/specs/0017-AlgebraicEffects.md).
//!
//! Discharge across instantiations is already exercised by
//! `effect_rows_tests.rs`; this module states the parts of the generic-effect
//! surface that module does not touch — the DECLARATION's variance positions,
//! the written instantiation spellings (`handle Stash<int>`,
//! `perform Stash<int>.take()`), rows with several generic entries, and the ML
//! twins of each.

use crate::testutil::{accepts, rejected_somehow, rejects_with, variance_position_message};
use osprey_syntax::Flavor;

/// The position diagnostic for an effect declaration's OPERATION.
fn op_position_message(param: &str, marker: &str, position: &str, op: &str, owner: &str) -> String {
    variance_position_message(param, marker, position, "operation", op, owner)
}

/// A generic effect plus a handler that discharges it at `instantiation`.
fn stash_program(instantiation: &str, answer: &str) -> String {
    format!(
        r#"effect Stash<T> {{
    take: fn() -> T
}}
fn main() -> Unit = {{
    let held = handle Stash{instantiation}
        take => {answer}
    in perform Stash{instantiation}.take()
    print("${{held}}")
}}"#
    )
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-DECL] — operation parameters are inputs, results outputs
// ---------------------------------------------------------------------------

/// "An effect may declare type parameters, including `in` and `out` variance."
#[test]
fn a_generic_effect_declares_type_parameters() {
    accepts(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
print("declared")"#,
    );
}

/// An operation RESULT is an output position: `out T` belongs there.
#[test]
fn out_is_legal_in_an_operation_result() {
    accepts(
        Flavor::Default,
        r#"effect Source<out T> {
    next: fn() -> T
}
print("declared")"#,
    );
}

/// An operation PARAMETER is an input position: `out T` is rejected.
#[test]
fn out_in_an_operation_parameter_is_rejected() {
    rejects_with(
        Flavor::Default,
        r#"effect Bad<out T> {
    send: fn(T) -> Unit
}
print("declared")"#,
        &op_position_message("T", "out", "input", "send", "Bad"),
    );
}

/// The contravariant mirror: `in T` belongs in an operation parameter.
#[test]
fn in_is_legal_in_an_operation_parameter() {
    accepts(
        Flavor::Default,
        r#"effect Sink<in T> {
    send: fn(T) -> Unit
}
print("declared")"#,
    );
}

/// …and is rejected in an operation result.
#[test]
fn in_in_an_operation_result_is_rejected() {
    rejects_with(
        Flavor::Default,
        r#"effect Bad<in T> {
    next: fn() -> T
}
print("declared")"#,
        &op_position_message("T", "in", "output", "next", "Bad"),
    );
}

/// An invariant parameter sits in either position.
#[test]
fn an_invariant_effect_parameter_is_legal_in_both_positions() {
    accepts(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
print("declared")"#,
    );
}

/// Two binders on one effect are independent.
#[test]
fn an_effect_may_declare_two_binders() {
    accepts(
        Flavor::Default,
        r#"effect Table<K, V> {
    get: fn(K) -> V
}
print("declared")"#,
    );
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-INSTANTIATION] — the written instantiation
// ---------------------------------------------------------------------------

/// "Each handler site instantiates a generic effect independently."
#[test]
fn a_written_instantiation_is_accepted_on_handle_and_perform() {
    accepts(Flavor::Default, &stash_program("<int>", "9"));
}

/// The same program at a different instantiation.
#[test]
fn a_written_string_instantiation_is_accepted() {
    accepts(Flavor::Default, &stash_program("<string>", "\"ready\""));
}

/// "Handler arm values and performs in the handled body must agree on that
/// instantiation" — an arm answering the wrong type is rejected.
#[test]
fn a_handler_arm_disagreeing_with_the_written_instantiation_is_rejected() {
    rejects_with(
        Flavor::Default,
        &stash_program("<int>", "\"ready\""),
        "cannot unify",
    );
}

/// The instantiation may also be left to inference at both sites.
#[test]
fn an_inferred_instantiation_is_accepted() {
    accepts(Flavor::Default, &stash_program("", "9"));
}

/// A written `handle Stash<int>` does not discharge a `Stash<string>` perform.
#[test]
fn a_written_handler_instantiation_discharges_only_its_own_operations() {
    rejected_somehow(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() = perform Stash.put("text")
fn main() -> Unit = {
    let done = handle Stash<int>
        put v => print("stored")
    in store()
    print("${done}")
}"#,
    );
}

/// Two handlers at two instantiations coexist in ONE program: the sites are
/// independent, not one global instantiation.
#[test]
fn two_instantiations_of_one_effect_coexist() {
    accepts(
        Flavor::Default,
        r#"effect Stash<T> {
    take: fn() -> T
}
fn main() -> Unit = {
    let n = handle Stash<int>
        take => 1
    in perform Stash<int>.take()
    let s = handle Stash<string>
        take => "one"
    in perform Stash<string>.take()
    print("${n}${s}")
}"#,
    );
}

/// A written instantiation whose arity misses the declaration is rejected.
#[test]
fn a_written_effect_instantiation_arity_is_checked() {
    rejected_somehow(
        Flavor::Default,
        r#"effect Stash<T> {
    take: fn() -> T
}
fn main() -> Unit = {
    let held = handle Stash<int, string>
        take => 1
    in perform Stash<int>.take()
    print("${held}")
}"#,
    );
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-ROWS] — a row entry pins an instantiation
// ---------------------------------------------------------------------------

/// "A row entry such as `!Stash<int>` pins the generic effect instantiation
/// used by performs in that function body."
#[test]
fn a_row_entry_pins_the_instantiation() {
    accepts(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash<int> = perform Stash.put(42)
fn main() -> Unit = {
    let done = handle Stash<int>
        put v => print("stored ${v}")
    in store()
    print("${done}")
}"#,
    );
}

/// A body contradicting its own pinned row is rejected, naming the row.
#[test]
fn a_body_contradicting_its_pinned_row_is_rejected() {
    rejects_with(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash<int> = perform Stash.put("text")
fn main() -> Unit = {
    let done = handle Stash<string>
        put v => print("stored")
    in store()
    print("${done}")
}"#,
        "outside its declared row",
    );
}

/// "A bare generic entry leaves its arguments to inference."
#[test]
fn a_bare_generic_row_entry_leaves_its_arguments_to_inference() {
    accepts(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash = perform Stash.put(42)
fn main() -> Unit = {
    let done = handle Stash
        put v => print("stored ${v}")
    in store()
    print("${done}")
}"#,
    );
}

/// A bracketed row may carry SEVERAL generic entries, each at its own
/// instantiation ([EFFECTS-GENERIC-ROWS], `effectSet`).
#[test]
fn a_bracketed_row_carries_several_generic_entries() {
    accepts(
        Flavor::Default,
        r#"effect Read<T> {
    get: fn() -> T
}
effect Write<T> {
    put: fn(T) -> Unit
}
fn copy() -> Unit ![Read<int>, Write<string>] = perform Write.put("${perform Read.get()}")
fn main() -> Unit = {
    let done = handle Read<int>
        get => 7
    in handle Write<string>
        put v => print("wrote ${v}")
    in copy()
    print("${done}")
}"#,
    );
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-RUNTIME] — the checker-visible half of the erased ABI
// ---------------------------------------------------------------------------

/// "Static discharge distinguishes resolved instantiations" — an undischarged
/// operation is named AT its instantiation at program entry, which is what the
/// runtime's mangled key (`Stash$string`) mirrors.
#[test]
fn an_unhandled_generic_operation_is_named_at_its_instantiation() {
    rejects_with(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() = perform Stash.put("text")
fn main() -> Unit = store()"#,
        "Stash<string>.put",
    );
}

// ---------------------------------------------------------------------------
// [FLAVOR-ML-GENERICS] — the ML spellings of the same effect surface
// ---------------------------------------------------------------------------

/// "Effects use the same binder form: `effect Stash T`." ML operation arrows
/// are effectful (`=>`), as the corpus writes them.
#[test]
fn ml_effect_binders_are_juxtaposed() {
    accepts(
        Flavor::Ml,
        "effect Stash T\n    take : Unit => T\nprint \"declared\"\n",
    );
}

/// "Effect rows apply arguments with angles: `! Stash<int>`."
#[test]
fn ml_rows_apply_arguments_with_angles() {
    accepts(
        Flavor::Ml,
        "effect Stash T\n    put : T => Unit\n\
         store : Unit -> Unit ! Stash<int>\n\
         store () = perform Stash.put 42\n\
         main () =\n\
         \x20   done = handle Stash<int>\n\
         \x20       put v => print \"stored\"\n\
         \x20   in store ()\n\
         \x20   print \"${done}\"\n",
    );
}

/// A bracketed ML row carries several generic entries: `! [Read<T>, Write<T>]`.
#[test]
fn ml_a_bracketed_row_carries_several_generic_entries() {
    accepts(
        Flavor::Ml,
        "effect Read T\n    get : Unit => T\n\
         effect Write T\n    put : T => Unit\n\
         copy : Unit -> Unit ! [Read<int>, Write<string>]\n\
         copy () = perform Write.put \"${perform Read.get ()}\"\n",
    );
}

/// The ML twin of the operation-position rule.
#[test]
fn ml_out_in_an_operation_parameter_is_rejected() {
    rejects_with(
        Flavor::Ml,
        "effect Bad out T\n    send : T => Unit\nprint \"declared\"\n",
        &op_position_message("T", "out", "input", "send", "Bad"),
    );
}

/// The ML twin of the written-instantiation rules.
#[test]
fn ml_a_written_instantiation_is_accepted_on_handle_and_perform() {
    accepts(
        Flavor::Ml,
        "effect Stash T\n    take : Unit => T\n\
         main () =\n\
         \x20   held = handle Stash<int>\n\
         \x20       take => 9\n\
         \x20   in perform Stash<int>.take ()\n\
         \x20   print \"${held}\"\n",
    );
}

// ---------------------------------------------------------------------------
// Spec-vs-language drift, stated as a test rather than left as prose
// ---------------------------------------------------------------------------

/// [EFFECTS-GENERIC-INSTANTIATION]'s own example writes the handled body after
/// `do`, not `in`. The language accepts only `in` today; the rename is plan
/// 0027 phase 0. One of the two sources is stale, and this test says which.
#[test]
fn the_spec_writes_a_handled_body_after_do() {
    accepts(
        Flavor::Default,
        r#"effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
let word = handle Stash
    put value => print(value)
    take => "ready"
do perform Stash.take()
print(word)"#,
    );
}
