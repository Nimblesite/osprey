//! Tests for [TYPE-ANNOTATION-REDUNDANT].
//!
//! Every assertion here pins the exact diagnostic text, not a substring: the
//! message names the declaration and prints the type the reader would get by
//! deleting the annotation, and a message that stops being that precise is a
//! regression even when it still contains the word "redundant".

use crate::redundant::{redundant_annotations, TypeWarning, REDUNDANT_ANNOTATION};

use osprey_ast::Position;
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Parse under `flavor` and collect the redundancy warnings. A snippet that
/// does not parse fails the test rather than scoring as "no warnings".
pub(crate) fn warnings(flavor: Flavor, src: &str) -> Vec<TypeWarning> {
    let parsed = parse_program_with_flavor(src, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} flavor rejected the snippet at parse time: {:?}\n{src}",
        parsed.errors
    );
    redundant_annotations(&parsed.program)
}

/// The warning messages under `flavor`, in report order.
pub(crate) fn messages(flavor: Flavor, src: &str) -> Vec<String> {
    warnings(flavor, src)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// Assert the exact set of messages `flavor` reports for `src`, in order.
pub(crate) fn reports(flavor: Flavor, src: &str, expected: &[&str]) {
    assert_eq!(messages(flavor, src), expected, "\n{src}");
}

/// Assert `src` is clean under `flavor` — every annotation in it is earning
/// its place, or there are none.
pub(crate) fn silent(flavor: Flavor, src: &str) {
    reports(flavor, src, &[]);
}

const GREET: &str = "fn greet(name: string) -> string = \"hi \" + name\n";

#[test]
fn a_redundant_parameter_and_return_are_both_named_exactly() {
    reports(
        Flavor::Default,
        GREET,
        &[
            "redundant type annotation on parameter `name` of `greet`: inference derives `string` without it",
            "redundant return type annotation on `greet`: inference derives `string` without it",
        ],
    );
}

#[test]
fn an_unannotated_function_reports_nothing() {
    silent(Flavor::Default, "fn greet(name) = \"hi \" + name\n");
}

#[test]
fn every_warning_carries_the_configurable_rule_identifier() {
    let raised = warnings(Flavor::Default, GREET);
    assert_eq!(raised.len(), 2, "{raised:?}");
    assert!(
        raised.iter().all(|w| w.rule == REDUNDANT_ANNOTATION),
        "{raised:?}"
    );
    assert_eq!(REDUNDANT_ANNOTATION, "redundant-annotation");
}

#[test]
fn a_warning_points_at_the_declaration_it_came_from() {
    let raised = warnings(Flavor::Default, &format!("\n\n{GREET}"));
    assert_eq!(
        raised.first().map(|w| w.position),
        Some(Some(Position { line: 3, column: 3 })),
        "{raised:?}"
    );
}

#[test]
fn a_redundant_let_annotation_is_reported_by_name() {
    reports(
        Flavor::Default,
        "let n: int = 42\n",
        &["redundant type annotation on `n`: inference derives `int` without it"],
    );
}

#[test]
fn an_empty_literal_annotation_is_load_bearing_and_kept() {
    silent(Flavor::Default, "let xs: List<int> = []\n");
}

#[test]
fn a_redundant_lambda_parameter_is_reported_against_the_lambda() {
    reports(
        Flavor::Default,
        "let exclaim = fn(s: string) => s + \"!\"\nlet r = exclaim(\"hi\")\n",
        &["redundant type annotation on parameter `s` of `<lambda>`: inference derives `string` without it"],
    );
}

#[test]
fn an_extern_declaration_is_never_reported() {
    // Implements [TYPE-ANNOTATION-REDUNDANT]: an `extern` has no body to infer
    // from, so its types are its declaration and can never be redundant.
    silent(
        Flavor::Default,
        "extern fn osprey_ffi_cell() -> Ptr\nlet c = osprey_ffi_cell()\n",
    );
}

#[test]
fn record_field_declarations_are_never_reported() {
    silent(
        Flavor::Default,
        "type Point = { x: int, y: int }\nlet p = Point { x: 1, y: 2 }\n",
    );
}

#[test]
fn an_ill_typed_program_reports_no_redundancy() {
    // Inference over a rejected program is a best-effort shape, so no
    // comparison against it would be trustworthy. The type errors come first.
    silent(
        Flavor::Default,
        "fn inc(x: int) -> int = x + 1\nlet r = inc(\"not an int\")\n",
    );
}

#[test]
fn a_redundant_generic_signature_reports_only_what_can_go_together() {
    // `-> T` is what keeps the declared binder `T` in use once `x: T` goes, so
    // the two cannot both be deleted and only the first is reported.
    reports(
        Flavor::Default,
        "fn identity<T>(x: T) -> T = x\nlet i = identity(42)\n",
        &["redundant type annotation on parameter `x` of `identity`: inference derives `T` without it"],
    );
}

#[test]
fn a_result_returning_annotation_that_matches_inference_is_still_redundant() {
    reports(
        Flavor::Default,
        "fn twice(n: int) -> Result<int, Error> = checkedMul(n, 2)\n",
        &[
            "redundant type annotation on parameter `n` of `twice`: inference derives `int` without it",
            "redundant return type annotation on `twice`: inference derives `Result<int, Error>` without it",
        ],
    );
}

#[test]
fn a_return_annotation_naming_the_wrong_error_type_is_a_type_error_not_a_warning() {
    // `checkedMul` yields `Result<int, Error>`. Naming `string` instead is a
    // mismatch the checker rejects, and a rejected program raises no warnings.
    silent(
        Flavor::Default,
        "fn twice(n: int) -> Result<int, string> = checkedMul(n, 2)\n",
    );
}

#[test]
fn each_flavor_reports_the_lines_that_flavor_lets_you_delete() {
    // Both surfaces agree the annotation is redundant and agree on the type
    // inference derives. They differ in how many warnings that is, because
    // they differ in how many things there are to delete: Default writes the
    // parameter and the return separately and either can go on its own, while
    // ML writes one header line that goes as a whole.
    reports(
        Flavor::Default,
        GREET,
        &[
            "redundant type annotation on parameter `name` of `greet`: inference derives `string` without it",
            "redundant return type annotation on `greet`: inference derives `string` without it",
        ],
    );
    reports(
        Flavor::Ml,
        "greet : string -> string\ngreet name = \"hi \" + name\n",
        &["redundant type signature on `greet`: inference derives `(string) -> string` without it"],
    );
}

#[test]
fn an_ml_module_body_repeating_its_signature_is_still_redundant() {
    // The `signature` block is the module's contract and is never reported.
    // A member that WRITES the same type again is judged like any other
    // function: signature elaboration marks the types it supplies
    // ([MODULES-SIGNATURE]), so a written duplicate is distinguishable from
    // the copy the compiler pushed onto an unannotated member.
    reports(
        Flavor::Ml,
        "signature Api\n    shout : string -> string\n\nmodule M : Api\n    shout : string -> string\n    shout s = s + \"!\"\n",
        &["redundant type signature on `shout`: inference derives `(string) -> string` without it"],
    );
}

#[test]
fn a_module_member_annotation_that_is_not_its_contract_is_still_reported() {
    // The carve-out is exactly the contract, not the whole module. A private
    // helper the signature never mentions is judged like any other function.
    reports(
        Flavor::Ml,
        "signature Api\n    shout : string -> string\n\nmodule M : Api\n    louder : string -> string\n    louder s = s + \"!!\"\n    shout s = louder s\n",
        &["redundant type signature on `louder`: inference derives `(string) -> string` without it"],
    );
}

#[test]
fn a_signature_block_alone_reports_nothing() {
    silent(
        Flavor::Ml,
        "signature Api\n    shout : string -> string\n\nmodule M : Api\n    shout s = s + \"!\"\n",
    );
}

#[test]
fn two_redundant_functions_are_reported_in_source_order() {
    reports(
        Flavor::Default,
        "fn first(a: string) -> string = a + \"1\"\nfn second(b: string) -> string = b + \"2\"\n",
        &[
            "redundant type annotation on parameter `a` of `first`: inference derives `string` without it",
            "redundant return type annotation on `first`: inference derives `string` without it",
            "redundant type annotation on parameter `b` of `second`: inference derives `string` without it",
            "redundant return type annotation on `second`: inference derives `string` without it",
        ],
    );
}

#[test]
fn a_nested_block_binding_is_reached() {
    reports(
        Flavor::Default,
        "fn run() -> string = {\n  let greeting: string = \"hi\"\n  greeting\n}\n",
        &[
            "redundant return type annotation on `run`: inference derives `string` without it",
            "redundant type annotation on `greeting`: inference derives `string` without it",
        ],
    );
}

#[test]
fn only_the_redundant_parameter_of_a_pair_is_reported() {
    // `label` is pinned by `+`; `count` only by `toString`, which takes any,
    // so its annotation is the thing keeping the signature concrete.
    reports(
        Flavor::Default,
        "fn tag(label: string, count: int) -> string = label + toString(count)\n",
        &[
            "redundant type annotation on parameter `label` of `tag`: inference derives `string` without it",
            "redundant return type annotation on `tag`: inference derives `string` without it",
        ],
    );
}

#[test]
fn a_nominal_record_parameter_annotation_is_kept_but_its_return_is_not() {
    // Erasing `p: Point` leaves a structural row that only needs an `x`, which
    // is a MORE GENERAL signature than the one written — so the parameter
    // annotation is doing work. The return type is derived either way.
    reports(
        Flavor::Default,
        "type Point = { x: int, y: int }\nfn px(p: Point) -> int = p.x\nlet v = px(Point { x: 1, y: 2 })\n",
        &["redundant return type annotation on `px`: inference derives `int` without it"],
    );
}

#[test]
fn a_union_typed_signature_is_reported_with_its_union_name() {
    reports(
        Flavor::Default,
        "type Color = Red | Green | Blue\nfn name(c: Color) -> string = match c {\n  Red => \"red\"\n  Green => \"green\"\n  Blue => \"blue\"\n}\nlet n = name(Red)\n",
        &[
            "redundant type annotation on parameter `c` of `name`: inference derives `Color` without it",
            "redundant return type annotation on `name`: inference derives `string` without it",
        ],
    );
}

#[test]
fn a_higher_order_parameter_annotation_is_judged_like_any_other() {
    let raised = messages(
        Flavor::Default,
        "fn twice(f: (int) -> int, x: int) -> int = f(f(x))\nfn inc(n: int) -> int = wrapAdd(n, 1)\nlet r = twice(inc, 1)\n",
    );
    assert!(
        raised.contains(
            &"redundant type annotation on parameter `x` of `twice`: inference derives `int` without it"
                .to_string()
        ),
        "{raised:?}"
    );
}

#[test]
fn a_mutable_binding_annotation_is_reported_like_an_immutable_one() {
    reports(
        Flavor::Default,
        "effect State { set: fn(int) -> Unit }\nfn main() -> Unit = {\n  mut cell: int = 0\n  handle State {\n      set value => { cell = value }\n  }\n  perform State.set(1)\n}\n",
        &[
            "redundant return type annotation on `main`: inference derives `Unit` without it",
            "redundant type annotation on `cell`: inference derives `int` without it",
        ],
    );
}

#[test]
fn an_ml_lambda_parameter_is_reported_against_the_lambda() {
    reports(
        Flavor::Ml,
        "exclaim = \\(s: string) => s + \"!\"\nr = exclaim \"hi\"\n",
        &["redundant type annotation on parameter `s` of `<lambda>`: inference derives `string` without it"],
    );
}

#[test]
fn a_namespaced_function_is_reported_by_its_source_name_not_its_symbol() {
    reports(
        Flavor::Ml,
        "namespace tax\n\nrate : string -> string\nrate band = band + \"%\"\n",
        &["redundant type signature on `rate`: inference derives `(string) -> string` without it"],
    );
}

#[test]
fn every_message_names_a_type_and_never_leaks_a_type_variable() {
    // `t0`-style inference variables are internal. A warning that printed one
    // would be telling the reader to write a type they cannot spell.
    let raised = messages(Flavor::Default, GREET);
    assert_ne!(raised, Vec::<String>::new());
    for message in &raised {
        assert!(
            message.starts_with("redundant "),
            "message does not open with the rule: {message}"
        );
        assert!(
            message.ends_with(" without it"),
            "message does not close with the fix: {message}"
        );
        assert!(
            !message.contains("`t0`") && !message.contains("`t1`"),
            "message leaked an inference variable: {message}"
        );
    }
}
