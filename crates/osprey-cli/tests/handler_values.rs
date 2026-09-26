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
fn open_row_callback_effects_reach_the_selected_handler_in_both_flavors() {
    assert_output(
        "open_row",
        "osp",
        "effect Log { write: fn(string) -> Unit }\nfn invoke(callback: fn() -> Unit) -> Unit !e = callback()\nfn report() -> Unit !Log = perform Log.write(\"sent\")\nlet h = handler Log { write message => print(message) }\nh(|| => invoke(report))\n",
        "sent\n",
    );
    assert_output(
        "open_row",
        "ospml",
        "effect Log\n    write : string => Unit\ninvoke : (Unit -> Unit) -> Unit !e\ninvoke callback = callback ()\nreport : Unit -> Unit !Log\nreport () = perform Log.write \"sent\"\nh = handler Log\n    write message => print message\n_ = h (\\() => invoke report)\n",
        "sent\n",
    );
}

#[test]
fn the_same_open_row_helper_runs_under_static_and_dynamic_handlers() {
    let expected = include_str!("../../../examples/handlers/staged-rows.expectedoutput");
    assert_output(
        "staged_rows",
        "osp",
        include_str!("../../../examples/handlers/staged-rows.osp"),
        expected,
    );
    assert_output(
        "staged_rows",
        "ospml",
        include_str!("../../../examples/handlers/staged-rows.ospml"),
        expected,
    );
}

#[test]
fn a_fixed_open_row_keeps_its_own_and_callback_operations_separate() {
    assert_output(
        "fixed_open_row",
        "osp",
        "effect Log { write: fn(string) -> Unit }\neffect Audit { write: fn(string) -> Unit }\nfn combine(callback: fn() -> Unit) -> Unit ![Log | e] = {\n    perform Log.write(\"fixed\")\n    callback()\n}\nfn report() -> Unit !Audit = perform Audit.write(\"tail\")\nlet logging = handler Log { write message => print(message) }\nlet auditing = handler Audit { write message => print(message) }\nlogging(|| => auditing(|| => combine(report)))\n",
        "fixed\ntail\n",
    );
    assert_output(
        "fixed_open_row",
        "ospml",
        "effect Log\n    write : string => Unit\neffect Audit\n    write : string => Unit\ncombine : (Unit -> Unit) -> Unit ![Log | e]\ncombine callback =\n    perform Log.write \"fixed\"\n    callback ()\nreport : Unit -> Unit !Audit\nreport () = perform Audit.write \"tail\"\nlogging = handler Log\n    write message => print message\nauditing = handler Audit\n    write message => print message\n_ = logging (\\() => auditing (\\() => combine report))\n",
        "fixed\ntail\n",
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
fn inferred_stateful_handler_factories_keep_their_concrete_callers() {
    let default = r#"
effect Count { next: fn() -> int }
fn counter(initial) = {
    mut count = initial
    handler Count {
        next => {
            count = (count + 1) ?: count
            count
        }
    }
}


let a = counter(0)
let b = counter(10)
let c = counter(20)
let first = a(|| => perform Count.next())
let second = a(|| => perform Count.next())
let separate = b(|| => perform Count.next())
let text = c(|| => "ready")
print("${first},${second},${separate},${text}")
"#;
    let ml = r#"
effect Count
    next : Unit => int
counter initial =
    mut count = initial
    handler Count
        next =>
            count := (count + 1) ?: count
            count
a = counter 0
b = counter 10
c = counter 20
first = a (\() => perform Count.next ())
second = a (\() => perform Count.next ())
separate = b (\() => perform Count.next ())
text = c (\() => "ready")
print "${first},${second},${separate},${text}"
"#;
    assert_output("inferred_factory", "osp", default, "1,2,11,ready\n");
    assert_output("inferred_factory", "ospml", ml, "1,2,11,ready\n");
    for (source, original, replacement, flavor) in [
        (
            default,
            "let second = a(|| => perform Count.next())",
            "let second = a(|| => \"wrong\")",
            osprey_syntax::Flavor::Default,
        ),
        (
            ml,
            "second = a (\\() => perform Count.next ())",
            "second = a (\\() => \"wrong\")",
            osprey_syntax::Flavor::Ml,
        ),
    ] {
        let mismatched = source.replace(original, replacement);
        let parsed = osprey_syntax::parse_program_with_flavor(&mismatched, flavor);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let errors = osprey_types::check_program(&parsed.program);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("cannot unify")),
            "one shared handler value must not change its callback ABI: {errors:?}"
        );
    }
}

#[test]
fn branched_stateful_handler_factories_keep_one_callable_abi() {
    let default = r#"
effect Count { next: fn() -> int }
fn counter(initial) = {
    mut count = initial
    if true {
        handler Count { next => {
            count = (count + 1) ?: count
            count
        } }
    } else {
        handler Count { next => {
            count = (count + 1) ?: count
            count
        } }
    }
}
let a = counter(0)
let first = a(|| => perform Count.next())
let second = a(|| => perform Count.next())
print("${first}:${second}")
"#;
    let ml = r#"
effect Count
    next : Unit => int
counter initial =
    mut count = initial
    match true
        true => handler Count
            next =>
                count := (count + 1) ?: count
                count
        false => handler Count
            next =>
                count := (count + 1) ?: count
                count
a = counter 0
first = a (\() => perform Count.next ())
second = a (\() => perform Count.next ())
print "${first}:${second}"
"#;
    for (source, original, replacement, flavor) in [
        (
            default,
            "let second = a(|| => perform Count.next())",
            "let second = a(|| => \"wrong\")",
            osprey_syntax::Flavor::Default,
        ),
        (
            ml,
            "second = a (\\() => perform Count.next ())",
            "second = a (\\() => \"wrong\")",
            osprey_syntax::Flavor::Ml,
        ),
    ] {
        let parsed = osprey_syntax::parse_program_with_flavor(
            &source.replace(original, replacement),
            flavor,
        );
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let errors = osprey_types::check_program(&parsed.program);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("cannot unify")),
            "one branched handler value must retain one callback ABI: {errors:?}"
        );
    }
    assert_output("branched_factory", "osp", default, "1:2\n");
    assert_output("branched_factory", "ospml", ml, "1:2\n");
}

#[test]
fn pure_handler_factory_values_remain_polymorphic() {
    let default = "effect Read { value: fn() -> int }\nfn make() = handler Read { value => 42 }\nlet h = make()\nlet n = h(|| => perform Read.value())\nlet text = h(|| => \"ready\")\nprint(\"${n}:${text}\")";
    let ml = "effect Read\n    value : Unit => int\nmake () = handler Read\n    value => 42\nh = make ()\nn = h (\\() => perform Read.value ())\ntext = h (\\() => \"ready\")\nprint \"${n}:${text}\"\n";
    assert_output("polymorphic_handler_factory", "osp", default, "42:ready\n");
    assert_output("polymorphic_handler_factory", "ospml", ml, "42:ready\n");
}

#[test]
fn function_valued_generic_operations_keep_their_instance_through_discharge() {
    // The operation result is itself a callable. Its type must identify the
    // same generic effect at the handler, perform and static rewrite sites.
    for (stage, name) in [("", "dynamic"), ("static ", "static")] {
        let default = format!(
            "effect Carry<T> {{ fetch: fn() -> T }}\n\
             let h = handler {stage}Carry<fn(int) -> int> {{ fetch => |x| => (x + 1) ?: 0 }}\n\
             let f: fn(int) -> int = h(|| => perform Carry.fetch())\n\
             print(f(41))\n"
        );
        let ml = format!(
            "effect Carry T\n    fetch : Unit => T\n\
             h = handler {stage}Carry<(int -> int)>\n    fetch => \\x => (x + 1) ?: 0\n\
             f : int -> int\nf = h (\\() => perform Carry.fetch ())\n\
             print (f 41)\n"
        );
        assert_output(&format!("generic_function_{name}"), "osp", &default, "42\n");
        assert_output(&format!("generic_function_{name}"), "ospml", &ml, "42\n");
    }
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
fn handler_values_can_return_functions_supplied_by_effect_arms() {
    assert_output(
        "returned_function",
        "osp",
        r"
effect Read { read: fn() -> fn(int) -> int }
let h = handler Read { read => |n| => (n + 1) ?: 0 }
let action = h(|| => perform Read.read())
print(action(41))
",
        "42\n",
    );
    assert_output(
        "returned_function",
        "ospml",
        r"
effect Read
    read : Unit => (int -> int)
h = handler Read
    read => \n => (n + 1) ?: 0
action = h (\() => perform Read.read ())
print (action 41)
",
        "42\n",
    );
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
