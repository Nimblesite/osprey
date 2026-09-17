//! Explicit static selection preserves ordinary value-operation contracts.

use osprey_ast::{walk_program, AstVisitor, Expr, Program};
use osprey_syntax::{parse_program_with_flavor, Flavor};

fn lower(source: &str, flavor: Flavor) -> Result<Program, Vec<osprey_types::TypeError>> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    osprey_types::lower_static_checked(&parsed.program)
}

fn assert_discharged(source: &str, flavor: Flavor) {
    let result = lower(source, flavor);
    assert!(result.is_ok(), "{result:?}");
    let Ok(program) = result else { return };
    struct Residue(bool);
    impl AstVisitor for Residue {
        fn expression(&mut self, expression: &Expr) {
            self.0 |= matches!(expression, Expr::Perform { .. } | Expr::Handler { .. });
        }
    }
    let mut residue = Residue(false);
    walk_program(&program, &mut residue);
    assert!(
        !residue.0,
        "statically selected operations remain: {program:?}"
    );
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn ordinary_value_effects_discharge_directly_and_through_helpers_and_callbacks() {
    assert_discharged(
        "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nfn invoke(f) = f()\nlet answer = handle static Read value => 42 in invoke(fetch)\nprint(answer)",
        Flavor::Default,
    );
    assert_discharged(
        "effect Read\n    value : Unit => int\nfetch () = perform Read.value ()\ninvoke f = f ()\nanswer = handle static Read\n    value => 42\nin invoke fetch\nprint answer\n",
        Flavor::Ml,
    );
}

#[test]
fn a_helper_retains_its_dynamic_callers_after_static_specialization() {
    let source = "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nlet a = handle static Read value => 42 in fetch()\nlet b = handle Read value => 7 in fetch()\nprint(\"${a}:${b}\")";
    let result = lower(source, Flavor::Default);
    assert!(result.is_ok(), "{result:?}");
    if let Ok(program) = result {
        assert!(program.statements.iter().any(|statement| matches!(statement, osprey_ast::Stmt::Function { name, .. } if name == "fetch")));
        let errors = osprey_types::check_program(&program);
        assert!(errors.is_empty(), "{errors:?}");
        let result = osprey_codegen::compile_program(&program);
        assert!(result.is_ok(), "{result:?}");
    }
}

#[test]
fn static_arm_forwarding_uses_the_outer_interpretation() {
    assert_discharged(
        "effect Read { value: fn() -> int }\nlet answer = handle static Read value => 42 in handle static Read value => perform Read.value() in perform Read.value()\nprint(answer)",
        Flavor::Default,
    );
}

#[test]
fn mixed_and_control_effects_cannot_be_selected_statically() {
    for operations in [
        "control stop: fn() -> int",
        "value: fn() -> int\ncontrol stop: fn() -> int",
    ] {
        let source = format!(
            "effect E {{ {operations} }}\nlet answer = handle static E value => 1 stop => 2 in 0"
        );
        let parsed = parse_program_with_flavor(&source, Flavor::Default);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let result = osprey_ast::stage::discharge(&parsed.program);
        assert!(
            result.as_ref().is_err_and(|errors| errors
                .iter()
                .any(|error| error.message.contains("all value operations"))),
            "{result:?}"
        );
    }
}

#[test]
fn unused_static_arms_reject_transitive_dynamic_requests() {
    for body in ["perform Runtime.read()", "fetch()", "internal()"] {
        let source = format!("effect Runtime {{ read: fn() -> int }}\nfn fetch() = perform Runtime.read()\nfn internal() = handle Runtime read => 2 in fetch()\neffect E {{ used: fn() -> int\nunused: fn() -> int }}\nlet answer = handle static E used => 1 unused => {body} in perform E.used()");
        let result = lower(&source, Flavor::Default);
        assert!(
            result.as_ref().is_err_and(|errors| errors
                .iter()
                .any(|error| error.message.contains("Runtime.read"))),
            "{result:?}"
        );
    }
}

#[test]
fn static_arms_retain_resolved_builtin_obligations() {
    for body in [
        "print(\"bad\")",
        "emit()",
        "alias(\"bad\")",
        "invoke(print)",
    ] {
        let source = format!("effect E {{ used: fn() -> int unused: fn() -> Unit }}\nfn emit() = print(\"bad\")\nfn invoke(f) = f(\"bad\")\nlet alias = print\nlet answer = handle static E used => 1 unused => {body} in perform E.used()");
        let result = lower(&source, Flavor::Default);
        assert!(
            result.as_ref().is_err_and(|errors| errors
                .iter()
                .any(|error| error.message.contains("runtime builtins: print"))),
            "{result:?}"
        );
    }
    assert_discharged("effect E { value: fn() -> int }\nlet print = fn(x) => x\nlet answer = handle static E value => print(42) in perform E.value()", Flavor::Default);
}

#[test]
fn generic_unmarked_effects_can_be_selected_statically() {
    assert_discharged(
        "effect Echo<T> { echo: fn(T) -> T }\nlet answer = handle static Echo<int> echo value => value in perform Echo<int>.echo(42)\nprint(answer)",
        Flavor::Default,
    );
}

#[test]
fn local_closures_and_alias_chains_keep_captures_when_specialized() {
    assert_discharged(
        "effect Read { value: fn() -> int }\nfn invoke(f) = f()\nfn main() = {\nlet prefix = \"captured\"\nlet closure = fn() => \"${prefix}:${perform Read.value()}\"\nlet alias = closure\nlet answer = handle static Read value => 42 in invoke(alias)\nprint(answer)\n}",
        Flavor::Default,
    );
}

#[test]
fn kernel_selection_accepts_an_ordinary_value_effect() {
    assert_discharged(
        "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nlet answer = kernel Read value => 42 in fetch()\nprint(answer)",
        Flavor::Default,
    );
}

#[test]
fn static_selection_and_dynamic_shadowing_execute_with_the_selected_answers() {
    let source = "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nfn invoke(f) = f()\nfn main() = {\nlet prefix = \"captured\"\nlet closure = fn() => \"${prefix}:${perform Read.value()}\"\nlet alias = closure\nlet a = handle static Read value => 42 in invoke(alias)\nlet b = handle Read value => 7 in fetch()\nlet c = handle static Read value => 9 in handle Read value => 3 in fetch()\nprint(\"${a}|${b}|${c}\")\n}";
    let path = std::env::temp_dir().join(format!(
        "osprey-static-selection-{}.osp",
        std::process::id()
    ));
    let written = std::fs::write(&path, source);
    assert!(written.is_ok(), "{written:?}");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&path)
        .arg("--run")
        .output();
    let _ = std::fs::remove_file(&path);
    assert!(result.is_ok(), "{result:?}");
    if let Ok(result) = result {
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&result.stdout), "captured:42|7|3\n");
    }
}
