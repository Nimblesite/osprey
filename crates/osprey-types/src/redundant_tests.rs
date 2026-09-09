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
fn warnings(flavor: Flavor, src: &str) -> Vec<TypeWarning> {
    let parsed = parse_program_with_flavor(src, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} flavor rejected the snippet at parse time: {:?}\n{src}",
        parsed.errors
    );
    redundant_annotations(&parsed.program)
}

/// The warning messages under `flavor`, in report order.
fn messages(flavor: Flavor, src: &str) -> Vec<String> {
    warnings(flavor, src)
        .into_iter()
        .map(|w| w.message)
        .collect()
}

/// Assert the exact set of messages `flavor` reports for `src`, in order.
fn reports(flavor: Flavor, src: &str, expected: &[&str]) {
    assert_eq!(messages(flavor, src), expected, "\n{src}");
}

/// Assert `src` is clean under `flavor` — every annotation in it is earning
/// its place, or there are none.
fn silent(flavor: Flavor, src: &str) {
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
fn a_redundant_generic_signature_is_reported_with_its_binder() {
    reports(
        Flavor::Default,
        "fn identity<T>(x: T) -> T = x\nlet i = identity(42)\n",
        &[
            "redundant type annotation on parameter `x` of `identity`: inference derives `T` without it",
            "redundant return type annotation on `identity`: inference derives `T` without it",
        ],
    );
}

#[test]
fn a_result_returning_annotation_that_matches_inference_is_still_redundant() {
    reports(
        Flavor::Default,
        "fn half(n: int) -> Result<int, Error> = intDiv(n, 2)\n",
        &[
            "redundant type annotation on parameter `n` of `half`: inference derives `int` without it",
            "redundant return type annotation on `half`: inference derives `Result<int, Error>` without it",
        ],
    );
}

#[test]
fn a_return_annotation_that_narrows_the_error_type_is_kept() {
    // `intDiv` yields `Result<int, Error>`; the annotation names a different
    // error type, so it is doing work and erasing it changes the signature.
    silent(
        Flavor::Default,
        "fn half(n: int) -> Result<int, MathError> = intDiv(n, 2)\n",
    );
}

#[test]
fn ml_and_default_spellings_of_one_signature_report_identically() {
    let default = messages(Flavor::Default, GREET);
    let ml = messages(Flavor::Ml, "greet : string -> string\ngreet name = \"hi \" + name\n");
    assert_eq!(default, ml, "flavors disagreed about the same signature");
    assert_eq!(ml.len(), 2, "{ml:?}");
}

#[test]
fn an_ml_module_body_repeating_its_signature_is_still_redundant() {
    // The `signature` block is the module's contract and is never reported;
    // the body's copy of the same type is judged like any other function.
    reports(
        Flavor::Ml,
        "signature Api\n    shout : string -> string\n\nmodule M : Api\n    shout : string -> string\n    shout s = s + \"!\"\n",
        &[
            "redundant type annotation on parameter `s` of `shout`: inference derives `string` without it",
            "redundant return type annotation on `shout`: inference derives `string` without it",
        ],
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

