//! Spec-driven assertions for **declaration-site variance**
//! ([TYPE-VARIANCE-DECL], [TYPE-VARIANCE-POSITIONS], [TYPE-VARIANCE-ASSIGN],
//! docs/specs/0004-TypeSystem.md).
//!
//! Position checking is the crisp half: "fields and function results are OUTPUT
//! positions; function parameters flip the polarity (INPUT); a nested
//! constructor's argument composes the position with that constructor's
//! declared variance". Every composition below is one row of that rule, and the
//! built-in variance table (`Result<out T, out E>`, `List<out T>`,
//! `Fiber<out T>`, `Map<K, out V>`, invariant `Channel<T>`) is checked THROUGH
//! it — a wrong entry in that table shows up as a position that should have
//! been rejected and was not.

use crate::testutil::{accepts, rejected_somehow, rejects_with, variance_position_message};
use osprey_syntax::Flavor;

/// The position diagnostic for a type declaration's FIELD.
fn position_message(param: &str, marker: &str, position: &str, field: &str, owner: &str) -> String {
    variance_position_message(param, marker, position, "field", field, owner)
}

/// A one-field record declaration, the smallest carrier of a position.
fn decl(params: &str, field: &str, ty: &str) -> String {
    format!("type Holder{params} = {{ {field}: {ty} }}\nprint(\"declared\")")
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-POSITIONS] — output positions
// ---------------------------------------------------------------------------

/// A field is an OUTPUT position, so `out T` belongs there.
#[test]
fn out_is_legal_in_a_field() {
    accepts(Flavor::Default, &decl("<out T>", "supply", "T"));
}

/// A function RESULT is an output position, so `out T` belongs there too.
#[test]
fn out_is_legal_in_a_function_result() {
    accepts(Flavor::Default, &decl("<out T>", "make", "(int) -> T"));
}

/// A function PARAMETER flips the polarity to input: `out T` is rejected.
#[test]
fn out_in_a_function_parameter_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<out T>", "consume", "(T) -> int"),
        &position_message("T", "out", "input", "consume", "Holder"),
    );
}

/// Two flips compose back to an output position, so `out T` is legal again.
#[test]
fn out_under_two_parameter_flips_is_legal() {
    accepts(Flavor::Default, &decl("<out T>", "hof", "((T) -> int) -> int"));
}

/// An `in` parameter belongs in an input position.
#[test]
fn in_is_legal_in_a_function_parameter() {
    accepts(Flavor::Default, &decl("<in T>", "admit", "(T) -> bool"));
}

/// A field is an output position, so `in T` is rejected there.
#[test]
fn in_in_a_field_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "held", "T"),
        &position_message("T", "in", "output", "held", "Holder"),
    );
}

/// A function result is an output position, so `in T` is rejected there.
#[test]
fn in_in_a_function_result_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "make", "(int) -> T"),
        &position_message("T", "in", "output", "make", "Holder"),
    );
}

/// Two flips compose back to output, so `in T` is rejected under them.
#[test]
fn in_under_two_parameter_flips_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "hof", "((T) -> int) -> int"),
        &position_message("T", "in", "output", "hof", "Holder"),
    );
}

/// An unannotated parameter is invariant and sits in EITHER position.
#[test]
fn an_invariant_parameter_is_legal_in_both_positions() {
    accepts(Flavor::Default, &decl("<T>", "held", "T"));
    accepts(Flavor::Default, &decl("<T>", "consume", "(T) -> int"));
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-POSITIONS] — composition through a nested constructor
// ---------------------------------------------------------------------------

/// A covariant argument of a covariant constructor keeps the position.
#[test]
fn out_inside_a_covariant_constructor_argument_is_legal() {
    accepts(
        Flavor::Default,
        &format!("type Feed<out A> = {{ supply: A }}\n{}", decl("<out T>", "nested", "Feed<T>")),
    );
}

/// A covariant argument of a CONTRAVARIANT constructor flips the position.
#[test]
fn out_inside_a_contravariant_constructor_argument_is_rejected() {
    rejects_with(
        Flavor::Default,
        &format!("type Gate<in A> = {{ admit: (A) -> bool }}\n{}", decl("<out T>", "nested", "Gate<T>")),
        &position_message("T", "out", "input", "nested", "Holder"),
    );
}

/// The mirror: `in T` inside a contravariant argument lands in output.
#[test]
fn in_inside_a_contravariant_constructor_argument_is_rejected() {
    rejects_with(
        Flavor::Default,
        &format!("type Gate<in A> = {{ admit: (A) -> bool }}\n{}", decl("<in T>", "nested", "Gate<T>")),
        &position_message("T", "in", "output", "nested", "Holder"),
    );
}

/// "an invariant argument position demands both directions, so only invariant
/// parameters may sit there" — a covariant parameter may not.
#[test]
fn out_inside_an_invariant_constructor_argument_is_rejected() {
    rejected_somehow(
        Flavor::Default,
        &format!("type Cell<A> = {{ slot: A }}\n{}", decl("<out T>", "nested", "Cell<T>")),
    );
}

/// …nor may a contravariant one.
#[test]
fn in_inside_an_invariant_constructor_argument_is_rejected() {
    rejected_somehow(
        Flavor::Default,
        &format!("type Cell<A> = {{ slot: A }}\n{}", decl("<in T>", "nested", "Cell<T>")),
    );
}

/// An invariant parameter is exactly what an invariant argument accepts.
#[test]
fn an_invariant_parameter_inside_an_invariant_argument_is_legal() {
    accepts(
        Flavor::Default,
        &format!("type Cell<A> = {{ slot: A }}\n{}", decl("<T>", "nested", "Cell<T>")),
    );
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-ASSIGN] — the built-in variance table, checked by position
// ---------------------------------------------------------------------------

/// `List<out T>`: a covariant parameter may sit in its argument.
#[test]
fn list_is_covariant_in_its_element() {
    accepts(Flavor::Default, &decl("<out T>", "items", "List<T>"));
}

/// `Result<out T, out E>`: both arguments are covariant.
#[test]
fn result_is_covariant_in_both_arguments() {
    accepts(Flavor::Default, &decl("<out T>", "value", "Result<T, string>"));
    accepts(Flavor::Default, &decl("<out T>", "failure", "Result<int, T>"));
}

/// `Fiber<out T>`: covariant in its answer.
#[test]
fn fiber_is_covariant_in_its_answer() {
    accepts(Flavor::Default, &decl("<out T>", "worker", "Fiber<T>"));
}

/// `Map<K, out V>`: the VALUE is covariant.
#[test]
fn map_is_covariant_in_its_value() {
    accepts(Flavor::Default, &decl("<out T>", "byName", "Map<string, T>"));
}

/// `Map<K, out V>`: the KEY is invariant, so a covariant parameter is rejected.
#[test]
fn map_keys_are_invariant() {
    rejected_somehow(Flavor::Default, &decl("<out T>", "byKey", "Map<T, int>"));
}

/// `Channel<T>` is invariant in both directions.
#[test]
fn channel_is_invariant() {
    rejected_somehow(Flavor::Default, &decl("<out T>", "wire", "Channel<T>"));
    rejected_somehow(Flavor::Default, &decl("<in T>", "wire", "Channel<T>"));
}

/// A `List` of a contravariant parameter is an output position.
#[test]
fn in_inside_a_list_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "items", "List<T>"),
        &position_message("T", "in", "output", "items", "Holder"),
    );
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-ASSIGN] — leaves match exactly, in every container
// ---------------------------------------------------------------------------

/// A `Result` payload is never extracted under a container, whatever the
/// declared variance: the stored representation differs.
fn feed_payload_program(marker: &str) -> String {
    format!(
        r#"type Feed<{marker}T> = Feed {{ supply: T }} | Dry
fn firstItem(f: Feed<int>) = match f {{
    Feed {{ supply }} => supply
    Dry => 0
}}
fn mkFeed() -> Feed<Result<int, MathError>> = Feed {{ supply: 20 * 5 }}
print("${{firstItem(mkFeed())}}")"#
    )
}

/// Under `out T`.
#[test]
fn a_result_payload_does_not_collapse_under_a_covariant_container() {
    rejects_with(Flavor::Default, &feed_payload_program("out "), "cannot unify");
}

/// Under an invariant parameter.
#[test]
fn a_result_payload_does_not_collapse_under_an_invariant_container() {
    rejects_with(Flavor::Default, &feed_payload_program(""), "cannot unify");
}

/// "Function returns also match exactly, so a `Feed<(int) -> Result<int, Error>>`
/// does not match a `Feed<(int) -> int>` slot" — the spec's own example.
#[test]
fn a_function_payloads_return_channel_matches_exactly() {
    rejects_with(
        Flavor::Default,
        r#"type Feed<out T> = Feed { supply: T } | Dry
fn label(f: Feed<(int) -> int>) = match f {
    Feed { supply } => "supplied"
    Dry => "dry"
}
fn mkFeed() -> Feed<(int) -> Result<int, MathError>> = Feed { supply: |n| => n * 2 }
print(label(mkFeed()))"#,
        "cannot unify",
    );
}

/// The direct value site keeps the one safe promotion `T -> Result<T, E>`,
/// which is what makes the container rejections above a real restriction rather
/// than a blanket ban.
#[test]
fn the_direct_site_promotion_still_holds() {
    accepts(
        Flavor::Default,
        r#"fn keep(n: Result<int, MathError>) = n ?: 0
print("${keep(5)}")"#,
    );
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-DECL] — where the markers may be written at all
// ---------------------------------------------------------------------------

/// "`out` and `in` are contextual keywords, reserved only inside
/// type-parameter lists" — they stay usable as ordinary names.
#[test]
fn out_and_in_stay_legal_identifiers_outside_a_parameter_list() {
    accepts(
        Flavor::Default,
        r#"let out = 1
let inn = 2
fn keep(out) = out
print("${keep(out)} ${inn}")"#,
    );
}

/// Variance is declaration-site on TYPES and EFFECTS only — never on a
/// function binder ([TYPE-GENERICS-FN]).
#[test]
fn a_variance_marker_on_a_function_binder_is_rejected() {
    rejects_with(
        Flavor::Default,
        r#"fn pick<out T>(first: T, second: T) -> T = first
print("${pick(1, 2)}")"#,
        "variance annotations are only valid on type and effect declarations",
    );
    rejects_with(
        Flavor::Default,
        r#"fn pick<in T>(first: T, second: T) -> T = first
print("${pick(1, 2)}")"#,
        "variance annotations are only valid on type and effect declarations",
    );
}

// ---------------------------------------------------------------------------
// [FLAVOR-ML-GENERICS] — the same rules through the ML spellings
// ---------------------------------------------------------------------------

/// "Types use juxtaposed binders: `type Box T`, `type Feed out T`,
/// `type Sink in T`."
#[test]
fn ml_variance_binders_check_the_same_positions() {
    accepts(
        Flavor::Ml,
        "type Feed out T =\n    supply : T\nprint \"declared\"\n",
    );
    rejects_with(
        Flavor::Ml,
        "type Bad out T =\n    consume : T -> int\nprint \"declared\"\n",
        &position_message("T", "out", "input", "consume", "Bad"),
    );
}

/// The ML twin of the contravariant rules.
#[test]
fn ml_contravariant_binders_check_the_same_positions() {
    accepts(
        Flavor::Ml,
        "type Gate in T =\n    admit : T -> bool\nprint \"declared\"\n",
    );
    rejects_with(
        Flavor::Ml,
        "type Bad in T =\n    held : T\nprint \"declared\"\n",
        &position_message("T", "in", "output", "held", "Bad"),
    );
}

/// ML function binders reject variance exactly as Default's do.
#[test]
fn ml_a_variance_marker_on_a_function_binder_is_rejected() {
    rejects_with(
        Flavor::Ml,
        "pick<out T> : (T, T) -> T\npick (first, second) = first\nkept = pick (1, 2)\nprint \"${kept}\"\n",
        "variance annotations are only valid on type and effect declarations",
    );
}
