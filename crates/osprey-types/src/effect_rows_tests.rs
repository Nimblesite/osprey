//! Focused regression matrix for static algebraic-effect discharge.
//!
//! These tests deliberately exercise inferred rows: return types and effect
//! annotations are omitted unless the annotation itself is the subject of the
//! test. A handler is lexical authority, not permission that may escape in a
//! closure.

use crate::{check_program, TypeError};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub(crate) fn diagnostics(source: &str) -> Vec<TypeError> {
    let parsed = parse_program_with_flavor(source, Flavor::Default);
    assert!(
        parsed.errors.is_empty(),
        "test fixture has syntax errors: {:?}",
        parsed.errors
    );
    check_program(&parsed.program)
}

pub(crate) fn assert_accepted(source: &str) {
    let errors = diagnostics(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:#?}");
}

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

/// A recursive curried function is the shape an ML definition of arity > 1
/// lowers to. Its first fixed-point iteration cannot resolve the self-call —
/// its own return provenance does not exist yet — and that transient verdict
/// used to be stored in the returned closure's summary and re-derived on every
/// later iteration, condemning the function forever.
#[test]
fn a_pure_recursive_curried_function_is_accepted() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn countDown(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => countDown((n - 1) ?: 0)((acc + n) ?: acc)\n\
         }\n\
         let total = countDown(3)(0)\n",
    );
}

#[test]
fn mutually_recursive_curried_functions_are_accepted() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn evens(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => odds((n - 1) ?: 0)((acc + n) ?: acc)\n\
         }\n\
         fn odds(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => evens((n - 1) ?: 0)((acc + n) ?: acc)\n\
         }\n\
         let total = evens(4)(0)\n",
    );
}

#[test]
fn a_recursive_curried_function_still_carries_its_effect_to_the_call_site() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn countDown(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => countDown((n - 1) ?: 0)((acc + perform Alarm.ring()) ?: acc)\n\
         }\n\
         let total = countDown(3)(0)\n",
        &["unhandled effect operations at program entry", "Alarm.ring"],
    );
}

#[test]
fn a_handler_discharges_a_recursive_curried_function() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn countDown(n) = fn(acc) => match n <= 0 {\n\
           true => acc\n\
           false => countDown((n - 1) ?: 0)((acc + perform Alarm.ring()) ?: acc)\n\
         }\n\
         let total = handle Alarm\n\
           ring => 1\n\
         in countDown(3)(0)\n",
    );
}

/// The guard on the fix above: provenance verdicts are re-derived after the
/// rows converge, NOT merely erased. A call this pass cannot resolve — here a
/// top-level `let` alias the function environment never sees — must still be
/// reported when it hides inside a returned closure.
#[test]
fn an_unresolvable_call_inside_a_returned_closure_is_still_rejected() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let siren = ring\n\
         fn make() = fn() => siren()\n\
         let answer = make()()\n",
        &["effect provenance cannot be proven"],
    );
}

#[test]
fn colliding_interpolation_perform_positions_keep_every_generic_instance() {
    assert_rejected_with(
        "effect Echo<T> { echo: fn(T) -> T }\n\
         let number = \"${perform Echo.echo(42)}\"\n\
         let text = \"${perform Echo.echo(\"hello\")}\"\n",
        &["Echo<int>.echo", "Echo<string>.echo"],
    );
}

#[test]
fn direct_unhandled_perform_is_rejected_at_entry() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let answer = perform Alarm.ring()\n",
        &["unhandled effect operations at program entry", "Alarm.ring"],
    );
}

#[test]
fn inferred_helper_and_transitive_call_chain_propagate_effects() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn relay() = ring()\n\
         fn relayAgain() = relay()\n\
         let answer = relayAgain()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn an_unused_effectful_function_is_latent_and_accepted() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn dormant() = perform Alarm.ring()\n\
         let answer = 42\n",
    );
}

#[test]
fn implicit_main_body_must_have_an_empty_effect_row() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> Unit }\n\
         fn main() = perform Alarm.ring()\n",
        &["unhandled effect operations at program entry", "Alarm.ring"],
    );
}

#[test]
fn complete_handler_discharges_inferred_transitive_effect() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn relay() = ring()\n\
         let answer = handle Alarm\n\
           ring => 42\n\
         in relay()\n",
    );
}

#[test]
fn user_higher_order_function_propagates_callback_effect() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn apply(callback) = callback()\n\
         fn forward(callback) = apply(callback)\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = forward(ring)\n",
        &["Alarm.ring"],
    );
}

#[test]
fn user_higher_order_callback_is_discharged_at_call_site() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn apply(callback) = callback()\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = handle Alarm\n\
           ring => 42\n\
         in apply(ring)\n",
    );
}

#[test]
fn higher_order_function_may_handle_its_callback_internally() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn guarded(callback) = handle Alarm\n\
           ring => 42\n\
         in callback()\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = guarded(ring)\n",
    );
}

#[test]
fn constructing_an_effectful_lambda_is_pure_until_invoked() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         let delayed = fn() => perform Alarm.ring()\n\
         let answer = 42\n",
    );
}

#[test]
fn invoking_an_effectful_lambda_outside_a_handler_is_rejected() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let delayed = fn() => perform Alarm.ring()\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn lambda_constructed_under_handler_cannot_escape_its_authority() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let delayed = handle Alarm\n\
           ring => 41\n\
         in fn() => perform Alarm.ring()\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn invoking_an_effectful_lambda_inside_handler_is_accepted() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         let delayed = fn() => perform Alarm.ring()\n\
         let answer = handle Alarm\n\
           ring => 42\n\
         in delayed()\n",
    );
}

#[test]
fn lambda_returned_by_higher_order_function_keeps_captured_callback_effects() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn defer(callback) = fn() => callback()\n\
         fn ring() = perform Alarm.ring()\n\
         let delayed = defer(ring)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn nested_returned_lambdas_keep_captured_callback_effects() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn deferTwice(callback) = fn() => fn() => callback()\n\
         let first = deferTwice(ring)\n\
         let second = first()\n\
         let answer = second()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn named_callback_argument_keeps_its_effect_when_returned() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn identity(callback) = callback\n\
         let delayed = identity(callback: ring)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn local_identity_lambda_keeps_a_returned_callback_effect() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let identity = fn(callback) => callback\n\
         let delayed = identity(ring)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn closure_returned_through_match_keeps_every_branch_effect() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn choose(flag) = match flag {\n\
           true => fn() => perform Alarm.ring()\n\
           false => fn() => 0\n\
           }\n\
         let delayed = choose(false)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn closure_returned_through_handler_block_cannot_escape_authority() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let delayed = handle Alarm\n\
           ring => 41\n\
         in {\n\
           let nested = fn() => perform Alarm.ring()\n\
           nested\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn callable_returned_by_a_handler_arm_keeps_its_latent_effect() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         effect Factory { make: fn() -> () -> int }\n\
         let delayed = handle Factory\n\
           make => fn() => perform Alarm.ring()\n\
         in perform Factory.make()\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn handler_arm_invocation_keeps_effects_of_operation_callback_arguments() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         effect Gate { apply: fn(() -> int) -> int }\n\
         let answer = handle Gate\n\
           apply callback => callback()\n\
         in perform Gate.apply(fn() => perform Alarm.ring())\n",
        &["Alarm.ring"],
    );
}

#[test]
fn operation_callback_argument_effect_can_be_discharged_by_an_outer_handler() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         effect Gate { apply: fn(() -> int) -> int }\n\
         let answer = handle Alarm\n\
           ring => 42\n\
         in handle Gate\n\
           apply callback => callback()\n\
         in perform Gate.apply(fn() => perform Alarm.ring())\n",
    );
}

#[test]
fn lazy_iterator_callback_cannot_escape_its_construction_handler() {
    assert_rejected_with(
        "effect Alarm { ring: fn(int) -> int }\n\
         let mapped = handle Alarm\n\
           ring value => value\n\
         in map(range(0, 1), fn(value) => perform Alarm.ring(value))\n\
         forEach(mapped, fn(value) => print(toString(value)))\n",
        &["Alarm.ring"],
    );
}

#[test]
fn lazy_iterator_callback_is_discharged_when_consumed_inside_handler() {
    assert_accepted(
        "effect Alarm { ring: fn(int) -> int }\n\
         handle Alarm\n\
           ring value => value\n\
         in forEach(\n\
           map(range(0, 1), fn(value) => perform Alarm.ring(value)),\n\
           fn(value) => print(toString(value))\n\
         )\n",
    );
}

#[test]
fn effectful_closure_cannot_escape_through_a_channel_alias() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let channel = Channel(1)\n\
         let alias = channel\n\
         handle Alarm\n\
           ring => 41\n\
         in send(alias, fn() => perform Alarm.ring())\n\
         let delayed = recv(channel)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn pure_dynamic_closure_survives_a_channel_alias() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         let channel = Channel(1)\n\
         let alias = channel\n\
         send(alias, fn() => 42)\n\
         let delayed = recv(channel)\n\
         let answer = delayed()\n",
    );
}

#[test]
fn pure_closure_round_tripped_through_a_channel_alias_remains_callable() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         let channel = Channel(1)\n\
         let alias = channel\n\
         send(alias, fn() => 42)\n\
         let delayed = recv(channel)\n\
         let answer = delayed()\n",
    );
}

#[test]
fn effectful_closure_cannot_escape_through_spawn_and_await() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let fiber = handle Alarm\n\
           ring => 41\n\
         in spawn (fn() => perform Alarm.ring())\n\
         let delayed = await(fiber)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn pure_dynamic_closure_survives_spawn_and_await() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         let fiber = spawn (fn() => 42)\n\
         let delayed = await(fiber)\n\
         let answer = delayed()\n",
    );
}

#[test]
fn statically_known_and_proven_pure_calls_are_not_rejected_as_unresolved() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         extern fn pureExtern() -> int\n\
         let pureLocal = fn() => 42\n\
         let first = pureExtern()\n\
         let second = pureLocal()\n",
    );
}

#[test]
fn effectful_closure_cannot_escape_through_a_record_field_alias() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Runner = { run: () -> int }\n\
         let runner = Runner { run: fn() => perform Alarm.ring() }\n\
         let delayed = runner.run\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn record_field_effect_propagates_through_a_function_parameter() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Runner = { run: () -> int }\n\
         fn execute(runner: Runner) = {\n\
           let delayed = runner.run\n\
           delayed()\n\
         }\n\
         let runner = Runner { run: fn() => perform Alarm.ring() }\n\
         let answer = execute(runner)\n",
        &["Alarm.ring"],
    );
}

#[test]
fn record_field_effect_propagates_from_a_function_return() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Runner = { run: () -> int }\n\
         fn makeRunner() = Runner { run: fn() => perform Alarm.ring() }\n\
         let runner = makeRunner()\n\
         let delayed = runner.run\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn record_update_preserves_callable_provenance_in_unchanged_fields() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Runner = { run: () -> int, label: string }\n\
         let runner = Runner {\n\
           run: fn() => perform Alarm.ring(),\n\
           label: \"old\"\n\
         }\n\
         let updated = runner { label: \"new\" }\n\
         let delayed = updated.run\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn record_update_replaces_callable_provenance_in_overridden_fields() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         type Runner = { run: () -> int, label: string }\n\
         let runner = Runner {\n\
           run: fn() => perform Alarm.ring(),\n\
           label: \"old\"\n\
         }\n\
         let updated = runner { run: fn() => 42 }\n\
         let delayed = updated.run\n\
         let answer = delayed()\n",
    );
}

#[test]
fn effectful_closure_cannot_escape_through_list_lookup() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let runners = [fn() => perform Alarm.ring()]\n\
         let delayed = match runners[0] {\n\
           Success { value } => value\n\
           Error { message } => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn persistent_list_transforms_preserve_callable_element_provenance() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let runners = listReverse(\n\
           listAppend([], fn() => perform Alarm.ring())\n\
         )\n\
         let delayed = match listGet(runners, 0) {\n\
           Success { value } => value\n\
           Error { message } => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn persistent_map_transforms_preserve_callable_value_provenance() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let runners = mapMerge(\n\
           mapSet(Map(), \"alarm\", fn() => perform Alarm.ring()),\n\
           Map()\n\
         )\n\
         let retained = mapRemove(runners, \"other\")\n\
         let delayed = match mapGet(retained, \"alarm\") {\n\
           Success { value } => value\n\
           Error { message } => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn list_element_effect_propagates_through_a_function_parameter() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn execute(runners) = {\n\
           let delayed = match runners[0] {\n\
             Success { value } => value\n\
             Error { message } => fn() => 0\n\
           }\n\
           delayed()\n\
         }\n\
         let answer = execute([fn() => perform Alarm.ring()])\n",
        &["Alarm.ring"],
    );
}

#[test]
fn effectful_closure_cannot_escape_through_map_lookup() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let runners = { \"alarm\": (fn() => perform Alarm.ring()) }\n\
         let delayed = match runners[\"alarm\"] {\n\
           Success { value } => value\n\
           Error { message } => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn effectful_closure_cannot_escape_through_an_explicit_success_value() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let wrapped = Success { value: fn() => perform Alarm.ring() }\n\
         let delayed = match wrapped {\n\
           Success { value } => value\n\
           Error { message } => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn effectful_closure_cannot_escape_through_positional_union_destructuring() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Wrapped = Empty | Has { run: () -> int }\n\
         let wrapped = Has { run: fn() => perform Alarm.ring() }\n\
         let delayed = match wrapped {\n\
           Has(value) => value\n\
           Empty => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn branch_reassignment_cannot_hide_an_effectful_closure() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         effect Switch { replace: fn() -> Unit }\n\
         mut delayed = fn() => 0\n\
         handle Switch\n\
           replace => { delayed = fn() => perform Alarm.ring() }\n\
         in perform Switch.replace()\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn one_branch_with_an_effect_makes_the_whole_function_effectful() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn choose(flag) = match flag {\n\
           true => perform Alarm.ring()\n\
           false => 0\n\
         }\n\
         let answer = choose(false)\n",
        &["Alarm.ring"],
    );
}

#[test]
fn partial_handler_leaves_unhandled_operation_in_row() {
    assert_rejected_with(
        "effect Pair { first: fn() -> int second: fn() -> int }\n\
         fn both() = [perform Pair.first(), perform Pair.second()]\n\
         let answer = handle Pair\n\
           first => 20\n\
         in both()\n",
        &["Pair.second"],
    );
}

#[test]
fn complementary_nested_partial_handlers_discharge_each_operation() {
    assert_accepted(
        "effect Pair { first: fn() -> int second: fn() -> int }\n\
         fn both() = [perform Pair.first(), perform Pair.second()]\n\
         let answer = handle Pair\n\
           second => 22\n\
         in handle Pair\n\
           first => 20\n\
         in both()\n",
    );
}

#[test]
fn generic_effect_handler_must_match_the_performed_instantiation() {
    assert_rejected_with(
        "effect Stash<T> { put: fn(T) -> Unit take: fn() -> T }\n\
         fn storeNumber() = perform Stash.put(42)\n\
         let done = handle Stash\n\
           put value => {}\n\
           take => \"cached text\"\n\
         in storeNumber()\n",
        &["Stash<int>.put"],
    );
}

#[test]
fn generic_effect_handler_discharges_matching_instantiation() {
    assert_accepted(
        "effect Stash<T> { put: fn(T) -> Unit take: fn() -> T }\n\
         fn storeText() = perform Stash.put(\"fresh text\")\n\
         let done = handle Stash\n\
           put value => {}\n\
           take => \"cached text\"\n\
         in storeText()\n",
    );
}

#[test]
fn declared_generic_effect_contract_must_match_the_required_instantiation() {
    assert_rejected_with(
        "effect Stash<T> { put: fn(T) -> Unit }\n\
         fn bad() -> Unit !Stash<string> = perform Stash.put(42)\n\
         let answer = 0\n",
        &["outside its declared row", "Stash<int>.put"],
    );
}

#[test]
fn handler_arm_cannot_recursively_perform_its_own_effect() {
    assert_rejected_with(
        "effect Loop { again: fn() -> int }\n\
         let answer = handle Loop\n\
           again => perform Loop.again()\n\
         in perform Loop.again()\n",
        &["handler arm `Loop.again`", "recursively re-enter"],
    );
}

#[test]
fn handler_arm_may_perform_a_different_generic_instance_for_an_outer_handler() {
    assert_accepted(
        "effect Relay<T> { fire: fn(T) -> int }\n\
         let answer = handle Relay\n\
           fire value => value + 0 ?: 7\n\
         in handle Relay\n\
           fire text => perform Relay.fire(42)\n\
         in perform Relay.fire(\"start\")\n",
    );
}

#[test]
fn partial_handler_arm_may_perform_an_uncovered_operation_for_an_outer_handler() {
    assert_accepted(
        "effect Pair { first: fn() -> int second: fn() -> int }\n\
         let answer = handle Pair\n\
           second => 22\n\
         in handle Pair\n\
           first => perform Pair.second()\n\
         in perform Pair.first()\n",
    );
}

#[test]
fn handler_arm_secondary_effect_requires_an_outer_handler() {
    let source = "effect Primary { ask: fn() -> int }\n\
         effect Audit { record: fn() -> int }\n\
         let answer = handle Primary\n\
           ask => perform Audit.record()\n\
         in perform Primary.ask()\n";
    assert_rejected_with(source, &["Audit.record"]);

    assert_accepted(
        "effect Primary { ask: fn() -> int }\n\
         effect Audit { record: fn() -> int }\n\
         let answer = handle Audit\n\
           record => 42\n\
         in handle Primary\n\
           ask => perform Audit.record()\n\
         in perform Primary.ask()\n",
    );
}

#[test]
fn declared_effect_row_is_a_contract_not_a_handler() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() -> int !Alarm = perform Alarm.ring()\n\
         let answer = ring()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn declared_row_cannot_hide_another_inferred_effect() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> Unit }\n\
         effect Audit { record: fn() -> Unit }\n\
         fn bad() -> Unit !Alarm = perform Audit.record()\n",
        &["outside its declared row", "Audit.record"],
    );
}

#[test]
fn unknown_declared_effect_is_rejected() {
    assert_rejected_with(
        "fn bad() -> Unit !Imaginary = {}\n",
        &["declares unknown effects", "Imaginary"],
    );
}
