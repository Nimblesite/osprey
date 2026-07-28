//! Static test-case discovery: find `test("name", …)` calls with a literal
//! name, for the `--list-tests` CLI mode and editor test explorers. Discovery
//! is statement-scoped — a `test` call is found wherever it stands as a
//! statement value (top level, block statements, lambda/handler/match bodies,
//! namespaces, modules); dynamically-named or expression-buried calls still
//! run and report via TAP, they are just not statically listable.
//!
//! A `///` (Default) or `(** … *)` (ML) block written directly above a test
//! case travels with it: `--list-tests` carries the one-line `summary` the
//! Test Explorer shows beside the case name and the full rendered `doc`
//! markdown its hover shows ([TESTING-DOC]). Implements [TESTING-LIST]
//! (docs/specs/0027-TestingFramework.md).

use crate::analysis::json_str;
use osprey_ast::{DocComment, Expr, Position, Program, Stmt};
use std::fmt::Write as _;

/// One statically-discovered test case: its literal name, the position of the
/// nearest enclosing statement (the `test` call's own line in practice), and
/// the documentation written directly above it, if any.
#[derive(Debug)]
pub struct TestCase {
    /// The test's literal name (the first argument to `test`).
    pub(crate) name: String,
    /// 1-based line / 0-based column of the enclosing statement.
    pub(crate) position: Option<Position>,
    /// The doc comment attached to the case's own statement ([TESTING-DOC]).
    pub(crate) doc: Option<DocComment>,
}

impl TestCase {
    /// The doc's first paragraph — the inline description beside the case name.
    /// Empty when undocumented.
    pub(crate) fn summary(&self) -> &str {
        self.doc.as_ref().map_or("", |doc| doc.summary.as_str())
    }

    /// The whole doc comment rendered as hover Markdown. Empty when
    /// undocumented ([DOC-EXPORT]).
    pub(crate) fn markdown(&self) -> String {
        self.doc
            .as_ref()
            .map_or_else(String::new, DocComment::render_markdown)
    }
}

/// The `--list-tests` JSON array:
/// `[{"name":…,"line":…,"column":…,"summary":…,"doc":…}, …]`. Line and column
/// are 1-based on the wire, matching `symbols_json`; `summary` and `doc` are
/// OMITTED entirely for an undocumented case, so the wire shape of an
/// undocumented suite is unchanged ([TESTING-DOC]).
#[must_use]
pub fn tests_json(program: &Program) -> String {
    let rendered: Vec<String> = collect_tests(program).iter().map(case_json).collect();
    format!("[{}]", rendered.join(","))
}

fn case_json(case: &TestCase) -> String {
    let (line, column) = case
        .position
        .map_or((1, 1), |p| (p.line, p.column.saturating_add(1)));
    let mut out = format!(
        "{{\"name\":{},\"line\":{line},\"column\":{column}",
        json_str(&case.name)
    );
    push_field(&mut out, "summary", case.summary());
    push_field(&mut out, "doc", &case.markdown());
    out.push('}');
    out
}

/// Append `,"key":"value"` when `value` is non-empty; an absent doc section
/// leaves no key at all rather than an empty string.
fn push_field(out: &mut String, key: &str, value: &str) {
    if !value.is_empty() {
        let _ = write!(out, ",\"{key}\":{}", json_str(value));
    }
}

/// Collect every statically-visible test case, in source order.
#[must_use]
pub(crate) fn collect_tests(program: &Program) -> Vec<TestCase> {
    let mut out = Vec::new();
    walk_stmts(&program.statements, None, &mut out);
    out
}

fn walk_stmts(stmts: &[Stmt], pos: Option<Position>, out: &mut Vec<TestCase>) {
    for stmt in stmts {
        walk_stmt(stmt, pos, out);
    }
}

fn walk_stmt(stmt: &Stmt, pos: Option<Position>, out: &mut Vec<TestCase>) {
    match stmt {
        // Only an expression statement's OWN doc documents a test case: a doc
        // on a `let`/`fn` describes the binding, not any test buried in it.
        Stmt::Expr {
            value,
            position,
            doc,
        } => walk_value(value, position.or(pos), doc.as_ref(), out),
        Stmt::Let {
            value, position, ..
        }
        | Stmt::Assignment {
            value, position, ..
        } => walk_value(value, position.or(pos), None, out),
        Stmt::Function { body, position, .. } => walk_value(body, position.or(pos), None, out),
        Stmt::Namespace { body, .. } => walk_stmts(body, pos, out),
        Stmt::Module { body, .. } => {
            for item in body {
                walk_stmt(&item.declaration, pos, out);
            }
        }
        _ => {}
    }
}

/// Descend a statement's value into the containers a test call can stand in.
/// `doc` is the enclosing statement's documentation; it reaches the `test` call
/// only when that call IS the statement's value, so a doc above a block never
/// silently claims the cases nested inside it.
fn walk_value(
    expr: &Expr,
    pos: Option<Position>,
    doc: Option<&DocComment>,
    out: &mut Vec<TestCase>,
) {
    match expr {
        Expr::Call {
            function,
            arguments,
            ..
        } => record_test_call(function, arguments, pos, doc, out),
        Expr::Block { statements, value } => {
            walk_stmts(statements, pos, out);
            if let Some(v) = value {
                walk_value(v, pos, None, out);
            }
        }
        Expr::Lambda { body, position, .. } => walk_value(body, position.or(pos), None, out),
        Expr::Handler {
            arms,
            body,
            position,
            ..
        } => {
            let pos = position.or(pos);
            for arm in arms {
                walk_value(&arm.body, pos, None, out);
            }
            walk_value(body, pos, None, out);
        }
        Expr::Match { arms, .. } => {
            for arm in arms {
                walk_value(&arm.body, pos, None, out);
            }
        }
        _ => {}
    }
}

/// Record a call whose callee is the bare identifier `test` and whose first
/// positional argument is a string literal.
fn record_test_call(
    function: &Expr,
    arguments: &[Expr],
    pos: Option<Position>,
    doc: Option<&DocComment>,
    out: &mut Vec<TestCase>,
) {
    if let (Expr::Identifier(callee), Some(Expr::Str(name))) = (function, arguments.first()) {
        if callee == "test" {
            out.push(TestCase {
                name: name.clone(),
                position: pos,
                doc: doc.cloned(),
            });
        }
    }
}

/// The documentation of the test case declared on `line` (1-based), rendered as
/// hover Markdown under the case's own name heading. Drives the editor hover
/// over a `test("…", …)` call ([TESTING-DOC], [LSP-HOVER-DOCS]).
#[must_use]
pub(crate) fn test_case_hover(program: &Program, line: u32) -> Option<String> {
    let case = collect_tests(program)
        .into_iter()
        .find(|case| case.position.is_some_and(|p| p.line == line))?;
    let markdown = case.markdown();
    let heading = format!("**Test:** {}", case.name);
    Some(if markdown.is_empty() {
        heading
    } else {
        format!("{heading}\n\n{markdown}")
    })
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use super::*;
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
            "effect Env {\n    mode: fn() -> string\n}\nfn suite() !Env = match perform Env.mode() {\n    \"fast\" => test(\"fast case\", fn() => expect(1, 1))\n    _ => test(\"slow case\", fn() => expect(2, 2))\n}\nhandle Env\n    mode => resume(\"fast\")\nin {\n    suite()\n}\n",
        ));
        let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["fast case", "slow case"]);
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
        assert!(
            md.contains("**Deprecated.** superseded by `commutes`"),
            "{md}"
        );
        assert!(md.starts_with("Legacy check."), "{md}");
    }

    #[test]
    fn test_case_hover_answers_on_the_cases_own_line_only() {
        let program = program(DOCUMENTED);
        let hov = test_case_hover(&program, 22).expect("hover on the `test(` line");
        assert!(hov.starts_with("**Test:** commutes"), "{hov}");
        assert!(hov.contains("Addition is commutative."), "{hov}");
        assert!(hov.contains("**Parameters**"), "full doc, not just summary");
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
}
