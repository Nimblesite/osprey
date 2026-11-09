//! Test discovery tests.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]

use super::*;
use crate::testkit::shows;
use osprey_syntax::{parse_program, parse_program_with_flavor, Flavor};

fn program(src: &str) -> Program {
    let parsed = parse_program(src);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    parsed.program
}

fn ml_program(src: &str) -> Program {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    parsed.program
}

#[test]
fn lists_top_level_literal_tests_with_positions() {
    let json = tests_json(&program(
        "fn add(a, b) = a + b\n\ntest(\"adds\", fn() => expect(add(1, 2), 3))\ntest(\"doubles\", fn() => expect(2 * 2, 4))\n",
    ));
    assert_eq!(
        json,
        "[{\"name\":\"adds\",\"line\":3,\"column\":1},{\"name\":\"doubles\",\"line\":4,\"column\":1}]"
    );
}

#[test]
fn skips_dynamic_names_and_unrelated_calls() {
    let cases = collect_tests(&program(
        "let name = \"dyn\"\ntest(name, fn() => expect(1, 1))\nprint(\"not a test\")\n",
    ));
    assert!(cases.is_empty());
}

#[test]
fn finds_tests_in_main_blocks_and_ml_modules() {
    let in_main = collect_tests(&program(
        "fn main() = {\n    test(\"in main\", fn() => expect(1, 1))\n}\n",
    ));
    let names: Vec<&str> = in_main.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["in main"]);

    let in_module = collect_tests(&ml_program(
        "namespace demo\n\nmodule Cases\n    run () =\n        test \"inside module\" (\\() => check \"x\" 1 1)\n",
    ));
    let names: Vec<&str> = in_module.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["inside module"]);
}

#[test]
fn finds_tests_under_match_and_handler_arms() {
    let cases = collect_tests(&program(
        "effect Env {\n    control mode: fn() -> string\n}\nfn suite() !Env = match perform Env.mode() {\n    \"fast\" => test(\"fast case\", fn() => expect(1, 1))\n    _ => test(\"slow case\", fn() => expect(2, 2))\n}\nlet _ = {\n    handle Env {\n        mode => resume(\"fast\")\n    }\n    suite()\n}\n",
    ));
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["fast case", "slow case"]);
}

// [TESTING-SHADOWING] a user-defined `test` is NOT the built-in, so its
// calls declare no cases: nothing is listed, hovered, or skip-warned.
#[test]
fn a_user_defined_test_function_shadows_the_builtin_and_declares_no_cases() {
    let shadowed = program(
        "fn test(name, body) = 0\n\
         let ignored = test(\"not a case\", fn() => Skip(\"not a real skip\"))\n",
    );
    assert!(
        collect_tests(&shadowed).is_empty(),
        "shadowed by a function"
    );
    assert_eq!(tests_json(&shadowed), "[]");
    assert_eq!(test_case_names(&shadowed), Vec::<String>::new());
    assert_eq!(test_case_hover(&shadowed, 2), None);

    // An `extern` named `test` shadows it just the same.
    let externed = program(
        "extern fn test(name: string, body: int) -> int\n\
         let ignored = test(\"not a case\", 1)\n",
    );
    assert!(collect_tests(&externed).is_empty(), "shadowed by an extern");

    // WITHOUT the shadowing declaration the very same call IS a case, so
    // the rule turns on the declaration and nothing else.
    let real = program("test(\"a case\", fn() => Skip(\"parked\"))\n");
    assert_eq!(collect_tests(&real).len(), 1);
    assert_eq!(test_case_names(&real), ["a case"]);
}

// [TESTING-SHADOWING] shadowing is LEXICAL. A `test` declared inside a
// namespace or module is `Helpers::test` — it cannot capture a bare
// top-level `test(...)` call, so it must not remove the sibling case from
// discovery. `collect_tests` bails out for the whole file when it sees a
// shadowing declaration, but that scan looks at ONE scope: the top-level
// statements. A regression that made it descend into container bodies
// would silently empty the inventory of every file with a private helper
// called `test` — no error, no warning, just no tests.
#[test]
fn a_test_declared_inside_a_container_does_not_erase_sibling_cases() {
    for source in [
        "namespace Helpers {\n    fn test(name, body) = 0\n}\n\
         test(\"must stay visible\", fn() => Skip(\"parked\"))\n",
        "module Helpers {\n    fn test(name, body) = 0\n}\n\
         test(\"must stay visible\", fn() => Skip(\"parked\"))\n",
    ] {
        let nested = program(source);
        assert_eq!(
            test_case_names(&nested),
            ["must stay visible"],
            "a container-scoped `test` must not erase the top-level case in:\n{source}"
        );
    }
}

// [TESTING-SKIP-WARNING] every statically-recognised `Skip` form, and the
// forms that must stay silent because they can only skip at run time.
// [TESTING-SKIP-REASON] a skip that names no reason is an ERROR.
#[test]
fn skip_warnings_cover_every_static_form_and_no_dynamic_one() {
    let cases = cases(
        "test(\"reasoned\", fn() => Skip(\"why not\"))\n\
         test(\"empty reason\", fn() => Skip(\"\"))\n\
         test(\"block body\", fn() => { Skip(\"from a block\") })\n\
         test(\"nested block\", fn() => { { Skip(\"deep\") } })\n\
         test(\"passes\", fn() => Pass)\n\
         test(\"asserts\", fn() => expect(1, 1))\n\
         test(\"helper\", fn() => guard(1))\n\
         test(\"named ref\", parked)\n",
    );
    let reported: Vec<Option<(bool, String)>> = cases
        .iter()
        .map(|case| {
            case.skip_diagnostic()
                .map(|report| (report.unexplained, report.message))
        })
        .collect();
    assert_eq!(
        reported,
        vec![
            Some((false, String::from("test 'reasoned' is skipped: why not"))),
            Some((
                true,
                String::from(
                    "test 'empty reason' is skipped with no reason; \
                              every skip must name one"
                )
            )),
            Some((
                false,
                String::from("test 'block body' is skipped: from a block")
            )),
            Some((false, String::from("test 'nested block' is skipped: deep"))),
            None,
            None,
            None,
            None,
        ]
    );
}

#[test]
fn json_escapes_quoted_test_names_and_renders_empty_array() {
    let json = tests_json(&program("test(\"lit \\\"q\\\"\", fn() => expect(1, 1))\n"));
    assert_eq!(
        json,
        "[{\"name\":\"lit \\\"q\\\"\",\"line\":1,\"column\":1}]"
    );
    assert_eq!(tests_json(&program("print(\"none\")\n")), "[]");
}

#[test]
fn missing_positions_default_to_line_one() {
    let case = TestCase {
        name: String::from("anon"),
        position: None,
        doc: None,
        skip: None,
    };
    assert_eq!(
        case_json(&case),
        "{\"name\":\"anon\",\"line\":1,\"column\":1}"
    );
}

// ---------- [TESTING-DOC] ----------

/// A documented Default-flavor suite covering every recognised doc section.
const DOCUMENTED: &str = "\
fn add(a, b) = a + b

/// Addition is commutative.
///
/// Swapping the operands cannot change the sum.
///
/// # Parameters
/// - left: the first addend
/// - right: the second addend
///
/// # Returns
/// Unit, reported through `expect`.
///
/// # Raises
/// - Overflow: when the sum leaves int range
///
/// # See also
/// [add]
///
/// # Since
/// 0.3
test(\"commutes\", fn() => expect(add(1, 2), add(2, 1)))

/// Zero is the additive identity.
test(\"identity\", fn() => expect(add(5, 0), 5))

test(\"undocumented\", fn() => expect(add(1, 1), 2))
";

fn cases(src: &str) -> Vec<TestCase> {
    collect_tests(&program(src))
}

#[test]
fn a_doc_comment_above_a_case_becomes_its_summary_and_markdown() {
    let found = cases(DOCUMENTED);
    assert_eq!(found.len(), 3, "every case still discovered");
    assert_eq!(found[0].name, "commutes");
    assert_eq!(found[0].summary(), "Addition is commutative.");
    assert_eq!(found[1].summary(), "Zero is the additive identity.");
    assert_eq!(found[2].summary(), "", "undocumented case has no summary");
    assert!(
        found[2].markdown().is_empty(),
        "undocumented case has no markdown"
    );
}

#[test]
fn the_rendered_markdown_carries_every_populated_doc_section() {
    let md = cases(DOCUMENTED)[0].markdown();
    for needle in [
        "Addition is commutative.",
        "Swapping the operands cannot change the sum.",
        "**Parameters**",
        "- `left` — the first addend",
        "- `right` — the second addend",
        "**Returns**",
        "Unit, reported through `expect`.",
        "**Raises**",
        "- `Overflow` — when the sum leaves int range",
        "**See also**",
        "[add]",
        "**Since**",
        "0.3",
    ] {
        assert!(md.contains(needle), "missing {needle:?} in:\n{md}");
    }
}

#[test]
fn a_documented_case_keeps_the_position_of_its_test_call_not_its_doc() {
    // The doc block starts on line 3; the `test(` call is on line 22. The
    // Test Explorer's gutter marker must land on the call.
    let found = cases(DOCUMENTED);
    assert_eq!(found[0].position.map(|p| p.line), Some(22));
    assert_eq!(found[0].position.map(|p| p.column), Some(0));
    assert_eq!(found[1].position.map(|p| p.line), Some(25));
    assert_eq!(found[2].position.map(|p| p.line), Some(27));
}

#[test]
fn documented_cases_gain_summary_and_doc_keys_and_bare_ones_do_not() {
    let json = tests_json(&program(
        "/// Doubles.\ntest(\"doubles\", fn() => expect(2 * 2, 4))\ntest(\"bare\", fn() => expect(1, 1))\n",
    ));
    assert_eq!(
        json,
        "[{\"name\":\"doubles\",\"line\":2,\"column\":1,\"summary\":\"Doubles.\",\"doc\":\"Doubles.\"},\
         {\"name\":\"bare\",\"line\":3,\"column\":1}]"
    );
}

#[test]
fn doc_text_is_json_escaped_on_the_wire() {
    // Quotes, backslashes, and the newlines a multi-paragraph doc produces
    // must all survive as escapes — the extension parses this with JSON.parse.
    let json = tests_json(&program(
        "/// Proves \"quoted\" \\ paths.\n///\n/// Second paragraph.\ntest(\"escapes\", fn() => expect(1, 1))\n",
    ));
    assert!(
        json.contains(r#""summary":"Proves \"quoted\" \\ paths.""#),
        "{json}"
    );
    assert!(json.contains(r"\n\nSecond paragraph."), "{json}");
    assert!(
        !json.contains('\n'),
        "no raw newline in the wire form: {json}"
    );
}

#[test]
fn ml_block_docs_attach_to_ml_cases() {
    let found = collect_tests(&ml_program(
        "add a b = a + b\n\n\
         (** Addition is commutative.\n\n    Order cannot matter. *)\n\
         test \"commutes\" (\\() => check \"c\" (add 1 2) (add 2 1))\n\n\
         test \"bare\" (\\() => check \"b\" 1 1)\n",
    ));
    let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["commutes", "bare"]);
    assert_eq!(found[0].summary(), "Addition is commutative.");
    assert!(found[0].markdown().contains("Order cannot matter."));
    assert_eq!(found[1].summary(), "", "the ML doc does not leak forward");
}

#[test]
fn a_declarations_doc_never_leaks_onto_the_tests_inside_it() {
    // A `///` on a function documents the FUNCTION. Cases registered from
    // its body must stay undocumented rather than inherit its prose.
    let found = cases(
        "/// Registers the arithmetic suite.\n\
         fn suite() = {\n\
         \x20   test(\"inner\", fn() => expect(1, 1))\n\
         }\n\
         /// Documents the binding, not the case.\n\
         let registered = test(\"bound\", fn() => expect(1, 1))\n",
    );
    let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["inner", "bound"]);
    assert!(found.iter().all(|c| c.summary().is_empty()), "{found:?}");
}

#[test]
fn a_doc_above_a_block_does_not_claim_the_cases_nested_in_it() {
    let found = cases(
        "/// Documents the block itself.\n\
         {\n\
         \x20   test(\"nested\", fn() => expect(1, 1))\n\
         }\n",
    );
    let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["nested"]);
    assert_eq!(found[0].summary(), "", "block doc stays on the block");
}

#[test]
fn docs_survive_namespaces_and_modules() {
    let found = collect_tests(&ml_program(
        "namespace demo\n\n\
         module Cases\n\
         \x20   run () =\n\
         \x20       test \"inside module\" (\\() => check \"x\" 1 1)\n",
    ));
    let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["inside module"], "container walk still finds cases");
}

#[test]
fn a_deprecated_case_renders_its_deprecation_first() {
    let md = cases(
        "/// Legacy check.\n\
         ///\n\
         /// # Deprecated\n\
         /// superseded by `commutes`\n\
         test(\"legacy\", fn() => expect(1, 1))\n",
    )[0]
    .markdown();
    shows(&md, &["**Deprecated.** superseded by `commutes`"]);
    assert!(md.starts_with("Legacy check."), "{md}");
}

// [TESTING-DOC-HOVER] the hover answers on the `test(` line only.
#[test]
fn test_case_hover_answers_on_the_cases_own_line_only() {
    let program = program(DOCUMENTED);
    let hov = test_case_hover(&program, 22).expect("hover on the `test(` line");
    assert!(hov.starts_with("**Test:** commutes"), "{hov}");
    shows(&hov, &["Addition is commutative.", "**Parameters**"]);
    // A line with no test case declared on it has no test hover at all.
    assert_eq!(test_case_hover(&program, 1), None);
    assert_eq!(test_case_hover(&program, 999), None);
}

#[test]
fn test_case_hover_still_names_an_undocumented_case() {
    let program = program("test(\"bare\", fn() => expect(1, 1))\n");
    assert_eq!(
        test_case_hover(&program, 1).as_deref(),
        Some("**Test:** bare")
    );
}

#[test]
fn push_field_omits_empty_values_and_escapes_present_ones() {
    let mut out = String::from("{");
    push_field(&mut out, "summary", "");
    assert_eq!(out, "{", "an empty section adds no key");
    push_field(&mut out, "doc", "a \"b\"");
    assert_eq!(out, "{,\"doc\":\"a \\\"b\\\"\"");
}
