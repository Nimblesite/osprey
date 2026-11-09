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
//! (docs/specs/language/0027-TestingFramework.md).

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
    /// The literal `Skip` reason when the case's body statically yields the
    /// `Skip` verdict — the "ignored test" idiom ([TESTING-SKIP-WARNING]).
    /// `None` when the body can only skip dynamically (reported at run time).
    pub(crate) skip: Option<String>,
}

/// A statically-detected skip and how loudly it must be reported.
///
/// A skip that names a reason is a visible debt, so it warns. A skip that names
/// NONE is a defect: nobody can weigh a hole in coverage whose cause was never
/// written down, so it is reported at Error severity and fails the run
/// ([TESTING-SKIP-REASON]).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SkipReport {
    /// The skip carries no reason, so this diagnostic is an error.
    pub(crate) unexplained: bool,
    /// The rendered diagnostic message.
    pub(crate) message: String,
}

/// What an unexplained skip must do about itself ([TESTING-SKIP-REASON]).
const SKIP_REASON_DEMAND: &str = "with no reason; every skip must name one";

impl TestCase {
    /// The diagnostic raised for a statically-skipped case, if any
    /// ([TESTING-SKIP-WARNING], [TESTING-SKIP-REASON]): a skipped test is never
    /// silent, and an unexplained one is an error rather than a warning.
    pub(crate) fn skip_diagnostic(&self) -> Option<SkipReport> {
        let reason = self.skip.as_ref()?;
        let head = format!("test '{}' is skipped", self.name);
        Some(if reason.is_empty() {
            SkipReport {
                unexplained: true,
                message: format!("{head} {SKIP_REASON_DEMAND}"),
            }
        } else {
            SkipReport {
                unexplained: false,
                message: format!("{head}: {reason}"),
            }
        })
    }

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

/// Collect every statically-visible test case, in source order. A program that
/// declares its own `test` function or `extern` has SHADOWED the built-in
/// ([TESTING-SHADOWING]), so none of its `test(...)` calls are test cases and
/// none are listed, hovered, or warned about.
#[must_use]
pub(crate) fn collect_tests(program: &Program) -> Vec<TestCase> {
    if shadows_test_builtin(&program.statements) {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk_stmts(&program.statements, None, &mut out);
    out
}

/// Whether THIS scope declares a `test` that shadows the testing built-in
/// ([TESTING-SHADOWING]). Shadowing is lexical: the built-in is resolved at the
/// call site's scope, so only declarations at the same level as the call can
/// capture it. Deliberately NOT recursive — descending into namespace and
/// module bodies turned a scoped helper into a whole-file kill switch, because
/// the file-wide bail-out below would then fire on a `test` that no top-level
/// call site can even see.
fn shadows_test_builtin(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::Function { name, .. } | Stmt::Extern { name, .. } => name == TEST_BUILTIN,
        _ => false,
    })
}

/// The declarations a module body contributes to its own scope.
fn module_scope(body: &[osprey_ast::ModuleItem]) -> Vec<Stmt> {
    body.iter()
        .map(|item| (*item.declaration).clone())
        .collect()
}

/// The built-in that declares a test case ([TESTING-BUILTIN-TEST]).
const TEST_BUILTIN: &str = "test";

/// Every statically-discoverable test case name, in source order. Consumers
/// disambiguate a TAP line whose description itself contains `# SKIP` by
/// checking it against these names ([TESTING-TAP], [TESTING-SKIP-WARNING]).
#[must_use]
pub fn test_case_names(program: &Program) -> Vec<String> {
    collect_tests(program)
        .into_iter()
        .map(|case| case.name)
        .collect()
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
        // A container that declares its own `test` shadows the built-in for the
        // calls INSIDE it, and only those: its siblings keep the built-in.
        Stmt::Namespace { body, .. } => {
            if !shadows_test_builtin(body) {
                walk_stmts(body, pos, out);
            }
        }
        Stmt::Module { body, .. } if !shadows_test_builtin(&module_scope(body)) => {
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
        Expr::Block {
            statements, value, ..
        } => {
            walk_stmts(statements, pos, out);
            if let Some(v) = value {
                walk_value(v, pos, None, out);
            }
        }
        Expr::Lambda { body, position, .. } => walk_value(body, position.or(pos), None, out),
        Expr::Handler {
            arms,
            body,
            return_clause,
            finally,
            position,
            ..
        } => {
            let pos = position.or(pos);
            for arm in arms {
                walk_value(&arm.body, pos, None, out);
            }
            walk_value(body, pos, None, out);
            for clause in return_clause.iter().chain(finally) {
                walk_value(clause, pos, None, out);
            }
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
        if callee == TEST_BUILTIN {
            out.push(TestCase {
                name: name.clone(),
                position: pos,
                doc: doc.cloned(),
                skip: static_skip(arguments.get(1)),
            });
        }
    }
}

/// The literal `Skip` reason when `body` is a lambda whose result expression is
/// the `Skip` verdict constructor ([TESTING-SKIP-WARNING], [TESTING-VERDICT]).
fn static_skip(body: Option<&Expr>) -> Option<String> {
    match body? {
        Expr::Lambda { body, .. } => skip_reason(result_expr(body)),
        _ => None,
    }
}

/// A block's trailing value; any other expression is its own result.
fn result_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::Block {
            value: Some(value), ..
        } => result_expr(value),
        _ => expr,
    }
}

/// `Skip`, `Skip("why")` (Default) or `Skip "why"` (ML) at the result
/// position. Both frontends lower the applied constructor to
/// [`Expr::TypeConstructor`]; a bare `Skip` reference stays an identifier.
fn skip_reason(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(name) if name == "Skip" => Some(String::new()),
        Expr::TypeConstructor { name, fields, .. } if name == "Skip" => {
            Some(match fields.first().map(|field| &field.value) {
                Some(Expr::Str(why)) => why.clone(),
                _ => String::new(),
            })
        }
        Expr::Call {
            function,
            arguments,
            ..
        } => match (function.as_ref(), arguments.first()) {
            (Expr::Identifier(name), Some(Expr::Str(why))) if name == "Skip" => Some(why.clone()),
            (Expr::Identifier(name), _) if name == "Skip" => Some(String::new()),
            _ => None,
        },
        _ => None,
    }
}

/// The documentation of the test case declared on `line` (1-based), rendered as
/// hover Markdown under the case's own name heading. An undocumented case still
/// answers with its `**Test:** <name>` heading. Implements [TESTING-DOC-HOVER]
/// ([TESTING-DOC], [LSP-HOVER-DOCS]).
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
mod tests;
