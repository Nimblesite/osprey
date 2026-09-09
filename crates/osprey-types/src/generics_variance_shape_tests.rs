//! Variance position checking over the SHAPES a declaration can take
//! ([TYPE-VARIANCE-POSITIONS], docs/specs/0004-TypeSystem.md): positions reached
//! through the built-in constructors, a parameter used in both directions at
//! once, several parameters with mixed markers, and union variants — named and
//! positional.
//!
//! The single-field core matrix is `generics_variance_tests.rs`, whose `decl`
//! and `position_message` helpers this module reuses.

use crate::generics_variance_tests::{decl, position_message};
use crate::testutil::{accepts, check, rejected_somehow, rejects_with};
use osprey_syntax::Flavor;

/// A function type inside a covariant built-in still flips its parameter.
#[test]
fn out_in_a_function_parameter_inside_a_list_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<out T>", "checks", "List<(T) -> int>"),
        &position_message("T", "out", "input", "checks", "Holder"),
    );
}

/// The `Result` ERROR channel is an output position too.
#[test]
fn out_is_legal_in_the_result_error_channel() {
    accepts(Flavor::Default, &decl("<out T>", "failure", "Result<int, T>"));
}

/// …so a contravariant parameter is rejected there.
#[test]
fn in_in_the_result_error_channel_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "failure", "Result<int, T>"),
        &position_message("T", "in", "output", "failure", "Holder"),
    );
}

/// `Fiber<out T>` is an output position, so `in T` cannot sit there.
#[test]
fn in_inside_a_fiber_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "worker", "Fiber<T>"),
        &position_message("T", "in", "output", "worker", "Holder"),
    );
}

/// `Map<K, out V>`: a contravariant parameter is rejected in the VALUE channel.
#[test]
fn in_in_a_map_value_is_rejected() {
    rejects_with(
        Flavor::Default,
        &decl("<in T>", "byName", "Map<string, T>"),
        &position_message("T", "in", "output", "byName", "Holder"),
    );
}

/// …and in the invariant KEY channel, both markers are refused.
#[test]
fn map_keys_refuse_both_markers() {
    rejected_somehow(Flavor::Default, &decl("<out T>", "byKey", "Map<T, int>"));
    rejected_somehow(Flavor::Default, &decl("<in T>", "byKey", "Map<T, int>"));
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-POSITIONS] — a parameter used in BOTH positions
// ---------------------------------------------------------------------------

/// A record that both holds and consumes its parameter demands both
/// directions, so only an invariant parameter fits.
fn both_positions(params: &str) -> String {
    format!("type Holder{params} = {{{{ held: T, consume: (T) -> int }}}}\nprint(\"declared\")")
}

/// The invariant spelling is accepted.
#[test]
fn a_parameter_used_in_both_positions_may_be_invariant() {
    accepts(Flavor::Default, &both_positions("<T>"));
}

/// The covariant spelling is rejected on the input half.
#[test]
fn a_parameter_used_in_both_positions_may_not_be_covariant() {
    rejects_with(
        Flavor::Default,
        &both_positions("<out T>"),
        &position_message("T", "out", "input", "consume", "Holder"),
    );
}

/// The contravariant spelling is rejected on the output half.
#[test]
fn a_parameter_used_in_both_positions_may_not_be_contravariant() {
    rejects_with(
        Flavor::Default,
        &both_positions("<in T>"),
        &position_message("T", "in", "output", "held", "Holder"),
    );
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-DECL] — several parameters, each checked on its own
// ---------------------------------------------------------------------------

/// Mixed markers on one declaration are checked independently.
#[test]
fn mixed_markers_are_checked_per_parameter() {
    accepts(
        Flavor::Default,
        "type Two<out A, in B> = { give: A, take: (B) -> bool }\nprint(\"declared\")",
    );
}

/// Swapping the two markers puts each parameter in the wrong position.
#[test]
fn swapped_markers_are_rejected_per_parameter() {
    let errs = check(
        "type Two<in A, out B> = { give: A, take: (B) -> bool }\nprint(\"declared\")",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains(&position_message("A", "in", "output", "give", "Two"))),
        "expected the `A` violation: {errs:?}"
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains(&position_message("B", "out", "input", "take", "Two"))),
        "expected the `B` violation: {errs:?}"
    );
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-POSITIONS] — union variants, including positional payloads
// ---------------------------------------------------------------------------

/// A union variant's payload is an output position.
#[test]
fn out_is_legal_in_a_union_variant_payload() {
    accepts(
        Flavor::Default,
        "type Maybe<out T> = Some { value: T } | None\nprint(\"declared\")",
    );
}

/// …so a contravariant parameter is rejected there.
#[test]
fn in_in_a_union_variant_payload_is_rejected() {
    rejects_with(
        Flavor::Default,
        "type Maybe<in T> = Some { value: T } | None\nprint(\"declared\")",
        &position_message("T", "in", "output", "value", "Maybe"),
    );
}

/// A POSITIONAL payload is the same output position under a different spelling
/// ([TYPE-UNION-POSITIONAL]).
#[test]
fn a_positional_variant_payload_is_an_output_position() {
    accepts(
        Flavor::Default,
        "type Maybe<out T> = Some(T) | None\nprint(\"declared\")",
    );
    rejected_somehow(
        Flavor::Default,
        "type Maybe<in T> = Some(T) | None\nprint(\"declared\")",
    );
}

/// Every variant is walked, not just the first.
#[test]
fn a_violation_in_a_later_variant_is_still_found() {
    rejects_with(
        Flavor::Default,
        "type Split<out T> = First { ok: T } | Second { consume: (T) -> int } | Empty\nprint(\"declared\")",
        &position_message("T", "out", "input", "consume", "Split"),
    );
}
