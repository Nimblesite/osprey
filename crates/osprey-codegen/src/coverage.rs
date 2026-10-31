//! Line-coverage instrumentation. Implements [TESTING-COVERAGE-CODEGEN]
//! (docs/specs/language/0027-TestingFramework.md).
//!
//! When [`crate::builder::CodegenOptions::coverage`] is set, lowering marks
//! each coverable source line — a function definition or a statement with a
//! recorded position — by bumping a per-line `i64` counter global inline where
//! control flow reaches it. A generated `__osp_cov_init` (called at the top of
//! `main`, before user code) registers every counter with the C runtime
//! (`compiler/runtime/coverage_runtime.c`), which dumps `<line> <hits>` rows
//! at exit when `OSPREY_COVERAGE=<path>` is set [TESTING-COVERAGE-ENV].

use crate::builder::Codegen;
use osprey_ast::{mutate, walk_program, AstVisitor, Expr, Position, Program, Stmt};
use std::collections::BTreeSet;

/// Keep source entries attached to the code that executes their bodies.
/// Implements [TESTING-COVERAGE-CODEGEN]; unused arms retain zero counters.
pub(crate) fn instrumented(program: &Program) -> Program {
    let mut instrumented = program.clone();
    for statement in &mut instrumented.statements {
        mutate::statement_children_mut(statement, &mut mark_handler_entries);
    }
    instrumented
}

fn mark_handler_entries(expression: &mut Expr) {
    mutate::children_mut(expression, &mut mark_handler_entries);
    if let Expr::Handler {
        stage,
        arms,
        body,
        position,
        ..
    } = expression
    {
        arms.iter_mut().for_each(mark_arm_entry);
        if stage.is_compile_time() {
            **body = arm_entry((**body).clone(), *position);
        }
    }
}

pub(crate) fn instrument_expressions(program: &mut Program) {
    for statement in &mut program.statements {
        mutate::statement_children_mut(statement, &mut mark_expression_entries);
    }
}

fn mark_expression_entries(expression: &mut Expr) {
    mutate::children_mut(expression, &mut mark_expression_entries);
    mark_expression_entry(expression);
}

fn mark_expression_entry(expression: &mut Expr) {
    if let Expr::Block {
        statements,
        value: Some(_),
        position: Some(position),
    } = expression
    {
        if statements.is_empty() {
            statements.push(entry_statement(Some(*position)));
        }
    }
}

fn mark_arm_entry(arm: &mut osprey_ast::HandlerArm) {
    arm.body = arm_entry(arm.body.clone(), arm.position);
}

fn arm_entry(body: Expr, position: Option<Position>) -> Expr {
    Expr::Block {
        statements: vec![entry_statement(position)],
        value: Some(Box::new(body)),
        position: None,
    }
}

fn entry_statement(position: Option<Position>) -> Stmt {
    Stmt::Expr {
        value: Expr::Block {
            statements: Vec::new(),
            value: None,
            position: None,
        },
        doc: None,
        position,
    }
}

/// Per-module coverage state: the coverable lines. Seeded from the whole AST
/// up front so an unexecuted (even never-lowered) line still counts against
/// the total — the denominator comes from the source, not from what codegen
/// happened to reach. `BTreeSet` keeps the dump in ascending line order.
#[derive(Debug, Default)]
pub(crate) struct CoverageState {
    lines: BTreeSet<u32>,
    /// Definition lines of functions specialised by inlining (`fn_defs`):
    /// they never pass through `gen_function`, so `try_inline` bumps the
    /// definition line through this map instead.
    inline_fn_lines: std::collections::HashMap<String, Position>,
}

/// All coverable lines in `program`: function definitions plus positioned
/// statements anywhere in the shared AST traversal.
fn coverable_lines(program: &Program) -> BTreeSet<u32> {
    let mut collector = CoverableLines::default();
    walk_program(program, &mut collector);
    collector.0
}

#[derive(Default)]
struct CoverableLines(BTreeSet<u32>);

impl AstVisitor for CoverableLines {
    fn expression(&mut self, expression: &Expr) {
        if let Expr::Block {
            statements,
            value: Some(_),
            position,
        } = expression
        {
            if statements.is_empty() {
                mark(*position, &mut self.0);
            }
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        let position = match statement {
            Stmt::Function { position, .. }
            | Stmt::Let { position, .. }
            | Stmt::Assignment { position, .. }
            | Stmt::Expr { position, .. } => *position,
            _ => None,
        };
        mark(position, &mut self.0);
    }
}

/// The program line a position counts against, or `None` when it has none or
/// belongs to the built-in prelude rather than the program.
fn coverable_line(position: Option<Position>) -> Option<u32> {
    position
        .map(|p| p.line.max(1))
        .filter(|line| !osprey_types::is_prelude_line(*line))
}

fn mark(position: Option<Position>, lines: &mut BTreeSet<u32>) {
    if let Some(line) = coverable_line(position) {
        let _ = lines.insert(line);
    }
}

/// The counter global for a source line.
fn counter_global(line: u32) -> String {
    format!("@__osp_cov_hits.{line}")
}

impl Codegen {
    /// Seed the coverable-line universe from the whole AST and declare every
    /// counter global up front, so a line codegen never reaches (an uncalled
    /// generic's body, a dead branch) still dumps as 0 hits. A no-op unless
    /// coverage instrumentation is active.
    pub(crate) fn cov_seed(&mut self, program: &Program) {
        if self.coverage.is_none() {
            return;
        }
        for line in coverable_lines(program) {
            let first_sight = self
                .coverage
                .as_mut()
                .is_some_and(|cov| cov.lines.insert(line));
            if first_sight {
                self.add_global(format!("{} = internal global i64 0", counter_global(line)));
            }
        }
    }

    /// Record an inline-specialised function's definition line, so each
    /// inlined call site bumps it (the body never reaches `gen_function`).
    pub(crate) fn cov_note_inline_fn(&mut self, name: &str, position: Option<Position>) {
        if let (Some(cov), Some(p)) = (self.coverage.as_mut(), position) {
            let _ = cov.inline_fn_lines.insert(name.to_string(), p);
        }
    }

    /// Bump the definition line of an inline-specialised function at one of
    /// its call sites. A no-op unless coverage is active and `name` is one.
    pub(crate) fn cov_hit_inline_fn(&mut self, name: &str) {
        let position = self
            .coverage
            .as_ref()
            .and_then(|cov| cov.inline_fn_lines.get(name).copied());
        self.cov_hit(position);
    }

    /// Credit an inlined callback body with its definition line per invocation.
    pub(crate) fn cov_inline_body(&self, body: Expr, position: Option<Position>) -> Expr {
        if self.coverage.is_none() || coverable_line(position).is_none() {
            return body;
        }
        arm_entry(body, position)
    }

    /// Mark `position`'s line covered at this point in the emitted code:
    /// declare its counter global on first sight and bump it in place. A
    /// no-op unless coverage instrumentation is active.
    pub(crate) fn cov_hit(&mut self, position: Option<Position>) {
        let Some(line) = coverable_line(position) else {
            return;
        };
        let first_sight = match self.coverage.as_mut() {
            Some(cov) => cov.lines.insert(line),
            None => return,
        };
        let global = counter_global(line);
        if first_sight {
            self.add_global(format!("{global} = internal global i64 0"));
        }
        let loaded = self.emit_reg(format!("load i64, i64* {global}"));
        let bumped = self.emit_reg(format!("add i64 {loaded}, 1"));
        self.emit(format!("store i64 {bumped}, i64* {global}"));
    }

    /// Emit the boot call that registers every counter before user code runs.
    /// Emitted at the top of `main`; the registration function body is
    /// rendered later (after all lowering) by [`Codegen::cov_render_init`].
    pub(crate) fn cov_emit_boot(&mut self) {
        if self.coverage.is_none() {
            return;
        }
        self.add_extern("declare void @osp_cov_register_line(i64, i64*)");
        self.emit("call void @__osp_cov_init()");
    }

    /// Render `__osp_cov_init`: one `osp_cov_register_line(line, &counter)`
    /// call per coverable line, in ascending line order. `None` when coverage
    /// is inactive.
    pub(crate) fn cov_render_init(&self) -> Option<String> {
        let cov = self.coverage.as_ref()?;
        let calls = cov
            .lines
            .iter()
            .map(|line| {
                format!(
                    "  call void @osp_cov_register_line(i64 {line}, i64* {})",
                    counter_global(*line)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!(
            "define internal void @__osp_cov_init() {{\nentry:\n{calls}\n  ret void\n}}"
        ))
    }
}

#[cfg(test)]
mod tests;
