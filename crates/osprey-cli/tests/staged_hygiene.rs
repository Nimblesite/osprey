//! Static discharge preserves lexical bindings and call evaluation order.
//! Implements [STAGE-LOWER] and [STAGE-LOWER-ORDER].

#[path = "common/effect_execution.rs"]
mod effect_execution;

use effect_execution::assert_flavored_output;

fn assert_output(name: &str, source: &str, expected: &str) {
    assert_flavored_output(name, "osp", source, expected);
}

#[test]
fn handler_captures_are_lexical() {
    assert_output(
        "lexical",
        include_str!("../../../examples/handlers/staging-scope.osp"),
        "static=1 dynamic=1\n",
    );
}

#[test]
fn all_operation_arguments_evaluate_once_before_arm_parameters_bind() {
    assert_output(
        "arguments",
        r#"
static effect Pair { join: fn(int, int) -> string }
fn mark(value) = {
    print("${value}")
    value
}
let x = 2
let answer = {
    handle static Pair {
        join x y => "${x}:${y}"
    }
    perform Pair.join(mark(1), mark(x))
}
print(answer)
"#,
        "1\n2\n1:2\n",
    );
}

#[test]
fn a_shadowed_helper_is_not_specialized_as_the_global_function() {
    assert_output(
        "helper_shadow",
        r#"
static effect Read { get: fn() -> int }
fn value() = perform Read.get()
let first = {
    handle static Read {
        get => 1
    }
    value()
}
let second = {
    handle static Read {
        get => 2
    }
    {
}
    let value = || => 3
    value()
}
print("${first}:${second}")
"#,
        "1:3\n",
    );
}

#[test]
fn a_specialized_helper_retains_the_handlers_lexical_capture() {
    assert_output(
        "helper_capture",
        r#"
static effect Read { get: fn() -> int }
fn value() = perform Read.get()
fn run(outer) = {
    handle static Read {
        get => outer
    }
    value()
}
print("${run(1)}:${run(2)}")
"#,
        "1:2\n",
    );
}

#[test]
fn captured_mutable_state_is_shared_with_specialized_helpers() {
    assert_output(
        "mutable_capture",
        r#"
static effect Counter { next: fn() -> int }
fn twice() = "${perform Counter.next()}:${perform Counter.next()}"
fn run(initial) = {
    mut count = initial
    let result = {
        handle static Counter {
            next => {
                count = (count + 1) ?: count
                count
            }
        }
        twice()
    }
    "${result}:${count}"
}
print("${run(0)}|${run(10)}")
"#,
        "1:2:2|11:12:12\n",
    );
    assert_flavored_output(
        "mutable_capture",
        "ospml",
        r#"
static effect Counter
    next : Unit => int
twice () = "${perform Counter.next ()}:${perform Counter.next ()}"
run initial =
    mut count = initial
    result =
        handle static Counter
            next =>
                count := (count + 1) ?: count
                count
        twice ()
    "${result}:${count}"
print "${run 0}|${run 10}"
"#,
        "1:2:2|11:12:12\n",
    );
}

#[test]
fn renamed_parameters_preserve_named_argument_calls() {
    assert_output(
        "named_args",
        r#"
static effect Read { get: fn() -> int }
fn add(first, second) = (first + second) ?: 0
let value = {
    handle static Read {
        get => 2
    }
    add(second: 3, first: perform Read.get())
}
print("${value}")
"#,
        "5\n",
    );
}

#[test]
fn pattern_and_lambda_binders_do_not_capture_handler_free_values() {
    assert_output(
        "nested_scopes",
        r#"
static effect Read { get: fn() -> int }
type Box = { outer: int }
let outer = 1
let value = {
    handle static Read {
        get => outer
    }
    {
}
    let f = |outer| => match Box { outer: outer } {
        Box { outer } => (perform Read.get() + outer) ?: 0
    }
    f(2)
}
print("${value}")
"#,
        "3\n",
    );
}

#[test]
fn global_captures_remain_visible_after_specialization() {
    assert_output(
        "global_capture",
        r#"
static effect Read { get: fn() -> int }
let amount = 2
fn work() = (perform Read.get() + amount) ?: 0
let value = {
    handle static Read {
        get => 1
    }
    work()
}
print("${value}")
"#,
        "3\n",
    );
}

#[test]
fn recursive_specializations_preserve_lexical_captures() {
    assert_output(
        "recursive_capture",
        r#"
static effect Read { get: fn() -> int }
fn readAfterSteps(n) = match n == 0 {
    true => perform Read.get()
    false => readAfterSteps((n - 1) ?: 0)
}
fn run(outer) = {
    handle static Read {
        get => outer
    }
    readAfterSteps(3)
}
print("${run(1)}:${run(2)}")
"#,
        "1:2\n",
    );
}
