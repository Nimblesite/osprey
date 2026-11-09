//! Closures, aliases and channels: an effectful callable keeps its provenance wherever it travels, and a handler is lexical authority that never escapes.

use crate::effect_rows_tests::{
    accept_alarm, assert_accepted, assert_rejected_with, handled, reject_alarm, ALARM, ALARM_ARG,
};

/// The guard on the fix above: provenance verdicts are re-derived after the
/// rows converge, NOT merely erased. A call this pass genuinely cannot resolve
/// must still be reported when it hides inside a returned closure, where the
/// stored summary is what the next iteration reads back.
///
/// The callee is an element that crossed into a SEPARATE eager callback slot,
/// as in `an_element_mapped_into_one_callback_and_invoked_in_another_fails_
/// closed` — the slot now hands the callback the element, so the invocation
/// resolves to the stored `ring` and is reported by its own truthful
/// diagnostic, `unhandled effect operations at program entry`, instead of
/// failing closed on the analyser's inability to see the binding.
#[test]
fn an_unresolvable_call_inside_a_returned_closure_is_still_rejected() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> Unit }\n\
         fn ring() = perform Alarm.ring()\n\
         let armed = map(range(0, 3), |n| => ring)\n\
         fn make() = fn() => forEach(armed, |armedRing| => armedRing())\n\
         let answer = make()()\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

/// A callee named by a file-scope `let` resolves through the environment seeded
/// under every function body, so its effects are the CALLER's effects: the
/// program is rejected for the operation it actually performs rather than for
/// the analyser's inability to see the binding.
#[test]
fn a_file_scope_alias_reports_the_effect_it_performs() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let siren = ring\n\
         fn relay() = siren()\n\
         let answer = relay()\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

/// The other half of the same seeding: once the alias IS handled, nothing is
/// reported. A file-scope handler and a file-scope generic binding are both
/// sanctioned by [MODULES-FILE-SCOPE-BINDING] and must coexist — the
/// provenance check used to fail closed on the pair.
#[test]
fn a_handled_file_scope_alias_is_accepted() {
    assert_accepted(&format!(
        "effect Alarm {{ control ring: fn() -> int }}\nfn ringer() = perform Alarm.ring()\nlet siren = ringer\nfn relay() = siren()\n{}",
        handled("got", "Alarm", "ring => resume(7)", "relay()")
    ));
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
    reject_alarm(
        "let answer = perform Alarm.ring()\n",
        &["unhandled effect operations at program entry", "Alarm.ring"],
    );
}

#[test]
fn inferred_helper_and_transitive_call_chain_propagate_effects() {
    reject_alarm(
        "fn ring() = perform Alarm.ring()\n\
         fn relay() = ring()\n\
         fn relayAgain() = relay()\n\
         let answer = relayAgain()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn an_unused_effectful_function_is_latent_and_accepted() {
    accept_alarm(
        "fn dormant() = perform Alarm.ring()\n\
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
    accept_alarm(&format!(
        "fn ring() = perform Alarm.ring()\n\
         fn relay() = ring()\n\
         {}",
        handled("answer", "Alarm", "ring => 42", "relay()")
    ));
}

#[test]
fn user_higher_order_function_propagates_callback_effect() {
    reject_alarm(
        "fn apply(callback) = callback()\n\
         fn forward(callback) = apply(callback)\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = forward(ring)\n",
        &["Alarm.ring"],
    );
}

#[test]
fn user_higher_order_callback_is_discharged_at_call_site() {
    accept_alarm(&format!(
        "fn apply(callback) = callback()\n\
         fn ring() = perform Alarm.ring()\n\
         {}",
        handled("answer", "Alarm", "ring => 42", "apply(ring)")
    ));
}

#[test]
fn higher_order_function_may_handle_its_callback_internally() {
    accept_alarm(
        "fn guarded(callback) = {\n\
         handle Alarm {\n\
             ring => 42\n\
         }\n\
         callback()\n\
         }\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = guarded(ring)\n",
    );
}

#[test]
fn constructing_an_effectful_lambda_is_pure_until_invoked() {
    accept_alarm(
        "let delayed = fn() => perform Alarm.ring()\n\
         let answer = 42\n",
    );
}

#[test]
fn invoking_an_effectful_lambda_outside_a_handler_is_rejected() {
    reject_alarm(
        "let delayed = fn() => perform Alarm.ring()\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn lambda_constructed_under_handler_cannot_escape_its_authority() {
    reject_alarm(
        &format!(
            "let delayed = {}let answer = delayed()\n",
            handled(
                "delayed",
                "Alarm",
                "ring => 41",
                "fn() => perform Alarm.ring()"
            )
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn invoking_an_effectful_lambda_inside_handler_is_accepted() {
    accept_alarm(&format!(
        "let delayed = fn() => perform Alarm.ring()\n{}",
        handled("answer", "Alarm", "ring => 42", "delayed()")
    ));
}

#[test]
fn lambda_returned_by_higher_order_function_keeps_captured_callback_effects() {
    reject_alarm(
        "fn defer(callback) = fn() => callback()\n\
         fn ring() = perform Alarm.ring()\n\
         let delayed = defer(ring)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn nested_returned_lambdas_keep_captured_callback_effects() {
    reject_alarm(
        "fn ring() = perform Alarm.ring()\n\
         fn deferTwice(callback) = fn() => fn() => callback()\n\
         let first = deferTwice(ring)\n\
         let second = first()\n\
         let answer = second()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn named_callback_argument_keeps_its_effect_when_returned() {
    reject_alarm(
        "fn ring() = perform Alarm.ring()\n\
         fn identity(callback) = callback\n\
         let delayed = identity(callback: ring)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn local_identity_lambda_keeps_a_returned_callback_effect() {
    reject_alarm(
        "fn ring() = perform Alarm.ring()\n\
         let identity = fn(callback) => callback\n\
         let delayed = identity(ring)\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn closure_returned_through_match_keeps_every_branch_effect() {
    reject_alarm(
        "fn choose(flag) = match flag {\n\
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
    reject_alarm(
        "let delayed = {\n\
         handle Alarm {\n\
             ring => 41\n\
         }\n\
         let nested = fn() => perform Alarm.ring()\n\
         nested\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn callable_returned_by_a_handler_arm_keeps_its_latent_effect() {
    reject_alarm(
        "effect Factory { make: fn() -> () -> int }\n\
         let delayed = {\n\
             handle Factory {\n\
                 make => fn() => perform Alarm.ring()\n\
             }\n\
             perform Factory.make()\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn handler_arm_invocation_keeps_effects_of_operation_callback_arguments() {
    reject_alarm(
        "effect Gate { apply: fn(() -> int) -> int }\n\
         let answer = {\n\
             handle Gate {\n\
                 apply callback => callback()\n\
             }\n\
             perform Gate.apply(fn() => perform Alarm.ring())\n\
         }\n",
        &["Alarm.ring"],
    );
}

#[test]
fn operation_callback_argument_effect_can_be_discharged_by_an_outer_handler() {
    accept_alarm(&format!(
        "effect Gate {{ apply: fn(() -> int) -> int }}\n{}",
        handled(
            "answer",
            "Alarm",
            "ring => 42",
            "handle Gate {\napply callback => callback()\n}\nperform Gate.apply(fn() => perform Alarm.ring())"
        )
    ));
}

#[test]
fn lazy_iterator_callback_cannot_escape_its_construction_handler() {
    assert_rejected_with(
        &format!(
            "{ALARM_ARG}let mapped = {}\nforEach(mapped, fn(value) => print(toString(value)))\n",
            handled(
                "mapped",
                "Alarm",
                "ring value => value",
                "map(range(0, 1), fn(value) => perform Alarm.ring(value))"
            )
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn lazy_iterator_callback_is_discharged_when_consumed_inside_handler() {
    assert_accepted(&format!(
        "{ALARM_ARG}{}",
        handled(
            "_",
            "Alarm",
            "ring value => value",
            "forEach(\nmap(range(0, 1), fn(value) => perform Alarm.ring(value)),\nfn(value) => print(toString(value))\n)"
        )
    ));
}

#[test]
fn effectful_closure_cannot_escape_through_a_channel_alias() {
    reject_alarm(
        &format!(
            "let channel = Channel(1)\nlet alias = channel\nlet _ = {}\nlet delayed = recv(channel)\nlet answer = delayed()\n",
            handled("_", "Alarm", "ring => 41", "send(alias, fn() => perform Alarm.ring())")
        ),
        &["Alarm.ring"],
    );
}

/// A pure closure round-tripped through a channel alias and invoked after
/// `recv`; both tests below assert this exact program stays callable.
fn pure_channel_round_trip() -> String {
    format!("{ALARM}let channel = Channel(1)\nlet alias = channel\nsend(alias, fn() => 42)\nlet delayed = recv(channel)\nlet answer = delayed()\n")
}

#[test]
fn pure_dynamic_closure_survives_a_channel_alias() {
    assert_accepted(&pure_channel_round_trip());
}

#[test]
fn pure_closure_round_tripped_through_a_channel_alias_remains_callable() {
    assert_accepted(&pure_channel_round_trip());
}

#[test]
fn effectful_closure_cannot_escape_through_spawn_and_await() {
    reject_alarm(
        &format!(
            "let fiber = {}\nlet delayed = await(fiber)\nlet answer = delayed()\n",
            handled(
                "fiber",
                "Alarm",
                "ring => 41",
                "spawn (fn() => perform Alarm.ring())"
            )
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn pure_dynamic_closure_survives_spawn_and_await() {
    accept_alarm(
        "let fiber = spawn (fn() => 42)\nlet delayed = await(fiber)\nlet answer = delayed()\n",
    );
}

#[test]
fn statically_known_and_proven_pure_calls_are_not_rejected_as_unresolved() {
    accept_alarm(
        "extern fn pureExtern() -> int\n\
         let pureLocal = fn() => 42\n\
         let first = pureExtern()\n\
         let second = pureLocal()\n",
    );
}
