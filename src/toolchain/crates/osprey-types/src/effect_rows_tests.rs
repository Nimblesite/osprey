//! Focused regression matrix for static algebraic-effect discharge.
//!
//! These tests deliberately exercise inferred rows: return types and effect
//! annotations are omitted unless the annotation itself is the subject of the
//! test. A handler is lexical authority, not permission that may escape in a
//! closure.

use crate::testutil::{accepts, rejects_with, typecheck};
use crate::{check_program, TypeError};
use osprey_ast::Stmt;
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub(crate) fn diagnostics(source: &str) -> Vec<TypeError> {
    typecheck(Flavor::Default, source)
}

#[track_caller]
pub(crate) fn assert_accepted(source: &str) {
    let errors = diagnostics(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:#?}");
}

#[track_caller]
pub(crate) fn assert_rejected_with(source: &str, expected: &[&str]) {
    let errors = diagnostics(source);
    assert!(!errors.is_empty(), "expected static rejection, got none");
    let rendered = errors
        .iter()
        .map(|error| error.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for fragment in expected {
        assert!(
            rendered.contains(fragment),
            "expected diagnostic containing {fragment:?}, got:\n{rendered}"
        );
    }
}

/// `source`, parsed under `flavor`, is rejected for performing `print`
/// outside its declared row.
#[track_caller]
fn assert_print_outside_its_row(source: &str, flavor: Flavor) {
    let errors = typecheck(flavor, source);
    assert!(
        errors.iter().any(|error| {
            error.message.contains("outside its declared row") && error.message.contains("print")
        }),
        "{flavor:?}: {errors:?}"
    );
}

/// The fixture most of this matrix is written against: one effect, one
/// `int`-returning operation.
pub(crate) const ALARM: &str = "effect Alarm { ring: fn() -> int }\n";

/// Variant whose operation takes the iterated value.
pub(crate) const ALARM_ARG: &str = "effect Alarm { ring: fn(int) -> int }\n";

/// Two-operation fixture for partial-handler coverage.
pub(crate) const PAIR: &str = "effect Pair { first: fn() -> int second: fn() -> int }\n";

/// Generic fixture whose handler must match the performed instantiation.
pub(crate) const STASH: &str = "effect Stash<T> { put: fn(T) -> Unit take: fn() -> T }\n";

#[track_caller]
pub(crate) fn accept_alarm(body: &str) {
    assert_accepted(&format!("{ALARM}{body}"));
}

#[track_caller]
pub(crate) fn reject_alarm(body: &str, expected: &[&str]) {
    assert_rejected_with(&format!("{ALARM}{body}"), expected);
}

/// `let {binding} = { handle {effect} {arms} {body} }` — the handle block the
/// discharge tests wrap their body in.
pub(crate) fn handled(binding: &str, effect: &str, arms: &str, body: &str) -> String {
    format!("let {binding} = {{\nhandle {effect} {{\n{arms}\n}}\n{body}\n}}\n")
}

/// `match {lookup} { Success { value } => value, Error => fn() => 0 }` — how
/// the collection-escape tests project a callable element out of a `Result`.
pub(crate) fn unwrap_success(lookup: &str) -> String {
    format!("match {lookup} {{\nSuccess {{ value }} => value\nError {{ message }} => fn() => 0\n}}")
}

#[test]
fn duplicate_handler_arms_are_rejected_in_both_flavors() {
    for (source, flavor) in [
        (
            "effect Read { value: fn() -> int }\nlet h = handler Read { value => 1 value => 2 }\nlet n = h(|| => perform Read.value())\n",
            Flavor::Default,
        ),
        (
            "effect Read\n    value : Unit => int\nh = handler Read\n    value => 1\n    value => 2\nn = h (\\() => perform Read.value ())\n",
            Flavor::Ml,
        ),
    ] {
        let errors = typecheck(flavor, source);
        assert!(
            errors.iter().any(|error| error.message.contains("duplicate handler arm `Read.value`")),
            "{flavor:?}: expected a duplicate-arm error, got {errors:?}"
        );
    }
}

#[test]
fn explicitly_empty_rows_reject_effects_but_omitted_rows_still_infer_them() {
    for (source, flavor) in [
        (
            "effect E { ping: fn() -> Unit }\nfn f() -> Unit ![] = perform E.ping()\nlet h = handler E { ping => print(\"called\") }\nlet result = h(f)\n",
            Flavor::Default,
        ),
        (
            "effect E\n    ping : Unit => Unit\nf : Unit -> Unit ![]\nf () = perform E.ping ()\nh = handler E\n    ping => print \"called\"\n_ = h f\n",
            Flavor::Ml,
        ),
    ] {
        let errors = typecheck(flavor, source);
        assert!(
            errors.iter().any(|error| error.message.contains("performs effects outside its declared row: E.ping")),
            "{flavor:?}: {errors:?}"
        );

        let inferred = typecheck(flavor, &source.replace(" ![]", ""));
        assert!(inferred.is_empty(), "{flavor:?}: omitted row should infer: {inferred:?}");
    }
    for (source, flavor) in [
        (
            "fn f() -> int ![] = 42\nlet answer = f()\n",
            Flavor::Default,
        ),
        (
            "f : Unit -> int ![]\nf () = 42\nanswer = f ()\n",
            Flavor::Ml,
        ),
    ] {
        let errors = typecheck(flavor, source);
        assert!(errors.is_empty(), "{flavor:?}: pure empty row: {errors:?}");
    }
}

#[test]
fn explicit_empty_rows_cannot_hide_runtime_builtin_work() {
    for (source, flavor) in [
        (
            "fn report() -> Unit ![] = print(\"sent\")\n",
            Flavor::Default,
        ),
        (
            "fn report() -> Unit ![] = {\n    let writer = print\n    writer(\"sent\")\n}\n",
            Flavor::Default,
        ),
        (
            "fn loud() -> Unit = print(\"sent\")\nfn report() -> Unit ![] = loud()\n",
            Flavor::Default,
        ),
        (
            "report : Unit -> Unit ![]\nreport () = print \"sent\"\n",
            Flavor::Ml,
        ),
        (
            "loud () = print \"sent\"\nreport : Unit -> Unit ![]\nreport () = loud ()\n",
            Flavor::Ml,
        ),
    ] {
        assert_print_outside_its_row(source, flavor);
    }
}

#[test]
fn explicit_empty_rows_reject_unproven_callback_effects() {
    for (source, flavor) in [
        (
            "fn run(callback: fn() -> Unit) -> Unit ![] = callback()\n",
            Flavor::Default,
        ),
        (
            "run : (Unit -> Unit) -> Unit ![]\nrun callback = callback ()\n",
            Flavor::Ml,
        ),
    ] {
        rejects_with(flavor, source, "unproven effects outside its declared row");
    }
}

#[test]
fn open_rows_transport_callback_effects_to_the_handler_in_both_flavors() {
    for (source, flavor) in [
        (
            "effect Log { write: fn(string) -> Unit }\nfn invoke(callback: fn() -> Unit) -> Unit !e = callback()\nfn report() -> Unit !Log = perform Log.write(\"sent\")\nlet h = handler Log { write message => print(message) }\nh(|| => invoke(report))\n",
            Flavor::Default,
        ),
        (
            "effect Log\n    write : string => Unit\ninvoke : (Unit -> Unit) -> Unit !e\ninvoke callback = callback ()\nreport : Unit -> Unit !Log\nreport () = perform Log.write \"sent\"\nh = handler Log\n    write message => print message\n_ = h (\\() => invoke report)\n",
            Flavor::Ml,
        ),
    ] {
        let parsed = parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{flavor:?}: {:?}", parsed.errors);
        let invoke = parsed
            .program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Stmt::Function {
                    name, effect_tail, ..
                } if name == "invoke" => effect_tail.as_deref(),
                _ => None,
            });
        assert_eq!(invoke, Some("e"), "{flavor:?}");
        let errors = check_program(&parsed.program);
        assert!(errors.is_empty(), "{flavor:?}: {errors:?}");
    }
}

#[test]
fn open_tail_does_not_hide_a_direct_operation_outside_its_fixed_labels() {
    for (accepted, rejected, flavor) in [
        (
            "effect Log { write: fn(string) -> Unit }\nfn combine(callback: fn() -> Unit) -> Unit ![Log | e] = {\n    perform Log.write(\"fixed\")\n    callback()\n}\n",
            "effect Log { write: fn(string) -> Unit }\nfn wrong() -> Unit !e = perform Log.write(\"missing\")\n",
            Flavor::Default,
        ),
        (
            "effect Log\n    write : string => Unit\ncombine : (Unit -> Unit) -> Unit ![Log | e]\ncombine callback =\n    perform Log.write \"fixed\"\n    callback ()\n",
            "effect Log\n    write : string => Unit\nwrong : Unit -> Unit !e\nwrong () = perform Log.write \"missing\"\n",
            Flavor::Ml,
        ),
    ] {
        accepts(flavor, accepted);

        rejects_with(flavor, rejected, "outside its declared row: Log.write");
    }
}

#[test]
fn open_tail_cannot_hide_a_direct_host_operation() {
    for (source, flavor) in [
        ("fn wrong() -> Unit !e = print(\"sent\")\n", Flavor::Default),
        (
            "wrong : Unit -> Unit !e\nwrong () = print \"sent\"\n",
            Flavor::Ml,
        ),
    ] {
        assert_print_outside_its_row(source, flavor);
    }
}

#[test]
fn a_closed_caller_cannot_erase_an_open_callback_requirement() {
    for (source, flavor) in [
        (
            "fn relay(callback) -> Unit !e = callback()\nfn closed(callback: fn() -> Unit) -> Unit ![] = relay(callback)\n",
            Flavor::Default,
        ),
        (
            "relay : (Unit -> Unit) -> Unit !e\nrelay callback = callback ()\nclosed : (Unit -> Unit) -> Unit ![]\nclosed callback = relay callback\n",
            Flavor::Ml,
        ),
    ] {
        rejects_with(flavor, source, "function `closed` calls a function with unproven effects");
    }
}

#[test]
fn nominal_record_row_entries_match_inferred_record_requests() {
    for (source, flavor) in [
        (
            "type Point = { x: int, y: int }\neffect Echo<T> { echo: fn(T) -> T }\nfn fetch() -> Point !Echo<Point> = perform Echo.echo(Point { x: 42, y: 1 })\nlet answer = {\n    handle Echo<Point> { echo value => value }\n    fetch()\n}\nprint(answer.x)\n",
            Flavor::Default,
        ),
        (
            "type Point =\n    x : int\n    y : int\neffect Echo T\n    echo : T => T\nfetch : Unit -> Point ! Echo<Point>\nfetch () = perform Echo.echo (Point(x = 42, y = 1))\nanswer =\n    handle Echo<Point>\n        echo value => value\n    fetch ()\nprint answer.x\n",
            Flavor::Ml,
        ),
    ] {
        accepts(flavor, source);
    }
}

#[test]
fn callable_handler_preserves_a_returned_functions_effect_provenance() {
    assert_accepted(
        "effect Read { read: fn() -> fn(int) -> int }\n\
         let h = handler Read { read => |n| => wrapAdd(n, 1) }\n\
         let action = h(|| => perform Read.read())\n\
         let answer = action(41)\n",
    );
}

#[test]
fn callable_handler_does_not_discharge_a_closure_invoked_after_it_returns() {
    assert_rejected_with(
        "effect Read { read: fn() -> int }\n\
         let h = handler Read { read => 41 }\n\
         let escaped = h(|| => || => perform Read.read())\n\
         let answer = escaped()\n",
        &["unhandled effect operations at program entry", "Read.read"],
    );
}

#[test]
fn callable_handler_does_not_supply_a_different_generic_instance() {
    assert_rejected_with(
        "effect Read<T> { read: fn() -> T }\n\
         let h = handler Read<int> { read => 41 }\n\
         let action = h(|| => perform Read<string>.read())\n\
         let answer = action\n",
        &["Read<string>.read"],
    );
}

#[test]
fn recursive_closure_provenance_reaches_a_bounded_fixed_point() {
    const CHILD: &str = "OSPREY_EFFECT_ROWS_RECURSIVE_CLOSURE_CHILD";
    const TEST: &str =
        "effect_rows_tests::recursive_closure_provenance_reaches_a_bounded_fixed_point";

    if std::env::var_os(CHILD).is_some() {
        let errors = diagnostics(
            "effect Alarm { ring: fn() -> int }\n\
             fn grow() = fn() => grow()\n\
             let answer = 42\n",
        );
        assert!(
            !errors.is_empty(),
            "the recursive/infinite value type should still be diagnosed"
        );
        return;
    }

    let executable = std::env::current_exe().expect("current test executable");
    let mut child = Command::new(executable)
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, "1")
        .spawn()
        .expect("spawn recursive-closure compiler check");
    // Keep the regression bounded without making loaded debug/CI runners race
    // a tight wall-clock threshold. The converged child normally takes < 20ms.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().expect("poll child test") {
            assert!(status.success(), "child compiler check failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill non-terminating compiler check");
            let _ = child.wait();
            panic!("effect analysis did not reach a fixed point within 10 seconds");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

const BRANCHING_CALLBACK_TREE: &str = "effect Alarm { ring: fn() -> int }\n\
    type Tree = Leaf(() -> int) | Node(Tree, Tree)\n\
    fn grow(n) = match n {\n\
      0 => Leaf(fn() => perform Alarm.ring())\n\
      _ => Node(grow(wrapSub(n, 1)), grow(wrapSub(n, 1)))\n\
    }\n\
    let root = grow(8)\n";

#[test]
fn branching_recursive_callback_retains_its_effect() {
    assert_rejected_with(
        &format!(
            "{BRANCHING_CALLBACK_TREE}let answer = match root {{ Leaf(callback) => callback() Node(_, _) => 0 }}\n"
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn branching_recursive_callback_accepts_a_matching_handler() {
    assert_accepted(&format!(
        "{BRANCHING_CALLBACK_TREE}let h = handler Alarm {{ ring => 42 }}\n\
         let answer = h(|| => match root {{ Leaf(callback) => callback() Node(_, _) => 0 }})\n"
    ));
}

/// A recursive curried function is the shape an ML definition of arity > 1
/// lowers to. Its first fixed-point iteration cannot resolve the self-call —
/// its own return provenance does not exist yet — and that transient verdict
/// used to be stored in the returned closure's summary and re-derived on every
/// later iteration, condemning the function forever.
#[test]
fn a_pure_recursive_curried_function_is_accepted() {
    accept_alarm(
        "fn countDown(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => countDown(wrapSub(n, 1))(wrapAdd(acc, n))\n\
         }\n\
         let total = countDown(3)(0)\n",
    );
}

#[test]
fn mutually_recursive_curried_functions_are_accepted() {
    accept_alarm(
        "fn evens(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => odds(wrapSub(n, 1))(wrapAdd(acc, n))\n\
         }\n\
         fn odds(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => evens(wrapSub(n, 1))(wrapAdd(acc, n))\n\
         }\n\
         let total = evens(4)(0)\n",
    );
}

#[test]
fn a_recursive_curried_function_still_carries_its_effect_to_the_call_site() {
    reject_alarm(
        "fn countDown(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => countDown(wrapSub(n, 1))(wrapAdd(acc, perform Alarm.ring()))\n\
         }\n\
         let total = countDown(3)(0)\n",
        &["unhandled effect operations at program entry", "Alarm.ring"],
    );
}

#[test]
fn a_handler_discharges_a_recursive_curried_function() {
    accept_alarm(&format!(
        "fn countDown(n) = fn(acc) => match n <= 0 {{\n\
         true => acc\n\
         false => countDown(wrapSub(n, 1))(wrapAdd(acc, perform Alarm.ring()))\n\
         }}\n\
         {}",
        handled("total", "Alarm", "ring => 1", "countDown(3)(0)")
    ));
}

/// A curried binding's declared row bounds its full application. `pair a b`
/// lowers to a function whose body is a closure literal, so what that closure
/// performs is what the signature's row governs ([FLAVOR-ML-CURRY]); checking
/// only the closure's construction accepted every curried row unconditionally.
#[test]
fn a_curried_declared_row_bounds_the_full_application() {
    let effects = "effect E\n    op : int => int\neffect F\n    go : int => int\n";
    for (signature, binding, undeclared) in [
        (
            "int -> int -> int",
            "pair a b = perform F.go (satAdd a b)",
            "F.go",
        ),
        ("int -> int -> int", "pair a b = a + b", "Arith.overflow"),
        (
            "int -> int -> int -> int",
            "pair a b c = perform F.go c",
            "F.go",
        ),
    ] {
        rejects_with(
            Flavor::Ml,
            format!("{effects}pair : {signature} ! E\n{binding}\n"),
            format!("function `pair` performs effects outside its declared row: {undeclared}"),
        );
    }
    accepts(
        Flavor::Ml,
        format!("{effects}pair : int -> int -> int ! F\npair a b = perform F.go (satAdd a b)\n"),
    );
    rejects_with(
        Flavor::Default,
        "effect F { go: fn(int) -> int }\nfn pair(a) ![] = fn(b) => perform F.go(satAdd(a, b))\n",
        "function `pair` performs effects outside its declared row: F.go",
    );
}

mod curried_callbacks;
mod gpu;
