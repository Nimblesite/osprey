//! Tests for [TYPE-ANNOTATION-REDUNDANT].
//!
//! Every assertion here pins the exact diagnostic text, not a substring: the
//! message names the declaration and prints the type the reader would get by
//! deleting the annotation, and a message that stops being that precise is a
//! regression even when it still contains the word "redundant".

use crate::redundant::{redundant_annotations, TypeWarning, REDUNDANT_ANNOTATION};
use std::fmt::Write as _;

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
        "fn half(n: int) -> Result<int, MathError> = intDiv(n, 2)\n",
        &[
            "redundant type annotation on parameter `n` of `half`: inference derives `int` without it",
            "redundant return type annotation on `half`: inference derives `Result<int, MathError>` without it",
        ],
    );
}

#[test]
fn a_return_annotation_naming_the_wrong_error_type_is_a_type_error_not_a_warning() {
    // `intDiv` yields `Result<int, MathError>`. Naming `Error` instead is a
    // mismatch the checker rejects, and a rejected program raises no warnings.
    silent(
        Flavor::Default,
        "fn half(n: int) -> Result<int, Error> = intDiv(n, 2)\n",
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
        "fn twice(f: (int) -> int, x: int) -> int = f(f(x))\nfn inc(n: int) -> int = (n + 1) ?: 0\nlet r = twice(inc, 1)\n",
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
        "effect State { set: fn(int) -> Unit }\nfn main() -> Unit = {\n  mut cell: int = 0\n  handle State\n    set value => { cell = value }\n  in { perform State.set(1) }\n}\n",
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
    assert!(!raised.is_empty());
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

#[test]
fn an_annotation_free_corpus_is_completely_silent() {
    silent(
        Flavor::Default,
        "type Point = { x: int, y: int }\n\
         fn px(p) = p.x\n\
         fn shout(s) = s + \"!\"\n\
         let p = Point { x: 1, y: 2 }\n\
         let out = shout(toString(px(p)))\n",
    );
}

/// Eight redundantly annotated functions around one whose parameter annotation
/// is doing real work — erasing `p: Point` would widen it to a structural row.
/// This is the shape the group-narrowing search has to get right.
fn mixed_corpus() -> String {
    let redundant = (0..8).fold(String::new(), |mut out, i| {
        let _ = writeln!(out, "fn f{i}(s: string) -> string = s + \"{i}\"");
        out
    });
    format!(
        "type Point = {{ x: int, y: int }}\n\
         {redundant}\
         fn keeper(p: Point) -> int = p.x\n\
         let v = keeper(Point {{ x: 1, y: 2 }})\n"
    )
}

#[test]
fn narrowing_does_not_lose_warnings_around_a_load_bearing_annotation() {
    // Erasing everything at once fails because of `keeper`, so the search
    // splits. Every redundant annotation must still be reported, and
    // `keeper`'s load-bearing parameter must not be.
    let raised = messages(Flavor::Default, &mixed_corpus());
    assert_eq!(raised.len(), 17, "{raised:?}");
    for index in 0..8 {
        assert!(
            raised.contains(&format!(
                "redundant type annotation on parameter `s` of `f{index}`: inference derives `string` without it"
            )),
            "lost the parameter of f{index}: {raised:?}"
        );
        assert!(
            raised.contains(&format!(
                "redundant return type annotation on `f{index}`: inference derives `string` without it"
            )),
            "lost the return type of f{index}: {raised:?}"
        );
    }
    assert!(
        raised.contains(
            &"redundant return type annotation on `keeper`: inference derives `int` without it"
                .to_string()
        ),
        "{raised:?}"
    );
    assert!(
        !raised
            .iter()
            .any(|m| m.contains("parameter `p` of `keeper`")),
        "reported a load-bearing parameter: {raised:?}"
    );
}

#[test]
fn narrowing_reports_in_source_order_regardless_of_how_it_searched() {
    // The search halves the annotation list and so visits it out of order.
    // What comes back must still read down the file.
    let raised = warnings(Flavor::Default, &mixed_corpus());
    let lines: Vec<u32> = raised
        .iter()
        .filter_map(|w| w.position.map(|p| p.line))
        .collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "warnings came out of order: {raised:?}");
    assert_eq!(lines.len(), raised.len(), "a warning lost its position");
}

#[test]
fn the_same_program_reports_the_same_warnings_every_run() {
    let source = mixed_corpus();
    let first = messages(Flavor::Default, &source);
    let second = messages(Flavor::Default, &source);
    assert_eq!(first, second, "detection is not deterministic");
}

#[test]
fn an_effect_operation_signature_is_never_reported() {
    // An `effect` declares its operations' types; there is no body to infer
    // them from, so they are declarations and not constraints.
    silent(
        Flavor::Default,
        "effect Log { line: fn(string) -> Unit }\n\
         fn main() = handle Log\n  line message => {}\nin { perform Log.line(\"hi\") }\n",
    );
}

/// The ML header `smaller : int -> int -> int` with `smaller a b = ...`.
/// One written line; the lowering turns it into a parameter, a curried
/// function-typed return, and a nested lambda carrying the rest.
const CURRIED_ML: &str = "smaller : int -> int -> int\n\
                          smaller a b = match a <= b\n    true => a\n    false => b\n";

#[test]
fn a_curried_ml_header_that_monomorphises_a_polymorphic_body_is_kept() {
    // `smaller a b = match a <= b ...` generalises to a comparison over any
    // one type. The header pins it to `int`, so it is doing real work and no
    // part of it is redundant. Judging the lowering's fragments one at a time
    // said the opposite — each slot looked derivable because the others were
    // still holding the type down — and reported four warnings for a line
    // that cannot be deleted.
    silent(Flavor::Ml, CURRIED_ML);
}

#[test]
fn a_curried_ml_header_never_blames_a_lambda_for_a_named_parameter() {
    // `b` is a parameter the reader named on the `smaller a b = ...` line.
    // Attributing it to `<lambda>` points at code nobody wrote.
    let raised = messages(Flavor::Ml, CURRIED_ML);
    assert!(
        !raised.iter().any(|m| m.contains("<lambda>")),
        "blamed a lambda for a written parameter: {raised:?}"
    );
}

#[test]
fn a_genuinely_redundant_curried_header_is_one_warning_naming_the_function() {
    // Here the body pins both parameters to `int` on its own, so the header
    // really is deletable — and it is ONE line, so it is ONE warning naming
    // the function and the whole signature. Reporting `(int) -> int` as the
    // "return type" would be the curried remainder, which appears nowhere in
    // the source.
    reports(
        Flavor::Ml,
        "combine : int -> int -> int\ncombine a b = intDiv a b ?: 0\n",
        &["redundant type signature on `combine`: inference derives `(int) -> (int) -> int` without it"],
    );
}

#[test]
fn a_default_signature_is_unaffected_by_the_curried_collapse() {
    // Nothing curries in Default flavor: each annotation is its own written
    // thing and keeps its own warning. The return type stays unreported
    // because the body only compares — something has to keep `smaller` at
    // `int`, and with both parameters gone that is the return.
    reports(
        Flavor::Default,
        "fn smaller(a: int, b: int) -> int = match a <= b {\n  true => a\n  false => b\n}\n",
        &[
            "redundant type annotation on parameter `a` of `smaller`: inference derives `int` without it",
            "redundant type annotation on parameter `b` of `smaller`: inference derives `int` without it",
        ],
    );
}

#[test]
fn a_hand_written_lambda_is_still_reported_against_the_lambda() {
    // The collapse is specific to a curry chain the lowering produced. A
    // lambda somebody actually wrote still names itself.
    reports(
        Flavor::Ml,
        "exclaim = \\(s: string) => s + \"!\"\nr = exclaim \"hi\"\n",
        &["redundant type annotation on parameter `s` of `<lambda>`: inference derives `string` without it"],
    );
}

#[test]
fn a_three_argument_curried_header_preserves_every_arrow() {
    // The whole written signature is one warning. Its three curried arrows
    // remain distinct from a function taking three arguments in one call.
    reports(
        Flavor::Ml,
        "clamp : int -> int -> int -> int\nclamp a b c = intDiv (intDiv a b ?: 0) c ?: 0\n",
        &["redundant type signature on `clamp`: inference derives `(int) -> (int) -> (int) -> int` without it"],
    );
}

#[test]
fn the_reported_set_is_deletable_as_a_whole_not_only_one_at_a_time() {
    // `pick` is pinned to `int` by its annotations and nothing else: the body
    // only compares. Each annotation alone IS removable, because the other two
    // still hold the type down — so judging them one at a time reports all
    // three. Delete all three and `pick` generalises, which is a different
    // program. A reader acts on the whole list, so the whole list has to be
    // safe: the rule reports a set it has verified can go together.
    let raised = messages(
        Flavor::Default,
        "fn pick(a: int, b: int) -> int = match a <= b {\n  true => a\n  false => b\n}\nlet chosen = pick(1, 2)\n",
    );
    assert!(
        !raised
            .iter()
            .any(|m| m.starts_with("redundant return type annotation on `pick`")),
        "reported every annotation of a mutually-pinned signature: {raised:?}"
    );
    assert_eq!(
        raised,
        vec![
            "redundant type annotation on parameter `a` of `pick`: inference derives `int` without it",
            "redundant type annotation on parameter `b` of `pick`: inference derives `int` without it",
        ],
        "the set must be one the reader can delete whole"
    );
}
