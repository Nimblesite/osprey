//! The ML half of **call-site type application** ([TYPE-GENERICS-APPLY],
//! docs/specs/0004-TypeSystem.md; spelling in [FLAVOR-ML-GENERICS],
//! docs/specs/0024-MLFlavorSyntax.md).
//!
//! Both surfaces lower to the same canonical node ([FLAVOR-BOUNDARY]), so every
//! Default assertion in `generics_apply_tests.rs` has a twin here and the two
//! flavors must agree down to the diagnostic sentence.

use crate::testutil::{accepts, fn_arity_message, rejected_somehow, rejects_with};
use osprey_syntax::Flavor;

/// The ML twin of `IDENTITY`: the binder lives on the signature line.
const ML_IDENTITY: &str = "identity<T> : T -> T\nidentity x = x\n";
/// The ML twin of `PICK`.
const ML_PICK: &str = "pick<T, U> : (T, U) -> T\npick (first, second) = first\n";

/// "Call-site type arguments attach to the callee name and precede the
/// juxtaposed argument: `identity<int> 5`."
#[test]
fn ml_type_application_pins_a_binder() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<int> 5
print "${{pinned}}""#
        ),
    );
}

/// The ML twin of the no-parameter-position case.
#[test]
fn ml_type_application_pins_a_binder_no_argument_mentions() {
    accepts(
        Flavor::Ml,
        r#"empty<T> : Unit -> List<T>
empty () = []
held = empty<int> ()
print "${length held}""#,
    );
}

/// Parenthesised ML argument tuples take type arguments too.
#[test]
fn ml_type_application_binds_binders_positionally() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_PICK}kept = pick<int, string> (1, "two")
print "${{kept}}""#
        ),
    );
}

/// The swapped ML twin must be rejected.
#[test]
fn ml_swapped_type_arguments_are_rejected() {
    rejects_with(
        Flavor::Ml,
        &format!(
            r#"{ML_PICK}kept = pick<string, int> (1, "two")
print "${{kept}}""#
        ),
        "cannot unify",
    );
}

/// The arity contract lives in the shared core, so ML reports the same
/// sentence as Default ([FLAVOR-BOUNDARY]).
#[test]
fn ml_type_application_arity_is_checked() {
    rejects_with(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<int, string> 5
print "${{pinned}}""#
        ),
        &fn_arity_message("identity", 1, 2),
    );
}

/// "A binding without a signature cannot declare function type parameters" —
/// and so cannot be applied at a call site either.
#[test]
fn ml_a_binding_without_a_signature_takes_no_type_arguments() {
    rejects_with(
        Flavor::Ml,
        r#"plain x = x
pinned = plain<int> 5
print "${pinned}""#,
        &fn_arity_message("plain", 0, 1),
    );
}

/// A nested ML type argument closes with `>>` here too.
#[test]
fn ml_a_type_argument_may_itself_be_generic() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}held = identity<List<int>> [1, 2]
print "${{length held}}""#
        ),
    );
}

/// The ML control: `<` stays comparison under layout juxtaposition.
#[test]
fn ml_a_comparison_is_not_type_application() {
    accepts(
        Flavor::Ml,
        r#"lower (a, b) = a < b
flag = lower (1, 2)
print "${toString flag}""#,
    );
}

/// The ML gap rule matches Default's: `identity <int> 5` is not application.
#[test]
fn ml_a_gap_before_the_angle_is_not_type_application() {
    rejected_somehow(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity <int> 5
print "${{pinned}}""#
        ),
    );
}

/// ML variance markers are declaration-site only, at the call site as well.
#[test]
fn ml_a_variance_marker_at_a_call_site_is_rejected() {
    rejected_somehow(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<out int> 5
print "${{pinned}}""#
        ),
    );
}

/// Curry-by-default: the written arguments attach to the callee name, so a
/// curried application takes them exactly once, at the front.
#[test]
fn ml_type_application_survives_curried_application() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_PICK}kept = pick<int, string> 1 "two"
print "${{kept}}""#
        ),
    );
}

/// The ML twin of the value-argument contradiction.
#[test]
fn ml_type_application_contradicting_the_value_argument_is_rejected() {
    rejects_with(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<int> "text"
print "${{pinned}}""#
        ),
        "cannot unify int with string",
    );
}

/// The ML twin of the expected-type contradiction, pinned by a signature.
#[test]
fn ml_type_application_contradicting_the_expected_type_is_rejected() {
    rejects_with(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned : string
pinned = identity<int> 5
print "${{pinned}}""#
        ),
        "cannot unify",
    );
}

/// The ML twin of "too few type arguments".
#[test]
fn ml_too_few_type_arguments_are_rejected_by_count() {
    rejects_with(
        Flavor::Ml,
        &format!(
            r#"{ML_PICK}kept = pick<int> (1, "two")
print "${{kept}}""#
        ),
        &fn_arity_message("pick", 2, 1),
    );
}

/// The ML twin of the lambda rule: `\x => x` declares no binder.
#[test]
fn ml_a_lambda_binding_takes_no_type_arguments() {
    rejects_with(
        Flavor::Ml,
        r#"f = \x => x
pinned = f<int> 5
print "${pinned}""#,
        &fn_arity_message("f", 0, 1),
    );
}

/// A declared record is a type argument in ML too.
#[test]
fn ml_a_declared_record_may_be_a_type_argument() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"type Box T =
    value : T
{ML_IDENTITY}held = identity<Box<int>> (Box(value = 7))
print "${{held.value}}""#
        ),
    );
}

/// A function type is a type argument in ML too.
#[test]
fn ml_a_function_type_may_be_a_type_argument() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}double n = n * 2 ?: 0
f = identity<(int) -> int> double
print "${{f 21}}""#
        ),
    );
}

/// Two written instantiations of one ML binding coexist.
#[test]
fn ml_two_written_instantiations_coexist() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}n = identity<int> 5
s = identity<string> "os"
print "${{n}} ${{s}}""#
        ),
    );
}

/// Written arguments do not excuse a wrong value argument: applying `Unit`
/// where `int` was pinned still fails. The layer that catches it is not pinned
/// — under curry-by-default this reads as either an arity or a unification
/// failure, and both are truthful rejections.
#[test]
fn ml_writing_type_arguments_does_not_excuse_the_value_argument() {
    rejected_somehow(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<int> ()
print "${{pinned}}""#
        ),
    );
}

/// A pipe target has no juxtaposed argument, so it is not an application site.
#[test]
fn ml_type_arguments_in_a_pipe_target_are_rejected() {
    rejected_somehow(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = 5 |> identity<int>
print "${{pinned}}""#
        ),
    );
}

/// An undeclared type name is not a type argument.
#[test]
fn ml_an_undeclared_type_argument_is_rejected() {
    rejected_somehow(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<Nope> 5
print "${{pinned}}""#
        ),
    );
}

/// An empty argument list is not a type list in ML either.
#[test]
fn ml_an_empty_type_argument_list_is_rejected() {
    rejected_somehow(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}pinned = identity<> 5
print "${{pinned}}""#
        ),
    );
}

/// Type application inside a lambda body and a match arm.
#[test]
fn ml_type_application_works_inside_lambdas_and_match_arms() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"{ML_IDENTITY}wrap = \n => identity<int> n
chosen = match wrap 1
    1 => identity<string> "one"
    _ => identity<string> "other"
print "${{chosen}}""#
        ),
    );
}

/// Call-site application coexists with an inferred generic effect in ML too.
#[test]
fn ml_type_application_coexists_with_generic_effect_instantiation() {
    accepts(
        Flavor::Ml,
        &format!(
            r#"effect Stash T
    take : Unit => T
{ML_IDENTITY}main () =
    held = handle Stash
        take => identity<int> 9
    in perform Stash.take ()
    print "${{held}}""#
        ),
    );
}
