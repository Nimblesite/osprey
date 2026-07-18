//! Line-coverage instrumentation. Implements [TESTING-COVERAGE-CODEGEN]
//! (docs/specs/0027-TestingFramework.md).
//!
//! When [`crate::builder::CodegenOptions::coverage`] is set, lowering marks
//! each coverable source line — a function definition or a statement with a
//! recorded position — by bumping a per-line `i64` counter global inline where
//! control flow reaches it. A generated `__osp_cov_init` (called at the top of
//! `main`, before user code) registers every counter with the C runtime
//! (`compiler/runtime/coverage_runtime.c`), which dumps `<line> <hits>` rows
//! at exit when `OSPREY_COVERAGE=<path>` is set [TESTING-COVERAGE-ENV].

use crate::builder::Codegen;
use osprey_ast::{Expr, Position, Program, Stmt};
use std::collections::BTreeSet;

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
/// statements, recursing through nested blocks, lambdas, match/handler arms,
/// and module/namespace bodies.
pub(crate) fn coverable_lines(program: &Program) -> BTreeSet<u32> {
    let mut lines = BTreeSet::new();
    collect_stmts(&program.statements, &mut lines);
    lines
}

fn collect_stmts(stmts: &[Stmt], lines: &mut BTreeSet<u32>) {
    for stmt in stmts {
        collect_stmt(stmt, lines);
    }
}

fn mark(position: Option<Position>, lines: &mut BTreeSet<u32>) {
    if let Some(p) = position {
        let _ = lines.insert(p.line.max(1));
    }
}

fn collect_stmt(stmt: &Stmt, lines: &mut BTreeSet<u32>) {
    match stmt {
        Stmt::Function { body, position, .. } => {
            mark(*position, lines);
            collect_expr(body, lines);
        }
        Stmt::Let {
            value, position, ..
        }
        | Stmt::Assignment {
            value, position, ..
        }
        | Stmt::Expr { value, position } => {
            mark(*position, lines);
            collect_expr(value, lines);
        }
        Stmt::Namespace { body, .. } => collect_stmts(body, lines),
        Stmt::Module { body, .. } => {
            for item in body {
                collect_stmt(&item.declaration, lines);
            }
        }
        _ => {}
    }
}

/// Recurse only into the expression forms that carry statements or arm bodies
/// — the places nested coverable statements live.
fn collect_expr(expr: &Expr, lines: &mut BTreeSet<u32>) {
    match expr {
        Expr::Block { statements, value } => {
            collect_stmts(statements, lines);
            if let Some(v) = value {
                collect_expr(v, lines);
            }
        }
        Expr::Lambda { body, .. } => collect_expr(body, lines),
        Expr::Match { value, arms } => {
            collect_expr(value, lines);
            for arm in arms {
                collect_expr(&arm.body, lines);
            }
        }
        Expr::Handler { arms, body, .. } => {
            for arm in arms {
                collect_expr(&arm.body, lines);
            }
            collect_expr(body, lines);
        }
        Expr::Call {
            function,
            arguments,
            ..
        } => {
            collect_expr(function, lines);
            for a in arguments {
                collect_expr(a, lines);
            }
        }
        _ => {}
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

    /// Mark `position`'s line covered at this point in the emitted code:
    /// declare its counter global on first sight and bump it in place. A
    /// no-op unless coverage instrumentation is active.
    pub(crate) fn cov_hit(&mut self, position: Option<Position>) {
        let Some(line) = position.map(|p| p.line.max(1)) else {
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
mod tests {
    use crate::builder::{Codegen, CodegenOptions};
    use osprey_ast::Position;

    fn coverage_codegen() -> Codegen {
        Codegen::with_options(
            osprey_types::ProgramTypes::default(),
            CodegenOptions {
                coverage: true,
                ..CodegenOptions::default()
            },
        )
    }

    #[test]
    fn cov_hit_declares_each_counter_once_and_bumps_in_place() {
        let mut cg = coverage_codegen();
        cg.begin_function("f", None);
        cg.cov_hit(Some(Position { line: 4, column: 0 }));
        cg.cov_hit(Some(Position { line: 4, column: 2 }));
        cg.cov_hit(None); // positionless statements are not coverable
        cg.emit("ret i64 0");
        cg.finish_function("i64", "f", &[]);
        cg.cov_emit_boot(); // extern must land even without a real main here
        let init = cg.cov_render_init().expect("coverage active");
        assert!(init.contains("call void @osp_cov_register_line(i64 4, i64* @__osp_cov_hits.4)"));
        let module = cg.render();
        assert_eq!(
            module
                .matches("@__osp_cov_hits.4 = internal global i64 0")
                .count(),
            1
        );
        assert_eq!(module.matches("store i64").count(), 2);
    }

    #[test]
    fn coverage_inactive_emits_nothing() {
        let mut cg = Codegen::new();
        cg.begin_function("f", None);
        cg.cov_hit(Some(Position { line: 9, column: 0 }));
        cg.cov_emit_boot();
        cg.emit("ret i64 0");
        cg.finish_function("i64", "f", &[]);
        assert!(cg.cov_render_init().is_none());
        assert!(!cg.render().contains("__osp_cov"));
    }
}
