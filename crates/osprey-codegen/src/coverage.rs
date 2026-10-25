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
        mutate::statement_children_mut(statement, &mut mark_source_entries);
    }
    instrumented
}

fn mark_source_entries(expression: &mut Expr) {
    mutate::children_mut(expression, &mut mark_source_entries);
    mark_expression_entry(expression);
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
mod tests {
    use super::coverable_lines;
    use crate::builder::{Codegen, CodegenOptions};
    use osprey_ast::{Expr, Position, Program, Stmt};

    #[test]
    fn coverage_preserves_static_specialization_of_selected_closure_factories() {
        let source = "static effect Read<B> { make: fn() -> B; value: fn() -> int }\nfn choose(flag) = if flag { fn() => perform Read.value() } else { fn() => perform Read.value() }\nfn run(flag) = {\nlet work = choose(flag)\nhandle static Read { make => 0; value => 7 }\nprint(work())\n}\nfn main() = run(true)\n";
        let parsed = osprey_syntax::parse_program(source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let plain = crate::compile_program(&parsed.program);
        assert!(plain.is_ok(), "plain: {plain:?}");
        let covered = crate::compile_program_coverage(&parsed.program);
        assert!(covered.is_ok(), "coverage: {covered:?}");
    }

    const NESTED_SOURCE: &str = "namespace sample {\n\
      module Stats {\n\
      fn choose(flag) = {\n\
      let identity = fn(x) => x\n\
      mut result = 0\n\
      result = match flag {\n\
      true => identity(1)\n\
      false => identity(2)\n\
      }\n\
      result\n\
      }\n\
      }\n\
      }\n\
      effect Ask { value: fn() -> int }\n\
      fn read() = perform Ask.value()\n\
      let handled = {\n\
          handle Ask {\n\
              value => {\n\
              let answer = 42\n\
              answer\n\
              }\n\
          }\n\
          let got = read()\n\
          got\n\
      }\n\
      print(toString(handled))\n";

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
        crate::testkit::shows(
            &init,
            &["call void @osp_cov_register_line(i64 4, i64* @__osp_cov_hits.4)"],
        );
        let module = cg.render();
        assert_eq!(
            module
                .matches("@__osp_cov_hits.4 = internal global i64 0")
                .count(),
            1
        );
        assert_eq!(module.matches("store i64").count(), 2);
    }

    /// The built-in prelude is compiler source attached beside the program,
    /// not the program: its lines are neither coverable nor counted, so a
    /// program that uses Json reports only its own lines.
    #[test]
    fn prelude_lines_are_never_coverable() {
        let mut cg = coverage_codegen();
        let prelude = Position {
            line: osprey_types::PRELUDE_LINE_BASE + 7,
            column: 0,
        };
        cg.begin_function("f", None);
        cg.cov_hit(Some(prelude));
        cg.cov_hit(Some(Position { line: 3, column: 0 }));
        cg.emit("ret i64 0");
        cg.finish_function("i64", "f", &[]);
        let program = Program {
            statements: vec![Stmt::Expr {
                value: Expr::Integer(1),
                doc: None,
                position: Some(prelude),
            }],
            doc: None,
        };
        cg.cov_seed(&program);
        let init = cg.cov_render_init().unwrap_or_default();
        crate::testkit::shows(&init, &["i64 3,"]);
        assert!(!init.contains(&format!("i64 {},", prelude.line)), "{init}");
        assert!(!cg
            .render()
            .contains(&format!("@__osp_cov_hits.{}", prelude.line)));
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

    #[test]
    fn cov_seed_finds_lines_inside_every_nested_control_flow_shape() {
        let parsed = osprey_syntax::parse_program(NESTED_SOURCE);
        assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
        // Namespace/module/function lines, then the handled region: the arm's
        // own statement (line 19) and the statement the handler governs (23).
        let expected = std::collections::BTreeSet::from([3, 4, 5, 6, 15, 16, 19, 23, 26]);
        assert_eq!(coverable_lines(&parsed.program), expected);
        let mut cg = coverage_codegen();
        cg.cov_seed(&parsed.program);
        let module = cg.render();
        for line in expected {
            let global = format!("@__osp_cov_hits.{line} = internal global i64 0");
            assert!(module.contains(&global), "missing line {line}:\n{module}");
        }
        assert_eq!(module.matches("internal global i64 0").count(), 9);
    }

    #[test]
    fn cov_seed_includes_uncalled_generic_branches_nested_in_containers() {
        let source = "fn unused<T>(flag) = [\n\
            match flag {\n\
                true => {\n\
                    let selected = 1\n\
                    selected\n\
                }\n\
                false => {\n\
                    let selected = 2\n\
                    selected\n\
                }\n\
            }\n\
        ]\n";
        let parsed = osprey_syntax::parse_program(source);
        assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
        assert_eq!(
            coverable_lines(&parsed.program),
            std::collections::BTreeSet::from([1, 4, 8])
        );
    }

    #[test]
    fn inline_function_definition_hits_are_recorded_at_call_sites() {
        let mut cg = coverage_codegen();
        cg.begin_function("caller", None);
        cg.cov_note_inline_fn("identity", Some(Position { line: 8, column: 3 }));
        cg.cov_hit_inline_fn("identity");
        cg.cov_hit_inline_fn("unknown");
        cg.emit("ret i64 0");
        cg.finish_function("i64", "caller", &[]);
        let module = cg.render();
        crate::testkit::shows(&module, &["@__osp_cov_hits.8 = internal global i64 0"]);
        assert_eq!(module.matches("store i64").count(), 1);
    }
}
