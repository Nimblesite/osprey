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
    return n => satAdd(n, perform Read.value())
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
    return n => satAdd n (perform Read.value ())
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
    satAdd(satAdd(a, b), perform Mixed.plain())
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
    satAdd (satAdd a b) (perform Mixed.plain ())
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
    satAdd(perform Read.value(), n)
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
    satAdd (perform Read.value ()) n
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

/// A file-scope handler value whose arms and return clause read file-scope
/// `let`s, applied inside a function, for a value and a control operation. It
/// failed with "codegen: unknown name `base`": the handler is inlined into the
/// function, and nothing gave the bindings its body reads module storage.
#[test]
fn file_scope_handler_values_read_file_scope_bindings_inside_functions() {
    assert_both(
        "file_scope_reads",
        r#"
effect Ask { value: fn() -> int }
effect Pick { control value: fn() -> int }
let base = 41
let prefix = "done="
let tagged = handler Ask {
    value => base
    return n => "${prefix}${n}"
}
let resumed = handler Pick {
    value => resume(base)
    return n => "${prefix}${n}!"
}
fn report() = "${tagged(|| => perform Ask.value())} ${resumed(|| => perform Pick.value())}"
print(report())
"#,
        r#"
effect Ask
    value : Unit => int
effect Pick
    control value : Unit => int
base = 41
prefix = "done="
tagged = handler Ask
    value => base
    return n => "${prefix}${n}"
resumed = handler Pick
    value => resume base
    return n => "${prefix}${n}!"
report () = "${tagged (\() => perform Ask.value ())} ${resumed (\() => perform Pick.value ())}"
print (report ())
"#,
        "done=41 done=41!\n",
    );
}

/// A handler value's clauses close over the bindings in scope where it is
/// DEFINED. Applying it where a parameter, a local or a block binding shadows
/// one of those names must not rebind it: the program exited zero printing
/// `param41 local41 block41` — the caller's values, silently.
#[test]
fn file_scope_handler_values_ignore_a_callers_shadowing_bindings() {
    assert_both(
        "shadowed_reads",
        r#"
effect Ask { value: fn() -> int }
let prefix = "file"
let tagged = handler Ask {
    value => 41
    return n => "${prefix}${n}"
}
fn viaParameter(prefix) = tagged(|| => perform Ask.value())
fn viaLocal() = {
    let prefix = "local"
    tagged(|| => perform Ask.value())
}
let inBlock = {
    let prefix = "block"
    tagged(|| => perform Ask.value())
}
print("${viaParameter("param")} ${viaLocal()} ${inBlock}")
"#,
        r#"
effect Ask
    value : Unit => int
prefix = "file"
tagged = handler Ask
    value => 41
    return n => "${prefix}${n}"
viaParameter prefix = tagged (\() => perform Ask.value ())
viaLocal () =
    prefix = "local"
    tagged (\() => perform Ask.value ())
inBlock =
    prefix = "block"
    tagged (\() => perform Ask.value ())
print "${viaParameter "param"} ${viaLocal ()} ${inBlock}"
"#,
        "file41 file41 file41\n",
    );
}
