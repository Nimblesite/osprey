//! Spec-driven assertions for **call-site type application**
//! ([TYPE-GENERICS-APPLY], docs/specs/0004-TypeSystem.md; the ML spelling is
//! [FLAVOR-ML-GENERICS], docs/specs/0024-MLFlavorSyntax.md).
//!
//! Type arguments have three positions in the language: the declaration binder
//! (`fn pick<T, U>`), the construction site (`Box<int> { … }`) and the call
//! site. Written arguments pin binders directly, including when value
//! arguments provide no constraint. An expected result type can also pin a
//! binder appearing in the return; an unconstrained immutable binding generalizes.
//! Tracked by plan 0015.
//!
//! Each test states one spec sentence and drives the compiler's public surface
//! — parse, then check — so a snippet the PARSER rejects is reported as a
//! syntax failure instead of being mistaken for a checker rejection.

use crate::testutil::{fn_arity_message, spec_cases};

/// `fn identity<T>` — one binder, mentioned by the parameter and the return.
const IDENTITY: &str = "fn identity<T>(x: T) -> T = x\n";
/// `fn pick<T, U>` — two binders, so written ORDER is observable.
const PICK: &str = "fn pick<T, U>(first: T, second: U) -> T = first\n";
/// A binder absent from parameters: written arguments or result context can pin it.
const EMPTY: &str = "fn empty<T>() -> List<T> = []\n";

// ---------------------------------------------------------------------------
// The accepted form — Default flavor
// ---------------------------------------------------------------------------

spec_cases! {
    /// "`identity<int>(5)` pins the callee's declared binders positionally."
    type_application_pins_a_binder_at_a_call_site: accepts(Default, format!(r#"{IDENTITY}print("${{identity<int>(5)}}")"#));

    /// Written arguments pin a binder absent from parameters when nothing else
    /// in the program constrains it.
    type_application_pins_a_binder_no_argument_mentions: accepts(Default, format!(r#"{EMPTY}print("${{length(empty<int>())}}")"#));

    /// An immutable binding generalizes an unpinned element type. Reading the
    /// list's length does not require a concrete element instantiation.
    an_unpinned_binder_generalizes_at_an_immutable_binding: accepts(Default, format!(
            r#"{EMPTY}let held = empty()
print("${{length(held)}}")"#
        ));

    /// "the written arguments unify with the instantiation the value arguments and
    /// the expected type would otherwise infer" — agreement is accepted.
    type_application_agreeing_with_the_value_argument_is_accepted: accepts(Default, format!(
            r#"{IDENTITY}let n: int = identity<int>(5)
print("${{n}}")"#
        ));

    /// Binders are pinned LEFT TO RIGHT.
    type_application_binds_binders_positionally: accepts(Default, format!(
            r#"{PICK}let kept = pick<int, string>(1, "two")
print("${{kept}}")"#
        ));

    /// The swapped twin of the case above must NOT type-check.
    swapped_type_arguments_are_rejected: rejects_with(Default, format!(
            r#"{PICK}let kept = pick<string, int>(1, "two")
print("${{kept}}")"#
        ), "cannot unify");

    /// A type argument may itself be generic: the `>>` closing two argument lists
    /// must lex as two closers, not as one shift operator.
    a_type_argument_may_itself_be_generic: accepts(Default, format!(r#"{IDENTITY}print("${{length(identity<List<int>>([1, 2]))}}")"#));

    /// Three closers in a row (`>>>`), and a two-argument constructor inside.
    a_type_argument_may_nest_two_levels_deep: accepts(Default, format!(
            r#"{IDENTITY}let groups = identity<Map<string, List<int>>>({{"a": [1]}})
print("${{mapLength(groups)}}")"#
        ));

    /// A declared record is as good a type argument as a primitive.
    a_declared_record_may_be_a_type_argument: accepts(Default, format!(
            r#"type Box<T> = {{ value: T }}
{IDENTITY}let boxed = identity<Box<int>>(Box {{ value: 7 }})
print("${{boxed.value}}")"#
        ));

    /// A function type is a type, so it may be written as a type argument.
    a_function_type_may_be_a_type_argument: accepts(Default, format!(
            r#"{IDENTITY}fn double(n) = n * 2 ?: 0
let f = identity<(int) -> int>(double)
print("${{f(21)}}")"#
        ));

    /// `Result<T, E>` is a type argument like any other — and writing it does NOT
    /// unwrap the failure channel ([TYPE-VARIANCE-ASSIGN]).
    a_result_type_may_be_a_type_argument: accepts(Default, format!(
            r#"{IDENTITY}let quotient = identity<Result<int, MathError>>(20 * 5)
print("${{quotient ?: 0}}")"#
        ));

    /// Type application is an expression: it nests inside another call.
    type_application_nests_inside_another_call: accepts(Default, format!(r#"{IDENTITY}print("${{identity<int>(identity<int>(5))}}")"#));

    /// Two written instantiations of ONE binding coexist — monomorphisation is
    /// per call site ([TYPE-GENERICS-FN]).
    two_written_instantiations_coexist_in_one_program: accepts(Default, format!(r#"{IDENTITY}print("${{identity<int>(5)}} ${{identity<string>("os")}}")"#));

    /// Named arguments are orthogonal to type arguments.
    type_application_composes_with_named_arguments: accepts(Default, format!(
            r#"{PICK}let kept = pick<int, string>(first: 1, second: "two")
print("${{kept}}")"#
        ));

    /// Receiver-first (UFCS) dispatch keeps the type arguments on the member name.
    type_application_composes_with_receiver_first_dispatch: accepts(Default, r#"fn headOr<T>(items: List<T>, fallback: T) -> T = items.listGet(0) ?: fallback
let first = [1, 2].headOr<int>(0)
print("${first}")"#);

    /// A generic HOF's binder may be pinned while the callback stays inferred.
    type_application_pins_a_generic_higher_order_call: accepts(Default, r#"fn also<T>(x: T, f: (T) -> T) -> T = f(x)
fn double(n) = n * 2 ?: 0
print("${also<int>(21, double)}")"#);

    /// The binder in scope may be applied to a RECURSIVE call inside the generic
    /// function's own body.
    a_binder_in_scope_may_be_applied_recursively: accepts(Default, r#"fn repeatOf<T>(value: T, times: int) -> T = match times {
    0 => value
    _ => repeatOf<T>(value, times - 1 ?: 0)
}
print("${repeatOf<int>(7, 3)}")"#);

    /// Inside a lambda body, and inside a match arm — no statement-level special
    /// case.
    type_application_works_inside_lambdas_and_match_arms: accepts(Default, format!(
            r#"{IDENTITY}let wrap = |n| => identity<int>(n)
let chosen = match wrap(1) {{
    1 => identity<string>("one")
    _ => identity<string>("other")
}}
print("${{chosen}}")"#
        ));

    /// Call-site application coexists with a generic effect whose instantiation is
    /// INFERRED — a written one is rejected on a dynamic effect ([STAGE-SIGNALS-EXACT]),
    /// so the `<` after a callee name must not be read as an effect mention.
    type_application_coexists_with_generic_effect_instantiation: accepts(Default, format!(
            r#"effect Stash<T> {{
    take: fn() -> T
}}
{IDENTITY}fn main() -> Unit = {{
    let held = handle Stash
        take => identity<int>(9)
    in perform Stash.take()
    print("${{held}}")
}}"#
        ));
}

// ---------------------------------------------------------------------------
// The rejections
// ---------------------------------------------------------------------------

spec_cases! {
    /// "A written argument that contradicts the value arguments … is a type error,
    /// not a silently ignored annotation."
    type_application_contradicting_the_value_argument_is_rejected: rejects_with(Default, format!(r#"{IDENTITY}print("${{identity<int>("text")}}")"#), "cannot unify int with string");

    /// A contradiction against the EXPECTED type is caught the same way, even
    /// though the value argument agrees with what was written.
    type_application_contradicting_the_expected_type_is_rejected: rejects_with(Default, format!(
            r"{IDENTITY}let s: string = identity<int>(5)
print(s)"
        ), "cannot unify");

    /// A contradiction one level down: the written `List<int>` disagrees with a
    /// `List<string>` value.
    a_nested_type_argument_contradiction_is_rejected: rejects_with(Default, format!(r#"{IDENTITY}print("${{length(identity<List<int>>(["a"]))}}")"#), "cannot unify");

    /// "The count must equal the callee's declared binder count."
    too_many_type_arguments_are_rejected_by_count: rejects_with(Default, format!(r#"{IDENTITY}print("${{identity<int, string>(5)}}")"#), fn_arity_message("identity", 1, 2));

    /// The same rule in the other direction: two binders, one written argument.
    too_few_type_arguments_are_rejected_by_count: rejects_with(Default, format!(
            r#"{PICK}let kept = pick<int>(1, "two")
print("${{kept}}")"#
        ), fn_arity_message("pick", 2, 1));

    /// "A callee that declares no binders … takes no type arguments." An
    /// unannotated function is implicitly polymorphic, which is NOT a binder.
    a_function_without_a_binder_takes_no_type_arguments: rejects_with(Default, r#"fn plain(x) = x
print("${plain<int>(5)}")"#, fn_arity_message("plain", 0, 1));

    /// A lambda binding declares no binders either.
    a_lambda_binding_takes_no_type_arguments: rejects_with(Default, r#"let f = |x| => x
print("${f<int>(5)}")"#, fn_arity_message("f", 0, 1));

    /// A parameter holding a function value declares no binders.
    a_function_valued_parameter_takes_no_type_arguments: rejects_with(Default, r#"fn apply(f, x) = f<int>(x)
print("${apply(|n| => n, 5)}")"#, fn_arity_message("f", 0, 1));

    /// Value arity is still checked when type arguments are written: the two
    /// argument lists are independent contracts.
    writing_type_arguments_does_not_excuse_a_missing_value_argument: rejected_somehow(Default, format!(r#"{IDENTITY}print("${{identity<int>()}}")"#));

    /// "Variance markers are not permitted, exactly as on the binder itself."
    a_covariance_marker_at_a_call_site_is_rejected: rejected_somehow(Default, format!(r#"{IDENTITY}print("${{identity<out int>(5)}}")"#));

    /// The contravariant half of the same rule.
    a_contravariance_marker_at_a_call_site_is_rejected: rejected_somehow(Default, format!(r#"{IDENTITY}print("${{identity<in int>(5)}}")"#));

    /// A type argument names a TYPE. An undeclared name is not one.
    an_undeclared_type_argument_is_rejected: rejected_somehow(Default, format!(r#"{IDENTITY}print("${{identity<Nope>(5)}}")"#));

    /// An empty argument list is not a type list.
    an_empty_type_argument_list_is_rejected: rejected_somehow(Default, format!(r#"{IDENTITY}print("${{identity<>(5)}}")"#));

    /// "the matching `>` immediately precedes the call's argument list" — with no
    /// argument list there is no type application, so a bare `identity<int>` value
    /// is not the way to specialise a function value.
    type_arguments_without_an_argument_list_are_rejected: rejected_somehow(Default, format!(
            r#"{IDENTITY}let g = identity<int>
print("${{g(5)}}")"#
        ));

    /// "recognised when the `<` immediately follows the callee name" — a gap makes
    /// it the comparison operator again, and `identity <int> (5)` is then nonsense.
    a_gap_before_the_angle_is_not_type_application: rejected_somehow(Default, format!(r#"{IDENTITY}print("${{identity <int> (5)}}")"#));
}

// ---------------------------------------------------------------------------
// Positive controls — `<` stays the comparison operator everywhere else
// ---------------------------------------------------------------------------

spec_cases! {
    /// "Everywhere else `<` is the comparison operator, so `a < b` and
    /// `f(a) < g(b)` are unaffected." Green today; must stay green after.
    a_comparison_is_not_type_application: accepts(Default, format!(
            r#"{IDENTITY}fn lower(a, b) = a < b
print("${{toString(lower(1, 2))}} ${{toString(identity(3) < identity(4))}}")"#
        ));

    /// A parenthesised relational chain keeps working ("must parenthesise").
    a_parenthesised_relational_chain_still_checks: accepts(Default, r#"let flag = (1 < 2) == true
print("${toString(flag)}")"#);

    /// Inference without written arguments is untouched: one binder still serves
    /// two instantiations ([TYPE-GENERICS-FN]).
    omitting_type_arguments_still_infers_each_instantiation: accepts(Default, format!(r#"{IDENTITY}print("${{identity(5)}} ${{identity("os")}}")"#));
}
