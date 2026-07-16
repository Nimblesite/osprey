//! Static test-case discovery: find `test("name", …)` calls with a literal
//! name, for the `--list-tests` CLI mode and editor test explorers. Discovery
//! is statement-scoped — a `test` call is found wherever it stands as a
//! statement value (top level, block statements, lambda/handler/match bodies,
//! namespaces, modules); dynamically-named or expression-buried calls still
//! run and report via TAP, they are just not statically listable. Implements
//! [TESTING-LIST] (docs/specs/0027-TestingFramework.md).

use crate::analysis::json_str;
use osprey_ast::{Expr, Position, Program, Stmt};

/// One statically-discovered test case: its literal name and the position of
/// the nearest enclosing statement (the `test` call's own line in practice).
#[derive(Debug)]
pub struct TestCase {
    /// The test's literal name (the first argument to `test`).
    pub name: String,
    /// 1-based line / 0-based column of the enclosing statement.
    pub position: Option<Position>,
}

/// The `--list-tests` JSON array: `[{"name":…,"line":…,"column":…}, …]`.
/// Line and column are 1-based on the wire, matching `symbols_json`.
#[must_use]
pub fn tests_json(program: &Program) -> String {
    let rendered: Vec<String> = collect_tests(program).iter().map(case_json).collect();
    format!("[{}]", rendered.join(","))
}

fn case_json(case: &TestCase) -> String {
    let (line, column) = case
        .position
        .map_or((1, 1), |p| (p.line, p.column.saturating_add(1)));
    format!(
        "{{\"name\":{},\"line\":{line},\"column\":{column}}}",
        json_str(&case.name)
    )
}

/// Collect every statically-visible test case, in source order.
#[must_use]
pub fn collect_tests(program: &Program) -> Vec<TestCase> {
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
        Stmt::Let {
            value, position, ..
        }
        | Stmt::Assignment {
            value, position, ..
        }
        | Stmt::Expr {
            value, position, ..
        } => walk_value(value, position.or(pos), out),
        Stmt::Function { body, position, .. } => walk_value(body, position.or(pos), out),
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
fn walk_value(expr: &Expr, pos: Option<Position>, out: &mut Vec<TestCase>) {
    match expr {
        Expr::Call {
            function,
            arguments,
            ..
        } => record_test_call(function, arguments, pos, out),
        Expr::Block { statements, value } => {
            walk_stmts(statements, pos, out);
            if let Some(v) = value {
                walk_value(v, pos, out);
            }
        }
        Expr::Lambda { body, position, .. } => walk_value(body, position.or(pos), out),
        Expr::Handler {
            arms,
            body,
            position,
            ..
        } => {
            let pos = position.or(pos);
            for arm in arms {
                walk_value(&arm.body, pos, out);
            }
            walk_value(body, pos, out);
        }
        Expr::Match { arms, .. } => {
            for arm in arms {
                walk_value(&arm.body, pos, out);
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
    out: &mut Vec<TestCase>,
) {
    if let (Expr::Identifier(callee), Some(Expr::Str(name))) = (function, arguments.first()) {
        if callee == "test" {
            out.push(TestCase {
                name: name.clone(),
                position: pos,
            });
        }
    }
}

#[cfg(test)]
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
        };
        assert_eq!(
            case_json(&case),
            "{\"name\":\"anon\",\"line\":1,\"column\":1}"
        );
    }
}
