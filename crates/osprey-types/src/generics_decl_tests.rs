//! Spec-driven assertions for **declared generics**: the binder and the
//! construction site ([TYPE-GENERICS-DECL], [GENERICS-CTOR-ARITY],
//! [TYPE-GENERICS-FN], docs/specs/0004-TypeSystem.md; ML spellings in
//! [FLAVOR-ML-GENERICS], docs/specs/0024-MLFlavorSyntax.md).
//!
//! The call site is stated separately in `generics_apply_tests.rs`; this module
//! covers the two positions that already ship, and the permutations of each
//! sentence the spec writes about them.

use crate::testutil::{accepts, ctor_arity_message, rejected_somehow, rejects_with};
use osprey_syntax::Flavor;

/// One binder, one field.
const BOX: &str = "type Box<T> = { value: T }\n";
/// Two binders across two fields — order is observable.
const PAIR: &str = "type Pair<T, U> = { first: T, second: U }\n";

// ---------------------------------------------------------------------------
// [TYPE-GENERICS-DECL] — binders span every variant field
// ---------------------------------------------------------------------------

/// "`type Pair<T, U> = …` binds `T`/`U` across every variant field."
#[test]
fn a_binder_spans_every_variant_field() {
    accepts(
        Flavor::Default,
        r#"type Holder<T, U> = Full { left: T, right: U } | Half { only: T } | None
fn label(h: Holder<int, string>) = match h {
    Full { left, right } => right
    Half { only } => "half"
    None => "none"
}
print(label(Full { left: 1, right: "one" }))"#,
    );
}

/// One binder used twice in one variant pins both fields to one type.
#[test]
fn one_binder_used_twice_pins_both_fields() {
    rejects_with(
        Flavor::Default,
        r#"type Both<T> = { left: T, right: T }
let b = Both { left: 1, right: "two" }
print("${b.left}")"#,
        "cannot unify",
    );
}

/// "A construction site may apply explicit type arguments … which unify with
/// the instantiation the fields would otherwise infer."
#[test]
fn a_construction_site_may_pin_its_instantiation() {
    accepts(
        Flavor::Default,
        &format!(
            r#"{PAIR}let p = Pair<int, string> {{ first: 1, second: "a" }}
print("${{p.first}}${{p.second}}")"#
        ),
    );
}

/// "an argument that contradicts a field is a type error."
#[test]
fn a_construction_type_argument_contradicting_a_field_is_rejected() {
    rejects_with(
        Flavor::Default,
        &format!(
            r#"{BOX}let b = Box<int> {{ value: "text" }}
print("${{b.value}}")"#
        ),
        "cannot unify",
    );
}

/// The contradiction is caught in the second position as well.
#[test]
fn a_contradiction_in_the_second_type_argument_is_rejected() {
    rejects_with(
        Flavor::Default,
        &format!(
            r#"{PAIR}let p = Pair<int, int> {{ first: 1, second: "a" }}
print("${{p.first}}")"#
        ),
        "cannot unify",
    );
}

/// A nested instantiation is pinned the same way.
#[test]
fn a_nested_construction_type_argument_is_checked() {
    accepts(
        Flavor::Default,
        &format!(
            r#"{BOX}let nested = Box<Box<int>> {{ value: Box<int> {{ value: 7 }} }}
print("${{nested.value.value}}")"#
        ),
    );
}

// ---------------------------------------------------------------------------
// [GENERICS-CTOR-ARITY] — the count is a contract with the declaration
// ---------------------------------------------------------------------------

/// "`Box<int, string> { v: 1 }` is rejected with `takes 1 type argument(s), got 2`."
#[test]
fn too_many_construction_type_arguments_are_rejected() {
    rejects_with(
        Flavor::Default,
        &format!(
            r#"{BOX}let b = Box<int, string> {{ value: 1 }}
print("${{b.value}}")"#
        ),
        &ctor_arity_message("Box", 1, 2),
    );
}

/// The other direction: two binders, one written argument.
#[test]
fn too_few_construction_type_arguments_are_rejected() {
    rejects_with(
        Flavor::Default,
        &format!(
            r#"{PAIR}let p = Pair<int> {{ first: 1, second: "a" }}
print("${{p.first}}")"#
        ),
        &ctor_arity_message("Pair", 2, 1),
    );
}

/// A type that declares NO binders accepts no type arguments.
#[test]
fn a_non_generic_type_takes_no_construction_type_arguments() {
    rejects_with(
        Flavor::Default,
        r#"type Plain = { value: int }
let p = Plain<int> { value: 1 }
print("${p.value}")"#,
        &ctor_arity_message("Plain", 0, 1),
    );
}

/// Omitting the arguments entirely stays legal — they are optional, and
/// inference supplies them.
#[test]
fn omitting_construction_type_arguments_still_infers() {
    accepts(
        Flavor::Default,
        &format!(
            r#"{BOX}let n = Box {{ value: 1 }}
let s = Box {{ value: "one" }}
print("${{n.value}}${{s.value}}")"#
        ),
    );
}

// ---------------------------------------------------------------------------
// [TYPE-GENERICS-FN] — what a function binder means
// ---------------------------------------------------------------------------

/// "A binder makes every use of `T` in the signature the SAME inference
/// variable" — so two arguments at different types are rejected.
#[test]
fn one_function_binder_relates_two_parameters() {
    accepts(
        Flavor::Default,
        r#"fn pick<T>(first: T, second: T) -> T = first
print("${pick(10, 20)} ${pick("left", "right")}")"#,
    );
    rejects_with(
        Flavor::Default,
        r#"fn pick<T>(first: T, second: T) -> T = first
print("${pick(10, "twenty")}")"#,
        "cannot unify",
    );
}

/// "without it, `T` in an annotation names a nominal type" — an undeclared
/// nominal name is not a type.
#[test]
fn an_unbound_annotation_name_is_nominal_not_a_binder() {
    rejected_somehow(
        Flavor::Default,
        r#"fn nom(a: T) -> T = a
print("${nom(1)}")"#,
    );
}

/// …and when that nominal name IS declared, it binds nothing: the function is
/// monomorphic in it.
#[test]
fn a_declared_nominal_name_makes_the_function_monomorphic() {
    accepts(
        Flavor::Default,
        r#"type Tag = { id: int }
fn nom(a: Tag) -> Tag = a
print("${nom(Tag { id: 1 }).id}")"#,
    );
    rejects_with(
        Flavor::Default,
        r#"type Tag = { id: int }
fn nom(a: Tag) -> Tag = a
print("${nom(1)}")"#,
        "cannot unify",
    );
}

/// "HM inference is unchanged: unannotated functions stay implicitly
/// polymorphic."
#[test]
fn an_unannotated_function_stays_implicitly_polymorphic() {
    accepts(
        Flavor::Default,
        r#"fn id(x) = x
print("${id(5)} ${id("os")} ${toString(id(true))}")"#,
    );
}

/// Two binders are independent: they may be instantiated at the same type or
/// at different ones.
#[test]
fn two_binders_are_independent() {
    accepts(
        Flavor::Default,
        r#"fn pair<T, U>(first: T, second: U) -> T = first
print("${pair(1, "a")} ${pair(1, 2)}")"#,
    );
}

/// A binder list names DISTINCT parameters; a repeat has no meaning.
#[test]
fn a_repeated_binder_name_is_rejected() {
    rejected_somehow(
        Flavor::Default,
        r#"fn dup<T, T>(first: T, second: T) -> T = first
print("${dup(1, 2)}")"#,
    );
}

/// An empty binder list is not a binder list.
#[test]
fn an_empty_binder_list_is_rejected() {
    rejected_somehow(
        Flavor::Default,
        r#"fn none<>(x) = x
print("${none(1)}")"#,
    );
}

/// A binder may appear in the return position only — the shape call-site type
/// application exists to serve ([TYPE-GENERICS-APPLY]).
#[test]
fn a_binder_may_appear_in_the_return_position_only() {
    accepts(
        Flavor::Default,
        r#"fn empty<T>() -> List<T> = []
let held: List<int> = empty()
print("${length(held)}")"#,
    );
}

// ---------------------------------------------------------------------------
// [TYPE-GENERICS-FN] — a generic function as a VALUE
// ---------------------------------------------------------------------------

/// "specialised wherever its ABI can be fixed: by a consuming slot, by a call
/// alias" — the alias form.
#[test]
fn a_generic_function_binds_as_a_call_alias() {
    accepts(
        Flavor::Default,
        r#"fn identity<T>(x: T) -> T = x
let g = identity
print("${g(5)} ${g("os")}")"#,
    );
}

/// …and the consuming-slot form: a generic function passed by name to a HOF.
#[test]
fn a_generic_function_specialises_into_a_consuming_slot() {
    accepts(
        Flavor::Default,
        r#"fn identity<T>(x: T) -> T = x
fn also(x, f) = f(x)
print("${also(5, identity)} ${also("os", identity)}")"#,
    );
}

/// "`fn pick() = |x| => x` followed by `let f = pick()` therefore serves
/// `f(7)`, `f("os")` and `f(2.5)` from one binding."
#[test]
fn a_returned_generic_lambda_serves_every_call_site_of_its_binding() {
    accepts(
        Flavor::Default,
        r#"fn mk() = |x| => x
let f = mk()
print("${f(7)} ${f("os")} ${f(2.5)}")"#,
    );
}

/// "A lambda so returned may close over the producing call's parameters."
#[test]
fn a_returned_lambda_may_close_over_the_producing_call() {
    accepts(
        Flavor::Default,
        r#"fn constly(v) = |x| => v
let always = constly(9)
print("${always(1)} ${always("os")}")"#,
    );
}

/// "a still-generic lambda used as a bare value … is rejected rather than
/// guessed", with the compiler's own sentence.
#[test]
fn a_still_generic_lambda_as_a_bare_value_is_rejected() {
    rejects_with(
        Flavor::Default,
        r#"fn mk(x) = |y| => x
print("${mk(1)}")"#,
        "a closure value with a still-generic type",
    );
}

// ---------------------------------------------------------------------------
// [FLAVOR-ML-GENERICS] — the ML spellings of the same declarations
// ---------------------------------------------------------------------------

/// "Types use juxtaposed binders: `type Box T`."
#[test]
fn ml_type_binders_are_juxtaposed() {
    accepts(
        Flavor::Ml,
        "type Box T =\n    value : T\nheld = Box(value = 7)\nprint \"${held.value}\"\n",
    );
}

/// "Construction-site type arguments use `Box<int>(item = 7)`."
#[test]
fn ml_construction_type_arguments_use_angles() {
    accepts(
        Flavor::Ml,
        "type Box T =\n    value : T\nheld = Box<int>(value = 7)\nprint \"${held.value}\"\n",
    );
}

/// The ML construction site is held to the same arity contract.
#[test]
fn ml_construction_type_argument_arity_is_checked() {
    rejects_with(
        Flavor::Ml,
        "type Box T =\n    value : T\nheld = Box<int, string>(value = 7)\nprint \"${held.value}\"\n",
        &ctor_arity_message("Box", 1, 2),
    );
}

/// "Function binders appear on a signature: `pick<T> : (T, T) -> T`."
#[test]
fn ml_function_binders_live_on_the_signature() {
    accepts(
        Flavor::Ml,
        "pick<T> : (T, T) -> T\npick (first, second) = first\nkept = pick (10, 20)\nprint \"${kept}\"\n",
    );
}

/// "A binding without a signature cannot declare function type parameters."
#[test]
fn ml_a_binding_cannot_declare_type_parameters_without_a_signature() {
    rejected_somehow(
        Flavor::Ml,
        "pick<T> (first, second) = first\nkept = pick (10, 20)\nprint \"${kept}\"\n",
    );
}

/// The ML twin of the one-binder-relates-two-parameters rule.
#[test]
fn ml_one_binder_relates_two_parameters() {
    rejects_with(
        Flavor::Ml,
        "pick<T> : (T, T) -> T\npick (first, second) = first\nkept = pick (10, \"twenty\")\nprint \"${kept}\"\n",
        "cannot unify",
    );
}
