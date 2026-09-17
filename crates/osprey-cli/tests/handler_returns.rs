//! Normal completion, control answers, and resumption have distinct paths.
//! Implements [EFFECTS-HANDLER-ARMS] and [EFFECTS-RESUME].

#[path = "common/effect_execution.rs"]
mod effect_execution;

use effect_execution::assert_flavored_output;

fn assert_both(name: &str, default: &str, ml: &str, expected: &str) {
    assert_flavored_output(name, "osp", default, expected);
    assert_flavored_output(name, "ospml", ml, expected);
}

#[test]
fn return_clauses_transform_completion_and_resume_but_not_control_answers() {
    assert_flavored_output(
        "handler_returns",
        "osp",
        include_str!("../../../examples/handlers/returns.osp"),
        include_str!("../../../examples/handlers/returns.expectedoutput"),
    );
    assert_flavored_output(
        "handler_returns",
        "ospml",
        include_str!("../../../examples/handlers/returns.ospml"),
        include_str!("../../../examples/handlers/returns.expectedoutput"),
    );
}

#[test]
fn return_requests_forward_outside_their_own_activation() {
    assert_both(
        "return_forward",
        r#"
effect Read { value: fn() -> int }
effect Ask { control value: fn() -> int }
let outer = handler Read { value => 41 }
let inner = handler Read {
    value => 1
    return n => (n + perform Read.value()) ?: 0
}
let controlOuter = handler Ask { value => resume(41) }
let controlInner = handler Ask {
    value => resume(1)
    return n => "${n}:${perform Ask.value()}"
}
print(outer(|| => inner(|| => perform Read.value())))
print(controlOuter(|| => controlInner(|| => perform Ask.value())))
"#,
        r#"
effect Read
    value : Unit => int
effect Ask
    control value : Unit => int
outer = handler Read
    value => 41
inner = handler Read
    value => 1
    return n => (n + perform Read.value ()) ?: 0
controlOuter = handler Ask
    value => resume 41
controlInner = handler Ask
    value => resume 1
    return n => "${n}:${perform Ask.value ()}"
print (outer (\() => inner (\() => perform Read.value ())))
print (controlOuter (\() => controlInner (\() => perform Ask.value ())))
"#,
        "42\n1:41\n",
    );
}

#[test]
fn repeated_deep_resume_and_value_dispatch_transform_completion_once() {
    assert_both(
        "return_deep",
        r#"
effect Mixed { plain: fn() -> int control ask: fn() -> int }
fn work() = {
    let a = perform Mixed.ask()
    let b = perform Mixed.ask()
    (((a + b) ?: 0) + perform Mixed.plain()) ?: 0
}
let h = handler Mixed {
    plain => 1
    ask => "${resume(20)}!"
    return n => "done=${n}"
}
print(h(work))
print(h(|| => 7))
"#,
        r#"
effect Mixed
    plain : Unit => int
    control ask : Unit => int
work () =
    a = perform Mixed.ask ()
    b = perform Mixed.ask ()
    (((a + b) ?: 0) + perform Mixed.plain ()) ?: 0
h = handler Mixed
    plain => 1
    ask => "${resume 20}!"
    return n => "done=${n}"
print (h work)
print (h (\() => 7))
"#,
        "done=41!!\ndone=7\n",
    );
}

#[test]
fn managed_body_answers_transfer_ownership_to_return_clauses() {
    assert_both(
        "return_managed",
        r#"
effect Text { control read: fn() -> string }
let h = handler Text {
    read => resume("hello")
    return text => "${text}!"
}
print(h(|| => "${perform Text.read()} world"))
"#,
        r#"
effect Text
    control read : Unit => string
h = handler Text
    read => resume "hello"
    return text => "${text}!"
print (h (\() => "${perform Text.read ()} world"))
"#,
        "hello world!\n",
    );
}

#[test]
fn static_handler_values_preserve_return_types_and_captures() {
    assert_both(
        "return_static",
        r#"
effect Read { value: fn() -> int }
let n = "done="
let h = handler static Read {
    value => 42
    return answer => "${n}${answer}"
}
let selected = h
let result = selected(|| => {
    let n = 7
    (perform Read.value() + n) ?: 0
})
print(result)
"#,
        r#"
effect Read
    value : Unit => int
n = "done="
h = handler static Read
    value => 42
    return answer => "${n}${answer}"
work () =
    n = 7
    (perform Read.value () + n) ?: 0
selected = h
result = selected work
print result
"#,
        "done=49\n",
    );
}

#[test]
fn return_clauses_keep_outer_obligations_and_check_answer_types() {
    use osprey_syntax::{parse_program_with_flavor, Flavor};
    for (source, expected) in [
        ("effect E { op: fn() -> int }\nlet h = handler E { op => 1 return n => perform E.op() }\nprint(h(|| => 0))", "unhandled"),
        ("effect E { control op: fn() -> int }\nlet h = handler E { op => 1 return n => \"text\" }\nprint(h(|| => perform E.op()))", "control arm"),
        ("effect E { control op: fn() -> int }\nlet h = handler E { op => resume(1) return n => resume(n) }\nprint(h(|| => perform E.op()))", "resume"),
    ] {
        let parsed = parse_program_with_flavor(source, Flavor::Default);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let errors = osprey_types::check_program(&parsed.program);
        assert!(format!("{errors:?}").contains(expected), "{errors:?}");
    }
}

#[test]
fn return_clauses_preserve_callable_body_and_answer_types() {
    assert_both(
        "return_callable",
        r#"
effect Ask { control value: fn() -> int }
let f = || => 7
let produce = handler Ask {
    value => resume(42)
    return n => || => "done=${n}"
}
let consume = handler Ask {
    value => resume(42)
    return f => f()
}
let result = produce(|| => perform Ask.value())
fn work() = {
    let n = perform Ask.value()
    let make = || => "used=${n}"
    make
}
print("${result()}\n${consume(work)}\n${f()}")
"#,
        r#"
effect Ask
    control value : Unit => int
f = \() => 7
produce = handler Ask
    value => resume 42
    return n => \() => "done=${n}"
consume = handler Ask
    value => resume 42
    return f => f ()
result = produce (\() => perform Ask.value ())
work () =
    n = perform Ask.value ()
    \() => "used=${n}"
print "${result ()}\n${consume work}\n${f ()}"
"#,
        "done=42\nused=42\n7\n",
    );
}
