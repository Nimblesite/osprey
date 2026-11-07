//! Semantics and source positions for [TYPE-WARNINGS-CALLBACKS].

use osprey_ast::Position;
use osprey_syntax::{parse_program_with_flavor, Flavor};

use crate::{redundant_callbacks, TypeWarning, REDUNDANT_CALLBACK};

fn warnings(flavor: Flavor, source: &str) -> Vec<TypeWarning> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    let errors = crate::check_program(&parsed.program);
    assert!(errors.is_empty(), "{source}\n{errors:?}");
    redundant_callbacks(&parsed.program)
}

/// One forwarding-wrapper warning, raised at exactly this position.
fn forwarding(flavor: Flavor, source: &str, line: u32, column: u32) {
    assert_eq!(
        warnings(flavor, source),
        [TypeWarning {
            message:
                "redundant callback wrapper: pass `work` directly; it already takes no arguments"
                    .into(),
            position: Some(Position { line, column }),
            rule: REDUNDANT_CALLBACK,
        }]
    );
}

/// Each listed Default-flavor program must raise no redundant-callback warning.
fn silent(sources: &[&str]) {
    for &source in sources {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}

/// Each listed Default-flavor program must raise exactly one warning.
fn raises_once(sources: &[&str]) {
    for &source in sources {
        assert_eq!(warnings(Flavor::Default, source).len(), 1, "{source}");
    }
}

#[test]
fn default_wrapper_names_the_function_and_points_to_the_lambda() {
    forwarding(
        Flavor::Default,
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        3,
        19,
    );
}

#[test]
fn ml_wrapper_has_the_same_rule_and_precise_position() {
    forwarding(
        Flavor::Ml,
        "work () = 41\nrelay callback = callback ()\nanswer = relay (\\() => work ())\n",
        3,
        16,
    );
}

#[test]
fn effectful_work_can_be_passed_directly_to_its_callable_handler() {
    forwarding(
        Flavor::Default,
        "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet provide = handler Read { value => 41 }\nlet answer = provide(fn() => work())\n",
        4,
        21,
    );
}

#[test]
fn wrappers_are_flagged_only_when_removal_is_safe() {
    // Direct passing and delayed arguments need no wrapper at all.
    silent(&[
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(work)\n",
        "fn work(value) = value\nfn relay(callback) = callback()\nlet answer = relay(fn() => work(41))\n",
        "fn work() = 41\nlet callback = fn() => { print(1)\nwork() }\nlet answer = callback()\n",
        "fn work() = 41\nlet callback = fn(value) => work()\nlet answer = callback(1)\n",
        // Local bindings, mutables, and shadowing never resolve to the named function.
        "let work = fn() => 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        "mut work = fn() => 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        "fn work() = 41\nfn relay(callback) = callback()\nfn wrap(work) = relay(fn() => work())\nlet answer = wrap(fn() => 42)\n",
        "fn work() = 41\nfn relay(callback) = callback()\nfn wrap() = { let work = fn() => 42\nrelay(fn() => work()) }\nlet answer = wrap()\n",
        // Return contracts and generic instantiation survive the rewrite.
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() -> int => work())\n",
        "fn work<T>() -> List<T> = []\nfn relay(callback) = callback()\nlet answer = relay(fn() => work<int>())\n",
        "fn work() = []\nfn relay(callback) = callback()\nlet answer: List<int> = relay(fn() => work())\n",
        // Handler scope, stored callbacks, and foreign observation keep the wrapper.
        "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet callback = fn() => { handle Read { value => 41 }\nwork() }\nlet answer = callback()\n",
        "fn work() = 41\nlet callback = fn() => work()\nlet answer = callback()\n",
        "fn work() = 41\nfn store(callback) = callback\nlet answer = store(fn() => work())()\n",
        "fn work() = 41\nfn relay(callback) = callback()\nfn wrap(relay) = relay(fn() => work())\nlet answer = wrap(fn(callback) => callback())\n",
        "effect Read { value: fn() -> int }\nfn work() = 41\nmut provide = handler Read { value => 41 }\nlet answer = provide(fn() => work())\n",
        "extern fn observe(callback: fn() -> int) -> int\nfn work() = 41\nlet answer = observe(fn() => work())\n",
    ]);
}

#[test]
fn multiple_removable_wrappers_are_proved_as_one_set() {
    let source = "fn work() = 41\nfn relay(callback) = callback()\nlet one = relay(fn() => work())\nlet two = relay(fn() => work())\n";
    let raised = warnings(Flavor::Default, source);
    assert_eq!(raised.len(), 2, "{raised:?}");
    assert!(raised
        .iter()
        .all(|warning| warning.rule == "redundant-callback"));
}

#[test]
fn invalid_programs_do_not_receive_speculative_simplifications() {
    let parsed = parse_program_with_flavor(
        "fn work() = missing\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        Flavor::Default,
    );
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let errors = crate::check_program(&parsed.program);
    assert!(!errors.is_empty(), "`missing` must be a type error");
    let raised = redundant_callbacks(&parsed.program);
    assert!(raised.is_empty(), "{raised:?}");
}

#[test]
fn removal_advice_survives_method_calls_comparisons_display_and_inline_handlers() {
    raises_once(&[
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\nlet size = \"hello\".length()\n",
        "extern fn readClock() -> int\nfn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\nlet correct = answer == 41\nprint(\"Answer: ${answer}\")\n",
        "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet provide = handler Read { value => 41 }\nlet answer = provide(fn() => work())\nlet correct = answer == 41\nprint(\"Answer: ${answer}\")\n",
        "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet answer = (handler Read { value => 41 })(fn() => work())\n",
    ]);
}

#[test]
fn assembled_function_names_are_reported_in_source_form() {
    let symbol = osprey_ast::symbol::mangle(["bank", "work"]);
    let source =
        format!("fn {symbol}() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => {symbol}())\n");
    let raised = warnings(Flavor::Default, &source);
    assert_eq!(raised.len(), 1);
    assert_eq!(
        raised.first().map(|warning| warning.message.as_str()),
        Some(
            "redundant callback wrapper: pass `bank::work` directly; it already takes no arguments"
        )
    );
}

/// A function's equality is not decided ([BOOL-EQUALITY]), so no comparison
/// can observe whether a wrapper was removed: each of these is a compile error
/// rather than a reason to keep the wrapper.
#[test]
fn comparing_functions_is_rejected_so_no_comparison_observes_a_wrapper() {
    for source in [
        "fn work() = 41\nlet callback = fn() => work()\nlet answer = callback == work\n",
        "fn work() = 41\nfn same(a, b) = a == b\nfn relay(callback) = same(callback, work)\nlet answer = relay(fn() => work())\n",
        "fn work() = 41\nlet callback = fn() => work()\nlet answer = listContains([work], callback)\n",
    ] {
        let parsed = parse_program_with_flavor(source, Flavor::Default);
        assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
        let errors = crate::check_program(&parsed.program);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("equality is decided only for")),
            "{source}\n{errors:?}"
        );
    }
}

#[test]
fn ml_effect_handler_keeps_delayed_execution_while_removing_only_forwarding() {
    forwarding(
        Flavor::Ml,
        "effect Read\n    value : Unit => int\nwork () = perform Read.value ()\nprovide = handler Read\n    value => 41\nanswer = provide (\\() => work ())\n",
        6,
        18,
    );
}

#[test]
fn adapting_the_result_type_does_not_recommend_changing_a_named_function() {
    let source = "fn work() = 41\nfn use(action: fn() -> Result<int, string>) = action()\nlet answer = use(fn() => work())\n";
    let program = parse_program_with_flavor(source, Flavor::Default).program;
    assert!(
        warnings(Flavor::Default, source).is_empty(),
        "{:?}",
        crate::infer_program(&program)
    );
}
