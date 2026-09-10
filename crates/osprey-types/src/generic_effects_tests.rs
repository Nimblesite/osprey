//! Spec-driven assertions for **generic effects** ([EFFECTS-GENERIC-DECL],
//! [EFFECTS-GENERIC-INSTANTIATION], [EFFECTS-GENERIC-RUNTIME],
//! [EFFECTS-GENERIC-ROWS], docs/specs/0017-AlgebraicEffects.md).
//!
//! Discharge across instantiations is already exercised by
//! `effect_rows_tests.rs`; this module states the parts of the generic-effect
//! surface that module does not touch — the DECLARATION's variance positions,
//! the instantiation surface (inferred at `handle`/`perform`, WRITTEN only on
//! an effect row), rows with several generic entries, and the ML twins of each.
//!
//! One boundary is easy to get wrong and is pinned below: a written
//! instantiation is legal on a ROW (`!Stash<int>`) and rejected on a dynamic
//! effect's `handle`/`perform` ([STAGE-SIGNALS-EXACT], spec 0035) — that
//! spelling is reserved for `static effect`, where the instantiation IS the
//! identity.

use crate::testutil::{spec_cases, variance_position_message};
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// The position diagnostic for an effect declaration's OPERATION.
fn op_position_message(param: &str, marker: &str, position: &str, op: &str, owner: &str) -> String {
    variance_position_message(param, marker, position, "operation", op, owner)
}

/// A generic effect whose instantiation is INFERRED from the handler arm — the
/// only spelling a dynamic effect accepts, since a written instantiation is
/// rejected by [STAGE-SIGNALS-EXACT] (see
/// `a_written_instantiation_on_a_dynamic_effect_is_rejected`).
fn stash_program(answer: &str) -> String {
    format!(
        r#"effect Stash<T> {{
    take: fn() -> T
}}
fn main() -> Unit = {{
    let held = handle Stash
        take => {answer}
    in perform Stash.take()
    print("${{held}}")
}}"#
    )
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-DECL] — operation parameters are inputs, results outputs
// ---------------------------------------------------------------------------

spec_cases! {
    /// "An effect may declare type parameters, including `in` and `out` variance."
    a_generic_effect_declares_type_parameters: accepts(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
print("declared")"#);

    /// An operation RESULT is an output position: `out T` belongs there.
    out_is_legal_in_an_operation_result: accepts(Default, r#"effect Source<out T> {
    next: fn() -> T
}
print("declared")"#);

    /// An operation PARAMETER is an input position: `out T` is rejected.
    out_in_an_operation_parameter_is_rejected: rejects_with(Default, r#"effect Bad<out T> {
    send: fn(T) -> Unit
}
print("declared")"#, op_position_message("T", "out", "input", "send", "Bad"));

    /// The contravariant mirror: `in T` belongs in an operation parameter.
    in_is_legal_in_an_operation_parameter: accepts(Default, r#"effect Sink<in T> {
    send: fn(T) -> Unit
}
print("declared")"#);

    /// …and is rejected in an operation result.
    in_in_an_operation_result_is_rejected: rejects_with(Default, r#"effect Bad<in T> {
    next: fn() -> T
}
print("declared")"#, op_position_message("T", "in", "output", "next", "Bad"));

    /// An invariant parameter sits in either position.
    an_invariant_effect_parameter_is_legal_in_both_positions: accepts(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
print("declared")"#);

    /// Two binders on one effect are independent.
    an_effect_may_declare_two_binders: accepts(Default, r#"effect Table<K, V> {
    get: fn(K) -> V
}
print("declared")"#);
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-INSTANTIATION] — the written instantiation
// ---------------------------------------------------------------------------

spec_cases! {
    /// "Each handler site instantiates a generic effect independently."
    a_written_instantiation_is_accepted_on_handle_and_perform: accepts(Default, stash_program("9"));

    /// The same program at a different instantiation.
    a_written_string_instantiation_is_accepted: accepts(Default, stash_program("\"ready\""));

    /// A handler answering `string` cannot discharge the independently inferred
    /// `Stash<int>.take` requirement of a helper ([EFFECTS-GENERIC-RUNTIME]).
    a_handler_arm_disagreeing_with_the_body_is_rejected: rejects_with(Default, r#"effect Stash<T> {
    take: fn() -> T
}
fn doubled() -> int = (perform Stash.take()) * 2 ?: 0
fn main() -> Unit = {
    let held = handle Stash
        take => "ready"
    in doubled()
    print("${held}")
}"#, "unhandled effect operations at program entry: Stash<int>.take");

    /// The instantiation may also be left to inference at both sites.
    an_inferred_instantiation_is_accepted: accepts(Default, stash_program("9"));
}

/// A written instantiation on a DYNAMIC effect's `perform` is rejected outright
/// — the spelling belongs to `static effect`, whose identity IS the
/// instantiation ([STAGE-SIGNALS-EXACT], pinned end-to-end by
/// `examples/failscompilation/stage_signal_instantiated_dynamic_effect.ospo`).
#[test]
fn a_written_instantiation_on_a_dynamic_perform_is_rejected() {
    let parsed = parse_program_with_flavor(
        r#"effect Stash<T> {
    take: fn() -> T
}
fn main() -> Unit = {
    let held = handle Stash
        take => 9
    in perform Stash<int>.take()
    print("${held}")
}"#,
        Flavor::Default,
    );
    assert_eq!(parsed.errors.len(), 1, "{:?}", parsed.errors);
    assert!(
        parsed.errors.iter().any(|error| error
            .message
            .starts_with("`perform Stash<int>` names an instantiation of dynamic effect `Stash`")),
        "{:?}",
        parsed.errors
    );
}

spec_cases! {
    /// The same restriction at the `handle` site.
    a_written_instantiation_on_a_dynamic_handle_is_rejected: rejected_somehow(Default, r#"effect Stash<T> {
    take: fn() -> T
}
fn main() -> Unit = {
    let held = handle Stash<int>
        take => 9
    in perform Stash.take()
    print("${held}")
}"#);

    /// A handler discharges only the instantiation it was INFERRED at: a handler
    /// answering `int` does not serve a `Stash<string>.put`.
    an_inferred_handler_instantiation_discharges_only_its_own_operations: rejected_somehow(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() = perform Stash.put("text")
fn main() -> Unit = {
    let done = handle Stash
        put v => print("stored ${v + 1 ?: 0}")
    in store()
    print("${done}")
}"#);

    /// Two handlers at two instantiations coexist in ONE program: "Each handler
    /// site instantiates a generic effect independently", and each infers its own.
    two_instantiations_of_one_effect_coexist: accepts(Default, r#"effect Stash<T> {
    take: fn() -> T
}
fn main() -> Unit = {
    let n = handle Stash
        take => 1
    in perform Stash.take()
    let s = handle Stash
        take => "one"
    in perform Stash.take()
    print("${n}${s}")
}"#);

    /// A row's written instantiation IS checked for arity — the row is the surface
    /// that keeps the angle spelling, so the count contract lives there.
    an_effect_row_instantiation_arity_is_checked: rejected_somehow(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash<int, string> = perform Stash.put(42)
fn main() -> Unit = {
    let done = handle Stash
        put v => print("stored")
    in store()
    print("${done}")
}"#);
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-ROWS] — a row entry pins an instantiation
// ---------------------------------------------------------------------------

spec_cases! {
    /// "A row entry such as `!Stash<int>` pins the generic effect instantiation
    /// used by performs in that function body."
    a_row_entry_pins_the_instantiation: accepts(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash<int> = perform Stash.put(42)
fn main() -> Unit = {
    let done = handle Stash
        put v => print("stored ${v}")
    in store()
    print("${done}")
}"#);

    /// A body contradicting its own pinned row is rejected, naming the row.
    a_body_contradicting_its_pinned_row_is_rejected: rejects_with(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash<int> = perform Stash.put("text")
fn main() -> Unit = {
    let done = handle Stash
        put v => print("stored")
    in store()
    print("${done}")
}"#, "outside its declared row");

    /// "A bare generic entry leaves its arguments to inference."
    a_bare_generic_row_entry_leaves_its_arguments_to_inference: accepts(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() -> Unit !Stash = perform Stash.put(42)
fn main() -> Unit = {
    let done = handle Stash
        put v => print("stored ${v}")
    in store()
    print("${done}")
}"#);

    /// A bracketed row may carry SEVERAL generic entries, each at its own
    /// instantiation ([EFFECTS-GENERIC-ROWS], `effectSet`).
    a_bracketed_row_carries_several_generic_entries: accepts(Default, r#"effect Read<T> {
    get: fn() -> T
}
effect Write<T> {
    put: fn(T) -> Unit
}
fn copy() -> Unit ![Read<int>, Write<string>] = perform Write.put("${perform Read.get()}")
fn main() -> Unit = {
    let done = handle Read
        get => 7
    in handle Write
        put v => print("wrote ${v}")
    in copy()
    print("${done}")
}"#);
}

// ---------------------------------------------------------------------------
// [EFFECTS-GENERIC-RUNTIME] — the checker-visible half of the erased ABI
// ---------------------------------------------------------------------------

spec_cases! {
    /// "Static discharge distinguishes resolved instantiations" — an undischarged
    /// operation is named AT its instantiation at program entry, which is what the
    /// runtime's mangled key (`Stash$string`) mirrors.
    an_unhandled_generic_operation_is_named_at_its_instantiation: rejects_with(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
}
fn store() = perform Stash.put("text")
fn main() -> Unit = store()"#, "Stash<string>.put");
}

// ---------------------------------------------------------------------------
// [FLAVOR-ML-GENERICS] — the ML spellings of the same effect surface
// ---------------------------------------------------------------------------

spec_cases! {
    /// "Effects use the same binder form: `effect Stash T`." ML operation arrows
    /// are effectful (`=>`), as the corpus writes them.
    ml_effect_binders_are_juxtaposed: accepts(Ml, "effect Stash T\n    take : Unit => T\nprint \"declared\"\n");

    /// "Effect rows apply arguments with angles: `! Stash<int>`."
    ml_rows_apply_arguments_with_angles: accepts(Ml, "effect Stash T\n    put : T => Unit\n\
         store : Unit -> Unit ! Stash<int>\n\
         store () = perform Stash.put 42\n\
         main () =\n\
         \x20   done = handle Stash\n\
         \x20       put v => print \"stored\"\n\
         \x20   in store ()\n\
         \x20   print \"${done}\"\n");

    /// A bracketed ML row carries several generic entries: `! [Read<T>, Write<T>]`.
    ml_a_bracketed_row_carries_several_generic_entries: accepts(Ml, "effect Read T\n    get : Unit => T\n\
         effect Write T\n    put : T => Unit\n\
         copy : Unit -> Unit ! [Read<int>, Write<string>]\n\
         copy () = perform Write.put \"${perform Read.get ()}\"\n");

    /// The ML twin of the operation-position rule.
    ml_out_in_an_operation_parameter_is_rejected: rejects_with(Ml, "effect Bad out T\n    send : T => Unit\nprint \"declared\"\n", op_position_message("T", "out", "input", "send", "Bad"));

    /// The ML twin of the written-instantiation rules.
    ml_a_written_instantiation_is_accepted_on_handle_and_perform: accepts(Ml, "effect Stash T\n    take : Unit => T\n\
         main () =\n\
         \x20   held = handle Stash\n\
         \x20       take => 9\n\
         \x20   in perform Stash.take ()\n\
         \x20   print \"${held}\"\n");
}

// ---------------------------------------------------------------------------
// Spec-vs-language drift, stated as a test rather than left as prose
// ---------------------------------------------------------------------------

spec_cases! {
    /// [EFFECTS-GENERIC-INSTANTIATION]'s own example writes the handled body after
    /// `do`, not `in`. The language accepts only `in` today; the rename is plan
    /// 0027 phase 0. One of the two sources is stale, and this test says which.
    the_spec_writes_a_handled_body_after_do: accepts(Default, r#"effect Stash<T> {
    put: fn(T) -> Unit
    take: fn() -> T
}
let word = handle Stash
    put value => print(value)
    take => "ready"
do perform Stash.take()
print(word)"#);
}
