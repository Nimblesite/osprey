//! Spec-driven assertions for **declaration-site variance**
//! ([TYPE-VARIANCE-DECL], [TYPE-VARIANCE-POSITIONS], [TYPE-VARIANCE-ASSIGN],
//! docs/specs/language/0004-TypeSystem.md).
//!
//! Position checking is the crisp half: "fields and function results are OUTPUT
//! positions; function parameters flip the polarity (INPUT); a nested
//! constructor's argument composes the position with that constructor's
//! declared variance". Every composition below is one row of that rule, and the
//! built-in variance table (`Result<out T, out E>`, `List<out T>`,
//! `Fiber<out T>`, `Map<K, out V>`, invariant `Channel<T>`) is checked THROUGH
//! it — a wrong entry in that table shows up as a position that should have
//! been rejected and was not.

use crate::testutil::{accepts, plain_cases, rejects_with, variance_position_message};
use osprey_syntax::Flavor;

/// The position diagnostic for a type declaration's FIELD.
pub(crate) fn position_message(
    param: &str,
    marker: &str,
    position: &str,
    field: &str,
    owner: &str,
) -> String {
    variance_position_message(param, marker, position, "field", field, owner)
}

/// A one-field record declaration, the smallest carrier of a position.
pub(crate) fn decl(params: &str, field: &str, ty: &str) -> String {
    format!("type Holder{params} = {{ {field}: {ty} }}\nprint(\"declared\")")
}

/// A position check through a nested constructor: the constructor's own
/// declaration sits above the one-field record that carries the position.
fn nested(prefix: &str, marker: &str, arg: &str) -> String {
    format!("{prefix}\n{}", decl(marker, "nested", arg))
}

/// A variance marker written on a function binder, where it is never legal.
fn fn_binder(marker: &str) -> String {
    format!("fn pick<{marker} T>(first: T, second: T) -> T = first\nprint(\"${{pick(1, 2)}}\")")
}

/// A one-field ML record declaration in the juxtaposed-binder spelling.
fn ml_decl(owner: &str, marker: &str, field: &str, ty: &str) -> String {
    format!("type {owner} {marker} T =\n    {field} : {ty}\nprint \"declared\"\n")
}

// [TYPE-VARIANCE-POSITIONS] — output positions

plain_cases! {
    /// A field is an OUTPUT position, so `out T` belongs there.
    out_is_legal_in_a_field: accepts(Flavor::Default, decl("<out T>", "supply", "T"));

    /// A function RESULT is an output position, so `out T` belongs there too.
    out_is_legal_in_a_function_result: accepts(Flavor::Default, decl("<out T>", "make", "(int) -> T"));

    /// A function PARAMETER flips the polarity to input: `out T` is rejected.
    out_in_a_function_parameter_is_rejected: rejects_with(Flavor::Default, decl("<out T>", "consume", "(T) -> int"), position_message("T", "out", "input", "consume", "Holder"));

    /// Two flips compose back to an output position, so `out T` is legal again.
    out_under_two_parameter_flips_is_legal: accepts(Flavor::Default, decl("<out T>", "hof", "((T) -> int) -> int"));

    /// An `in` parameter belongs in an input position.
    in_is_legal_in_a_function_parameter: accepts(Flavor::Default, decl("<in T>", "admit", "(T) -> bool"));

    /// A field is an output position, so `in T` is rejected there.
    in_in_a_field_is_rejected: rejects_with(Flavor::Default, decl("<in T>", "held", "T"), position_message("T", "in", "output", "held", "Holder"));

    /// A function result is an output position, so `in T` is rejected there.
    in_in_a_function_result_is_rejected: rejects_with(Flavor::Default, decl("<in T>", "make", "(int) -> T"), position_message("T", "in", "output", "make", "Holder"));

    /// Two flips compose back to output, so `in T` is rejected under them.
    in_under_two_parameter_flips_is_rejected: rejects_with(Flavor::Default, decl("<in T>", "hof", "((T) -> int) -> int"), position_message("T", "in", "output", "hof", "Holder"));

    /// An unannotated parameter is invariant and sits in EITHER position.
    an_invariant_parameter_is_legal_in_both_positions:
        accepts(Flavor::Default, decl("<T>", "held", "T")),
        accepts(Flavor::Default, decl("<T>", "consume", "(T) -> int"));
}

// [TYPE-VARIANCE-POSITIONS] — composition through a nested constructor

plain_cases! {
    /// A covariant argument of a covariant constructor keeps the position.
    out_inside_a_covariant_constructor_argument_is_legal: accepts(Flavor::Default, nested("type Feed<out A> = { supply: A }", "<out T>", "Feed<T>"));

    /// A covariant argument of a CONTRAVARIANT constructor flips the position.
    out_inside_a_contravariant_constructor_argument_is_rejected: rejects_with(Flavor::Default, nested("type Gate<in A> = { admit: (A) -> bool }", "<out T>", "Gate<T>"), position_message("T", "out", "input", "nested", "Holder"));

    /// The mirror composes the other way and is LEGAL: a field is an output
    /// position, the contravariant argument flips it to input, and an input
    /// position is exactly where `in T` belongs (output x contravariant = input).
    in_inside_a_contravariant_constructor_argument_is_legal: accepts(Flavor::Default, nested("type Gate<in A> = { admit: (A) -> bool }", "<in T>", "Gate<T>"));

    /// "an invariant argument position demands both directions, so only invariant
    /// parameters may sit there" — a covariant parameter may not.
    out_inside_an_invariant_constructor_argument_is_rejected: rejects_with(Flavor::Default, nested("type Cell<A> = { slot: A }", "<out T>", "Cell<T>"), position_message("T", "out", "invariant", "nested", "Holder"));

    /// …nor may a contravariant one.
    in_inside_an_invariant_constructor_argument_is_rejected: rejects_with(Flavor::Default, nested("type Cell<A> = { slot: A }", "<in T>", "Cell<T>"), position_message("T", "in", "invariant", "nested", "Holder"));

    /// An invariant parameter is exactly what an invariant argument accepts.
    an_invariant_parameter_inside_an_invariant_argument_is_legal: accepts(Flavor::Default, nested("type Cell<A> = { slot: A }", "<T>", "Cell<T>"));
}

// [TYPE-VARIANCE-ASSIGN] — the built-in variance table, checked by position

plain_cases! {
    /// `List<out T>`: a covariant parameter may sit in its argument.
    list_is_covariant_in_its_element: accepts(Flavor::Default, decl("<out T>", "items", "List<T>"));

    /// `Fiber<out T>`: covariant in its answer.
    fiber_is_covariant_in_its_answer: accepts(Flavor::Default, decl("<out T>", "worker", "Fiber<T>"));

    /// `Map<K, out V>`: the VALUE is covariant.
    map_is_covariant_in_its_value: accepts(Flavor::Default, decl("<out T>", "byName", "Map<string, T>"));

    /// `Map<K, out V>`: the KEY is invariant, so a covariant parameter is rejected.
    map_keys_are_invariant: rejects_with(Flavor::Default, decl("<out T>", "byKey", "Map<T, int>"), position_message("T", "out", "invariant", "byKey", "Holder"));

    /// A `List` of a contravariant parameter is an output position.
    in_inside_a_list_is_rejected: rejects_with(Flavor::Default, decl("<in T>", "items", "List<T>"), position_message("T", "in", "output", "items", "Holder"));

    /// `Result<out T, out E>`: both arguments are covariant.
    result_is_covariant_in_both_arguments:
        accepts(Flavor::Default, decl("<out T>", "value", "Result<T, string>")),
        accepts(Flavor::Default, decl("<out T>", "failure", "Result<int, T>"));

    /// `Channel<T>` is invariant in both directions.
    channel_is_invariant:
        rejects_with(Flavor::Default, decl("<out T>", "wire", "Channel<T>"), position_message("T", "out", "invariant", "wire", "Holder")),
        rejects_with(Flavor::Default, decl("<in T>", "wire", "Channel<T>"), position_message("T", "in", "invariant", "wire", "Holder"));
}

// [TYPE-VARIANCE-ASSIGN] — leaves match exactly, in every container

/// A `Result` payload is never extracted under a container, whatever the
/// declared variance: the stored representation differs.
fn feed_payload_program(marker: &str) -> String {
    format!(
        r#"type Feed<{marker}T> = Feed {{ supply: T }} | Dry
fn firstItem(f: Feed<int>) = match f {{
    Feed {{ supply }} => supply
    Dry => 0
}}
fn mkFeed() -> Feed<Result<int, string>> = Feed {{ supply: 20 * 5 }}
print("${{firstItem(mkFeed())}}")"#
    )
}

plain_cases! {
    /// Under `out T`.
    a_result_payload_does_not_collapse_under_a_covariant_container: rejects_with(Flavor::Default, feed_payload_program("out "), "cannot unify");

    /// Under an invariant parameter.
    a_result_payload_does_not_collapse_under_an_invariant_container: rejects_with(Flavor::Default, feed_payload_program(""), "cannot unify");

    /// "Function returns also match exactly, so a `Feed<(int) -> Result<int, Error>>`
    /// does not match a `Feed<(int) -> int>` slot" — the spec's own example.
    a_function_payloads_return_channel_matches_exactly: rejects_with(Flavor::Default, r#"type Feed<out T> = Feed { supply: T } | Dry
fn label(f: Feed<(int) -> int>) = match f {
    Feed { supply } => "supplied"
    Dry => "dry"
}
fn mkFeed() -> Feed<(int) -> Result<int, string>> = Feed { supply: |n| => n * 2 }
print(label(mkFeed()))"#, "cannot unify");

    /// The direct value site keeps the one safe promotion `T -> Result<T, E>`,
    /// which is what makes the container rejections above a real restriction rather
    /// than a blanket ban.
    the_direct_site_promotion_still_holds: accepts(Flavor::Default, r#"fn keep(n: Result<int, string>) = n ?: 0
print("${keep(5)}")"#);
}

// [TYPE-VARIANCE-DECL] — where the markers may be written at all

plain_cases! {
    /// "`out` and `in` are contextual keywords, reserved only inside
    /// type-parameter lists" — they stay usable as ordinary names.
    out_and_in_stay_legal_identifiers_outside_a_parameter_list: accepts(Flavor::Default, r#"let out = 1
let inn = 2
fn keep(out) = out
print("${keep(out)} ${inn}")"#);

    /// Variance is declaration-site on TYPES and EFFECTS only — never on a
    /// function binder ([TYPE-GENERICS-FN]).
    a_variance_marker_on_a_function_binder_is_rejected:
        rejects_with(Flavor::Default, fn_binder("out"), "variance annotations are only valid on type and effect declarations"),
        rejects_with(Flavor::Default, fn_binder("in"), "variance annotations are only valid on type and effect declarations");
}

// [FLAVOR-ML-GENERICS] — the same rules through the ML spellings

plain_cases! {
    /// "Types use juxtaposed binders: `type Box T`, `type Feed out T`,
    /// `type Sink in T`."
    ml_variance_binders_check_the_same_positions:
        accepts(Flavor::Ml, ml_decl("Feed", "out", "supply", "T")),
        rejects_with(Flavor::Ml, ml_decl("Bad", "out", "consume", "T -> int"), position_message("T", "out", "input", "consume", "Bad"));

    /// The ML twin of the contravariant rules.
    ml_contravariant_binders_check_the_same_positions:
        accepts(Flavor::Ml, ml_decl("Gate", "in", "admit", "T -> bool")),
        rejects_with(Flavor::Ml, ml_decl("Bad", "in", "held", "T"), position_message("T", "in", "output", "held", "Bad"));

    /// ML function binders reject variance exactly as Default's do.
    ml_a_variance_marker_on_a_function_binder_is_rejected: rejects_with(Flavor::Ml, "pick<out T> : (T, T) -> T\npick (first, second) = first\nkept = pick (1, 2)\nprint \"${kept}\"\n", "variance annotations are only valid on type and effect declarations");
}
