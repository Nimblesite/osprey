//! Explicit static selection preserves ordinary value-operation contracts.

#[path = "common/effect_execution.rs"]
mod effect_execution;

use effect_execution::assert_flavored_output;
use osprey_ast::{walk_program, AstVisitor, Expr, Program};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::collections::BTreeSet;

fn lower(source: &str, flavor: Flavor) -> Result<Program, Vec<osprey_types::TypeError>> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    osprey_types::lower_static_checked(&parsed.program)
}

fn assert_discharged(source: &str, flavor: Flavor) {
    struct Residue(bool);
    impl AstVisitor for Residue {
        fn expression(&mut self, expression: &Expr) {
            self.0 |= matches!(expression, Expr::Perform { .. } | Expr::Handler { .. });
        }
    }
    let result = lower(source, flavor);
    assert!(result.is_ok(), "{result:?}");
    let Ok(program) = result else { return };
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
        "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nfn invoke(f) = f()\nlet answer = {\n    handle static Read {\n        value => 42\n    }\n    invoke(fetch)\n}\nprint(answer)",
        Flavor::Default,
    );
    assert_discharged(
        "effect Read\n    value : Unit => int\nfetch () = perform Read.value ()\ninvoke f = f ()\nanswer =\n    handle static Read\n        value => 42\n    invoke fetch\nprint answer\n",
        Flavor::Ml,
    );
}

#[test]
fn a_helper_retains_its_dynamic_callers_after_static_specialization() {
    let source = "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nlet a = {\n    handle static Read {\n        value => 42\n    }\n    fetch()\n}\nlet b = {\n    handle Read {\n        value => 7\n    }\n    fetch()\n}\nprint(\"${a}:${b}\")";
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
        "effect Read { value: fn() -> int }\nlet answer = {\n    handle static Read {\n        value => 42\n    }\n    handle static Read {\n        value => perform Read.value()\n    }\n    perform Read.value()\n}\nprint(answer)",
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
            "effect E {{ {operations} }}\nlet answer = {{\n    handle static E {{\n        value => 1 stop => 2\n    }}\n    0\n}}"
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
        let source = format!("effect Runtime {{ read: fn() -> int }}\nfn fetch() = perform Runtime.read()\nfn internal() = {{\n    handle Runtime {{\n        read => 2\n    }}\n    fetch()\n}}\neffect E {{ used: fn() -> int\nunused: fn() -> int }}\nlet answer = {{\n    handle static E {{\n        used => 1 unused => {body}\n    }}\n    perform E.used()\n}}");
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
        let source = format!("effect E {{ used: fn() -> int unused: fn() -> Unit }}\nfn emit() = print(\"bad\")\nfn invoke(f) = f(\"bad\")\nlet alias = print\nlet answer = {{\n    handle static E {{\n        used => 1 unused => {body}\n    }}\n    perform E.used()\n}}");
        let result = lower(&source, Flavor::Default);
        assert!(
            result.as_ref().is_err_and(|errors| errors
                .iter()
                .any(|error| error.message.contains("runtime builtins: print"))),
            "{result:?}"
        );
    }
    assert_discharged("effect E { value: fn() -> int }\nlet print = fn(x) => x\nlet answer = {\n    handle static E {\n        value => print(42)\n    }\n    perform E.value()\n}", Flavor::Default);
}

#[test]
fn generic_unmarked_effects_can_be_selected_statically() {
    assert_discharged(
        "effect Echo<T> { echo: fn(T) -> T }\nlet answer = {\n    handle static Echo<int> {\n        echo value => value\n    }\n    perform Echo<int>.echo(42)\n}\nprint(answer)",
        Flavor::Default,
    );
    assert_discharged(
        "effect Echo<T> { echo: fn(T) -> T }\nlet answer = {\n    handle static Echo<int> {\n        echo value => value\n    }\n    perform Echo.echo(42)\n}\nprint(answer)",
        Flavor::Default,
    );
    assert_discharged(
        "effect Echo T\n    echo : T => T\nanswer =\n    handle static Echo<int>\n        echo value => value\n    perform Echo.echo 42\nprint answer\n",
        Flavor::Ml,
    );
    assert_flavored_output(
        "inferred_static_instance",
        "osp",
        "effect Echo<T> { echo: fn(T) -> T }\nlet answer = {\n    handle static Echo<int> {\n        echo value => value\n    }\n    perform Echo.echo(42)\n}\nprint(answer)",
        "42\n",
    );
    assert_flavored_output(
        "inferred_static_instance",
        "ospml",
        "effect Echo T\n    echo : T => T\nanswer =\n    handle static Echo<int>\n        echo value => value\n    perform Echo.echo 42\nprint answer\n",
        "42\n",
    );
}

#[test]
fn inferred_record_effect_arguments_discharge_at_the_resolved_instantiation() {
    assert_discharged(
        "type Point = { x: int, y: int }\neffect Echo<T> { echo: fn(T) -> T }\nlet answer = {\n    handle static Echo<Point> {\n        echo value => value\n    }\n    perform Echo.echo(Point { x: 42, y: 1 })\n}\nprint(answer.x)",
        Flavor::Default,
    );
    assert_discharged(
        "type Point =\n    x : int\n    y : int\neffect Echo T\n    echo : T => T\nanswer =\n    handle static Echo<Point>\n        echo value => value\n    perform Echo.echo (Point(x = 42, y = 1))\nprint answer.x\n",
        Flavor::Ml,
    );
    assert_flavored_output(
        "inferred_static_record_instance",
        "osp",
        "type Point = { x: int, y: int }\neffect Echo<T> { echo: fn(T) -> T }\nlet answer = {\n    handle static Echo<Point> {\n        echo value => value\n    }\n    perform Echo.echo(Point { x: 42, y: 1 })\n}\nprint(answer.x)",
        "42\n",
    );
    assert_flavored_output(
        "inferred_static_record_instance",
        "ospml",
        "type Point =\n    x : int\n    y : int\neffect Echo T\n    echo : T => T\nanswer =\n    handle static Echo<Point>\n        echo value => value\n    perform Echo.echo (Point(x = 42, y = 1))\nprint answer.x\n",
        "42\n",
    );
}

#[test]
fn staged_generic_records_keep_distinct_effect_identities() {
    let header = "type Box<T> = { item: T }\neffect Echo<T> { echo: fn(T) -> T }\n";
    let matching = format!("{header}let answer = {{\n    handle static Echo<Box<int>> {{ echo value => value }}\n    perform Echo.echo(Box {{ item: 42 }})\n}}\nprint(answer.item)");
    assert_discharged(&matching, Flavor::Default);
    let mismatched = format!("{header}fn fetchString() = perform Echo.echo(Box {{ item: \"wrong\" }})\nlet answer = {{\n    handle static Echo<Box<int>> {{ echo value => value }}\n    fetchString()\n}}\nprint(answer.item)");
    let result = lower(&mismatched, Flavor::Default);
    assert!(
        result.as_ref().is_err_and(|errors| errors
            .iter()
            .any(|error| error.message.contains("Echo<{ item: string }>.echo"))),
        "{result:?}"
    );
}

#[test]
fn inferred_union_effect_arguments_discharge_at_the_resolved_instantiation() {
    let source = "type Choice<T> = Some { value: T } | None\neffect Echo<T> { echo: fn(T) -> T }\nlet answer = {\n    handle static Echo<Choice<int>> { echo value => value }\n    perform Echo.echo(Some { value: 42 })\n}\nprint(match answer { Some { value } => value None => 0 })";
    assert_discharged(source, Flavor::Default);
}

#[test]
fn local_closures_and_alias_chains_keep_captures_when_specialized() {
    assert_discharged(
        "effect Read { value: fn() -> int }\nfn invoke(f) = f()\nfn main() = {\nlet prefix = \"captured\"\nlet closure = fn() => \"${prefix}:${perform Read.value()}\"\nlet alias = closure\nlet answer = {\n    handle static Read {\n        value => 42\n    }\n    invoke(alias)\n}\nprint(answer)\n}",
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
fn a_kernel_rejects_an_unresolved_callback_requirement() {
    for body in [
        "kernel Read value => 42 in callback()",
        "{ let alias = callback\nkernel Read value => 42 in alias() }",
        "kernel Read value => 42 in invoke(callback)",
    ] {
        let source = format!(
            "effect Read {{ value: fn() -> int }}\nfn invoke(f) = f()\nfn run(callback) = {body}"
        );
        let parsed = parse_program_with_flavor(&source, Flavor::Default);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let errors = osprey_ast::stage::validate(&parsed.program);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("<unknown>")),
            "a callback with no published effect row must not pass a kernel gate: {errors:?}"
        );
    }
    let ml = "effect Read\n    value : Unit => int\n\ninvoke f = f ()\nrun callback =\n    kernel\n        Read value => 42\n    in invoke callback\n";
    let parsed = parse_program_with_flavor(ml, Flavor::Ml);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let errors = osprey_ast::stage::validate(&parsed.program);
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("<unknown>")),
        "ML callbacks must obey the same kernel stage gate: {errors:?}"
    );
}

#[test]
fn static_selection_and_dynamic_shadowing_execute_with_the_selected_answers() {
    let source = "effect Read { value: fn() -> int }\nfn fetch() = perform Read.value()\nfn invoke(f) = f()\nfn main() = {\nlet prefix = \"captured\"\nlet closure = fn() => \"${prefix}:${perform Read.value()}\"\nlet alias = closure\nlet a = {\n    handle static Read {\n        value => 42\n    }\n    invoke(alias)\n}\nlet b = {\n    handle Read {\n        value => 7\n    }\n    fetch()\n}\nlet c = {\n    handle static Read {\n        value => 9\n    }\n    handle Read {\n        value => 3\n    }\n    fetch()\n}\nprint(\"${a}|${b}|${c}\")\n}";
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

/// Every `Effect.op` a program still requests.
struct RequestedOperations(BTreeSet<String>);

impl AstVisitor for RequestedOperations {
    fn expression(&mut self, expression: &Expr) {
        if let Expr::Perform {
            effect, operation, ..
        } = expression
        {
            let _ = self.0.insert(format!("{effect}.{operation}"));
        }
    }
}

/// The statically lowered program, which must still pass the checker.
fn lowered(source: &str, flavor: Flavor) -> Option<Program> {
    let result = lower(source, flavor);
    assert!(result.is_ok(), "{result:?}");
    let program = result.ok()?;
    let errors = osprey_types::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
    Some(program)
}

/// The requests left standing after static lowering.
fn residual_operations(source: &str, flavor: Flavor) -> Vec<String> {
    let mut requested = RequestedOperations(BTreeSet::new());
    if let Some(program) = lowered(source, flavor) {
        walk_program(&program, &mut requested);
    }
    requested.0.into_iter().collect()
}

/// The declared effect row of every specialization of `original`.
fn specialized_rows(program: &Program, original: &str) -> Vec<Vec<String>> {
    let prefix = format!("{original}__stage");
    let mut rows: Vec<Vec<String>> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            osprey_ast::Stmt::Function { name, effects, .. } if name.starts_with(&prefix) => {
                Some(effects.iter().map(|effect| effect.name.clone()).collect())
            }
            _ => None,
        })
        .collect();
    rows.sort();
    rows
}

/// An inner runtime handler supplying only `first` shadows `first` alone;
/// `second` still reaches the enclosing static interpretation directly, through
/// a helper called under the runtime handler, and through a helper that installs
/// the runtime handler itself ([STAGE-LOWER-ORDER]).
const PARTIAL_SHADOWING_DEFAULT: &str = "effect Pair { first: fn() -> int second: fn() -> int }
fn pairTotal() = {
    let first = perform Pair.first()
    let second = perform Pair.second()
    first + second ?: 0
}
fn firstOnly() = {
    handle Pair {
        first => 2
    }
    pairTotal()
}
let answer = {
    handle static Pair {
        first => 1 second => 40
    }
    let direct = {
        handle Pair {
            first => 2
        }
        (perform Pair.first() + perform Pair.second()) ?: 0
    }
    let helper = {
        handle Pair {
            first => 2
        }
        pairTotal()
    }
    let installer = firstOnly()
    \"${direct}|${helper}|${installer}\"
}
print(answer)
";

const PARTIAL_SHADOWING_ML: &str = "effect Pair
    first : Unit => int
    second : Unit => int

pairTotal () =
    first = perform Pair.first ()
    second = perform Pair.second ()
    first + second ?: 0

firstOnly () =
    handle Pair
        first => 2
    pairTotal ()

answer =
    handle static Pair
        first => 1
        second => 40
    direct =
        handle Pair
            first => 2
        (perform Pair.first () + perform Pair.second ()) ?: 0
    helper =
        handle Pair
            first => 2
        pairTotal ()
    installer = firstOnly ()
    \"${direct}|${helper}|${installer}\"

print answer
";

#[test]
fn a_partial_dynamic_handler_shadows_only_the_operations_it_supplies() {
    for (extension, source, flavor) in [
        ("osp", PARTIAL_SHADOWING_DEFAULT, Flavor::Default),
        ("ospml", PARTIAL_SHADOWING_ML, Flavor::Ml),
    ] {
        assert_eq!(
            residual_operations(source, flavor),
            ["Pair.first"],
            "{extension}"
        );
    }
}

#[test]
fn partial_dynamic_shadowing_executes_with_the_outer_static_answers() {
    assert_flavored_output(
        "partial_static_shadowing",
        "osp",
        PARTIAL_SHADOWING_DEFAULT,
        "42|42|42\n",
    );
    assert_flavored_output(
        "partial_static_shadowing",
        "ospml",
        PARTIAL_SHADOWING_ML,
        "42|42|42\n",
    );
}

/// `total` declares its row. The copy reached under the whole static
/// interpretation has nothing left to declare; the copy reached under the
/// partial runtime handler still performs `Pair.first`, so it keeps `!Pair`
/// ([STAGE-RESIDUE]).
const DECLARED_ROW_SHADOWING: &str = "effect Pair { first: fn() -> int second: fn() -> int }
fn total() !Pair = (perform Pair.first() + perform Pair.second()) ?: 0
let answer = {
    handle static Pair {
        first => 1 second => 40
    }
    let shadowed = {
        handle Pair {
            first => 2
        }
        total()
    }
    \"${total()}|${shadowed}\"
}
print(answer)
";

#[test]
fn a_specialization_keeps_its_declared_row_while_an_operation_stays_dynamic() {
    let program = lowered(DECLARED_ROW_SHADOWING, Flavor::Default);
    assert!(program.is_some());
    let rows = program.map(|program| specialized_rows(&program, "total"));
    assert_eq!(rows, Some(vec![Vec::new(), vec!["Pair".to_owned()]]));
    assert_flavored_output(
        "declared_row_shadowing",
        "osp",
        DECLARED_ROW_SHADOWING,
        "41|42\n",
    );
}

/// A runtime handler for ANOTHER effect shadows nothing of the static
/// interpretation around it: its body, a helper it calls and its return clause
/// all still receive the static answer ([STAGE-LOWER-DYNAMIC]).
const UNRELATED_RUNTIME_HANDLER_DEFAULT: &str = "effect Read { value: fn() -> int }
effect Tag { label: fn() -> string }
fn fetch() = perform Read.value()
let answer = {
    handle static Read {
        value => 42
    }
    handle Tag {
        label => \"n\"
        return text => \"${text}:${perform Read.value()}\"
    }
    \"${perform Tag.label()}=${fetch()}\"
}
print(answer)
";

const UNRELATED_RUNTIME_HANDLER_ML: &str = "effect Read
    value : Unit => int
effect Tag
    label : Unit => string
fetch () = perform Read.value ()
answer =
    handle static Read
        value => 42
    handle Tag
        label => \"n\"
        return text => \"${text}:${perform Read.value ()}\"
    fetched = fetch ()
    \"${perform Tag.label ()}=${fetched}\"
print answer
";

#[test]
fn a_runtime_handler_for_another_effect_keeps_the_static_interpretation() {
    for (extension, source, flavor) in [
        ("osp", UNRELATED_RUNTIME_HANDLER_DEFAULT, Flavor::Default),
        ("ospml", UNRELATED_RUNTIME_HANDLER_ML, Flavor::Ml),
    ] {
        assert_eq!(
            residual_operations(source, flavor),
            ["Tag.label"],
            "{extension}"
        );
        assert_flavored_output("unrelated_runtime_handler", extension, source, "n=42:42\n");
    }
}
