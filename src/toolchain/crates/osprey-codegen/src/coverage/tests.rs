use super::coverable_lines;
use crate::builder::{Codegen, CodegenOptions};
use osprey_ast::{Expr, Position, Program, Stmt};

#[test]
fn cov_seed_registers_unlowered_ml_match_arms() -> Result<(), &'static str> {
    let source = "namespace probe\nunused flag = match flag\n    true => 1\n    false => 0\n";
    let parsed = osprey_syntax::parse_program_for_path("unused.ospml", source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let expected = std::collections::BTreeSet::from([2, 3, 4]);
    assert_eq!(coverable_lines(&parsed.program), expected);
    let mut cg = coverage_codegen();
    cg.cov_seed(&parsed.program);
    let module = cg.render();
    let registration = cg.cov_render_init().ok_or("missing counters")?;
    for line in [2, 3, 4] {
        let counter = format!("@__osp_cov_hits.{line}");
        assert!(module.contains(&format!("{counter} = internal global i64 0")));
        assert!(registration.contains(&counter));
    }
    Ok(())
}

#[test]
fn coverage_preserves_static_specialization_of_selected_closure_factories() {
    let source = "static effect Read<B> {\nmake: fn() -> B\nvalue: fn() -> int\n}\nfn choose(flag) = if flag { fn() => perform Read.value() } else { fn() => perform Read.value() }\nfn run(flag) = {\nlet work = choose(flag)\nhandle static Read {\nmake => 0\nvalue => 7\n}\nprint(work())\n}\nfn other(flag) = {\nlet work = choose(flag)\nhandle static Read {\nmake => \"buffer\"\nvalue => 9\n}\nprint(work())\n}\nfn main() = {\nrun(true)\nother(false)\n}\n";
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

const NAMED_CALLBACKS: &str = "type Box =\n    held : int\ninvoke f x = f x\nsaveOne box = box.held\nsaveMany box =\n    n = box.held\n    n\nprint \"${invoke saveOne (Box(held = 1))}|${invoke saveMany (Box(held = 2))}\"\n";

fn callback_header_owners(module: &str, line: u32) -> Vec<&str> {
    let counter = format!("i64* @__osp_cov_hits.{line}");
    module
        .split("\n}")
        .filter(|body| {
            body.lines()
                .any(|line| line.contains("store i64") && line.ends_with(&counter))
        })
        .filter_map(|body| body.rsplit("define ").next())
        .collect()
}

#[test]
fn named_callback_headers_are_bumped_inside_the_emitted_closure() {
    let parsed = osprey_syntax::parse_program_for_path("callbacks.ospml", NAMED_CALLBACKS);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let module = crate::compile_program_coverage(&parsed.program).expect("coverage lowering");
    for line in [4, 5] {
        let owners = callback_header_owners(&module, line);
        assert_eq!(owners.len(), 1, "header {line}: {module}");
        assert!(
            owners.iter().all(|owner| owner.contains("@__closure_fn_")),
            "header {line} must count invocation, not creation: {owners:?}"
        );
    }
}

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
    let expected = std::collections::BTreeSet::from([3, 4, 5, 6, 7, 8, 15, 16, 19, 23, 26]);
    assert_eq!(coverable_lines(&parsed.program), expected);
    let mut cg = coverage_codegen();
    cg.cov_seed(&parsed.program);
    let module = cg.render();
    for line in expected {
        let global = format!("@__osp_cov_hits.{line} = internal global i64 0");
        assert!(module.contains(&global), "missing line {line}:\n{module}");
    }
    assert_eq!(module.matches("internal global i64 0").count(), 11);
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
        std::collections::BTreeSet::from([1, 3, 4, 7, 8])
    );
}

/// `steps.ospml` flattened by `osprey-project`; mapper on line 7, uncalled on line 9.
const INLINED_RECORD_MAPPER: &str = "type Item =\n    value : string\n\ntype Section =\n    records : List<Item>\n\nentries section = mapList (section.records, \\item => Item(value = item.value))\ngather sections = mapList (sections, entries)\nuncalled item = Item(value = item.value)\n\nmain () = gather [Section(records = [Item(value = \"first\")])]\n";
/// The real `steps.ospml`; its private record mapper sits on line 11.
const MODULE_STEPS: &str = "// [UI-TESTING-CALLBACK-COVERAGE]\nnamespace proof\n\ntype Item =\n    value : string\n\ntype Section =\n    records : List<Item>\n\nmodule Steps\n    entries section = mapList (section.records, \\item => Item(value = item.value))\n    (** Transform every record through a function value. *)\n    export gather sections = mapList (sections, entries)\n";
#[test]
fn higher_order_record_mapper_header_counts_when_the_mapper_runs() {
    let parsed = osprey_syntax::parse_program_for_path("mapper.ospml", INLINED_RECORD_MAPPER);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let module = crate::compile_program_coverage(&parsed.program).expect("coverage lowering");
    let owners = callback_header_owners(&module, 7);
    assert_eq!(owners.len(), 1, "mapper header 7: {owners:?}\n{module}");
    assert_eq!(
        callback_header_owners(&module, 9).len(),
        0,
        "uncalled stays at zero"
    );
    let steps = osprey_syntax::parse_program_for_path("steps.ospml", MODULE_STEPS);
    assert!(
        coverable_lines(&steps.program).contains(&11),
        "line 11 left the denominator"
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
