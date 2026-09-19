//! Run reusable handler values through the real compiler and native runtimes.
//! Implements [EFFECTS-HANDLER-VALUE].

use std::path::Path;
use std::process::Command;

fn execute(
    name: &str,
    extension: &str,
    source: &str,
    memory: &str,
) -> std::io::Result<std::process::Output> {
    let directory = std::env::temp_dir().join(format!("osprey_handlers_{}", std::process::id()));
    assert!(std::fs::create_dir_all(&directory).is_ok());
    let path = directory.join(format!("{name}_{memory}.{extension}"));
    assert!(std::fs::write(&path, source).is_ok());
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .arg(&path)
        .args(["--run", &format!("--memory={memory}")])
        .output()
}

fn assert_output(name: &str, extension: &str, source: &str, expected: &str) {
    for memory in ["default", "gc", "arc"] {
        let result = execute(name, extension, source, memory);
        assert!(result.is_ok(), "could not run compiler: {result:?}");
        let Ok(output) = result else { return };
        assert!(
            output.status.success(),
            "{name}/{memory}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{name}/{memory}"
        );
    }
}

#[test]
fn returned_handlers_keep_their_captures_and_callable_argument_types() {
    let expected = include_str!("../../../examples/handlers/handlers.expectedoutput");
    assert_output(
        "factory",
        "osp",
        include_str!("../../../examples/handlers/handlers.osp"),
        expected,
    );
    assert_output(
        "factory",
        "ospml",
        include_str!("../../../examples/handlers/handlers.ospml"),
        expected,
    );
}

#[test]
fn returned_handler_closures_preserve_shared_and_isolated_state() {
    // This existing-syntax equivalent isolates the runtime requirement from
    // the new parser: the promoted cell must survive its maker's return.
    assert_output(
        "state",
        "osp",
        r#"
effect Count { next: fn() -> int }
fn counter(initial) -> fn(fn() -> int) -> int = {
    mut count = initial
    (|action| => {
        handle Count {
            next => {
                count = (count + 1) ?: count
                count
            }
        }
        action()
    })
}
let a = counter(0)
let b = counter(10)
let first = a(|| => perform Count.next())
let second = a(|| => perform Count.next())
let separate = b(|| => perform Count.next())
print("${first},${second},${separate}")
"#,
        "1,2,11\n",
    );
}

#[test]
fn handler_values_are_callable_in_both_flavors_without_in_or_do() {
    let default = r#"
effect Ask { value: fn() -> int }
let answer = handler Ask { value => 41 }
fn work() = (perform Ask.value() + 1) ?: 0
let first = answer(work)
let second = answer(work)
print("${first}\n${second}")
"#;
    let ml = r#"
effect Ask
    value : Unit => int
answer = handler Ask
    value => 41
work () = (perform Ask.value () + 1) ?: 0
first = answer work
second = answer work
print "${first}\n${second}"
"#;
    assert_output("callable", "osp", default, "42\n42\n");
    assert_output("callable", "ospml", ml, "42\n42\n");
}

#[test]
fn handler_values_and_generic_lambdas_can_be_passed_to_functions() {
    let default = r#"
fn apply(g) = g(|x| => (x + 1) ?: 0)
fn exact(g) = (g(|x| => x) + 1) ?: 0
fn text(g) = g(|x| => "v${x}") + "!"
let call = |f| => f(1)
effect Reader { name: fn() -> string }
fn reading(person) = handler Reader { name => person }
fn greet() = "Hello, " + perform Reader.name() + "!"
fn run(h) = h(greet)
let ada = reading("Ada")
let grace = reading("Grace")
let number = apply(call)
let concrete = exact(call)
let string = text(call)
let first = run(ada)
let second = run(grace)
print("${number}/${concrete}/${string}\n${first}\n${second}")
"#;
    let ml = r#"
apply g = g (\x => (x + 1) ?: 0)
exact g = (g (\x => x) + 1) ?: 0
text g = g (\x => "v${x}") + "!"
call = \f => f 1
effect Reader
    name : Unit => string
reading person = handler Reader
    name => person
greet () = "Hello, " + perform Reader.name () + "!"
run h = h greet
ada = reading "Ada"
grace = reading "Grace"
number = apply call
concrete = exact call
string = text call
first = run ada
second = run grace
print "${number}/${concrete}/${string}\n${first}\n${second}"
"#;
    assert_output(
        "passed",
        "osp",
        default,
        "2/2/v1!\nHello, Ada!\nHello, Grace!\n",
    );
    assert_output(
        "passed",
        "ospml",
        ml,
        "2/2/v1!\nHello, Ada!\nHello, Grace!\n",
    );
}
