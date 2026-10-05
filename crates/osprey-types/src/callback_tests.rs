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

fn forwarding(flavor: Flavor, source: &str, position: Position) {
    assert_eq!(
        warnings(flavor, source),
        [TypeWarning {
            message:
                "redundant callback wrapper: pass `work` directly; it already takes no arguments"
                    .into(),
            position: Some(position),
            rule: REDUNDANT_CALLBACK,
        }]
    );
}

#[test]
fn default_wrapper_names_the_function_and_points_to_the_lambda() {
    forwarding(
        Flavor::Default,
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        Position {
            line: 3,
            column: 19,
        },
    );
}

#[test]
fn ml_wrapper_has_the_same_rule_and_precise_position() {
    forwarding(
        Flavor::Ml,
        "work () = 41\nrelay callback = callback ()\nanswer = relay (\\() => work ())\n",
        Position {
            line: 3,
            column: 16,
        },
    );
}

#[test]
fn effectful_work_can_be_passed_directly_to_its_callable_handler() {
    let source = "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet provide = handler Read { value => 41 }\nlet answer = provide(fn() => work())\n";
    forwarding(
        Flavor::Default,
        source,
        Position {
            line: 4,
            column: 21,
        },
    );
}

#[test]
fn direct_callback_and_required_delayed_arguments_are_silent() {
    for source in [
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(work)\n",
        "fn work(value) = value\nfn relay(callback) = callback()\nlet answer = relay(fn() => work(41))\n",
        "fn work() = 41\nlet callback = fn() => { print(1)\nwork() }\nlet answer = callback()\n",
        "fn work() = 41\nlet callback = fn(value) => work()\nlet answer = callback(1)\n",
    ] {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}

#[test]
fn local_closures_and_shadowed_named_functions_are_not_recommended() {
    for source in [
        "let work = fn() => 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        "mut work = fn() => 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\n",
        "fn work() = 41\nfn relay(callback) = callback()\nfn wrap(work) = relay(fn() => work())\nlet answer = wrap(fn() => 42)\n",
        "fn work() = 41\nfn relay(callback) = callback()\nfn wrap() = { let work = fn() => 42\nrelay(fn() => work()) }\nlet answer = wrap()\n",
    ] {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}

#[test]
fn lambda_return_contracts_and_generic_targets_are_kept() {
    for source in [
        "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() -> int => work())\n",
        "fn work<T>() -> List<T> = []\nfn relay(callback) = callback()\nlet answer = relay(fn() => work<int>())\n",
    ] {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}

#[test]
fn necessary_handler_scope_inside_a_callback_is_preserved() {
    let source = "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet callback = fn() => { handle Read { value => 41 }\nwork() }\nlet answer = callback()\n";
    assert!(warnings(Flavor::Default, source).is_empty());
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
    assert!(parsed.errors.is_empty());
    assert!(!crate::check_program(&parsed.program).is_empty());
    assert!(redundant_callbacks(&parsed.program).is_empty());
}

#[test]
fn method_resolution_after_a_removed_wrapper_remains_identical() {
    let source = "fn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\nlet size = \"hello\".length()\n";
    assert_eq!(warnings(Flavor::Default, source).len(), 1);
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

#[test]
fn ml_effect_handler_keeps_delayed_execution_while_removing_only_forwarding() {
    let source = "effect Read\n    value : Unit => int\nwork () = perform Read.value ()\nprovide = handler Read\n    value => 41\nanswer = provide (\\() => work ())\n";
    forwarding(
        Flavor::Ml,
        source,
        Position {
            line: 6,
            column: 18,
        },
    );
}

#[test]
fn inferred_generic_functions_are_not_monomorphized_by_advice() {
    let source =
        "fn work() = []\nfn relay(callback) = callback()\nlet answer: List<int> = relay(fn() => work())\n";
    assert!(warnings(Flavor::Default, source).is_empty());
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

#[test]
fn function_identity_comparisons_keep_the_wrapper() {
    for source in [
        "fn work() = 41\nlet callback = fn() => work()\nlet answer = callback == work\n",
        "fn work() = 41\nfn same(a, b) = a == b\nfn relay(callback) = same(callback, work)\nlet answer = relay(fn() => work())\n",
        "fn work() = 41\nlet callback = fn() => work()\nlet answer = listContains([work], callback)\n",
    ] {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}

#[test]
fn function_display_and_foreign_observation_keep_the_wrapper() {
    for source in [
        "fn work() = 41\nlet callback = fn() => work()\nprint(\"${callback} ${work}\")\n",
        "extern fn observe(callback: fn() -> int) -> int\nfn work() = 41\nlet answer = observe(fn() => work())\n",
    ] {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}

#[test]
fn unrelated_comparisons_interpolation_and_ffi_do_not_hide_direct_invoker_advice() {
    let source = "extern fn readClock() -> int\nfn work() = 41\nfn relay(callback) = callback()\nlet answer = relay(fn() => work())\nlet correct = answer == 41\nprint(\"Answer: ${answer}\")\n";
    assert_eq!(warnings(Flavor::Default, source).len(), 1);
}

#[test]
fn handler_callbacks_stay_useful_in_programs_that_display_and_compare_results() {
    let source = "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet provide = handler Read { value => 41 }\nlet answer = provide(fn() => work())\nlet correct = answer == 41\nprint(\"Answer: ${answer}\")\n";
    assert_eq!(warnings(Flavor::Default, source).len(), 1);
}

#[test]
fn inline_handler_literals_are_proved_invokers() {
    let source = "effect Read { value: fn() -> int }\nfn work() !Read = perform Read.value()\nlet answer = (handler Read { value => 41 })(fn() => work())\n";
    assert_eq!(warnings(Flavor::Default, source).len(), 1);
}

#[test]
fn stored_callbacks_escaping_consumers_and_shadowed_invokers_are_preserved() {
    for source in [
        "fn work() = 41\nlet callback = fn() => work()\nlet answer = callback()\n",
        "fn work() = 41\nfn store(callback) = callback\nlet answer = store(fn() => work())()\n",
        "fn work() = 41\nfn relay(callback) = callback()\nfn wrap(relay) = relay(fn() => work())\nlet answer = wrap(fn(callback) => callback())\n",
        "effect Read { value: fn() -> int }\nfn work() = 41\nmut provide = handler Read { value => 41 }\nlet answer = provide(fn() => work())\n",
    ] {
        assert!(warnings(Flavor::Default, source).is_empty(), "{source}");
    }
}
