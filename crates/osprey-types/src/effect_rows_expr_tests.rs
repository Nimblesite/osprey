//! Effect-row coverage for the expression forms that carry a row, or a
//! callable's provenance, indirectly: pipes, UFCS method calls, `select`,
//! record updates, namespaced and modular declarations, and the concurrency
//! and collection forms whose payload provenance discharge has to follow.
//!
//! Static discharge short-circuits on a program with no `effect` declaration,
//! so every fixture here declares one — that is what makes these paths
//! reachable at all. Implements [EFFECTS-STATIC-DISCHARGE].

use super::effect_rows_tests::{assert_accepted, assert_rejected_with};
use crate::{check_program, TypeError};
use osprey_ast::{Expr, FieldAssignment, Program, Stmt};
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Fragments every "this escaped to the entry point" diagnostic carries.
const UNHANDLED_RING: &[&str] = &["unhandled effect operations at program entry", "Alarm.ring"];

#[test]
fn both_pipe_shapes_carry_the_callee_row_to_the_call_site() {
    // A bare identifier on the right applies the piped value as the sole
    // argument; a call on the right prepends it to the written arguments.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn wake(n) = (n + perform Alarm.ring()) ?: n\n\
         let woken = 1 |> wake\n",
        UNHANDLED_RING,
    );
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn wake(n, m) = (n + m) ?: perform Alarm.ring()\n\
         let woken = 1 |> wake(2)\n",
        UNHANDLED_RING,
    );
}

#[test]
fn a_handler_discharges_both_pipe_shapes() {
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn wake(n) = (n + perform Alarm.ring()) ?: n\n\
         fn twice(n, m) = (n + m) ?: perform Alarm.ring()\n\
         let quiet = handle Alarm\n\
           ring => 0\n\
         in ((1 |> wake) |> twice(2))\n",
    );
}

#[test]
fn a_piped_callback_keeps_its_provenance_through_the_pipe() {
    // The pipe hands `ring` to `apply` as a VALUE: discharge must still see a
    // known callable on the other side, not an unprovable dynamic call.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn apply(callback) = callback()\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = ring |> apply\n",
        UNHANDLED_RING,
    );
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn apply(callback) = callback()\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = handle Alarm\n\
           ring => 42\n\
         in (ring |> apply)\n",
    );
}

#[test]
fn a_ufcs_method_call_carries_the_row_of_the_function_it_names() {
    // `value.wake()` lowers to `wake(value)`, so the row travels through the
    // method-call form exactly as through the call form.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn wake(n) = (n + perform Alarm.ring()) ?: n\n\
         let start = 1\n\
         let woken = start.wake()\n",
        UNHANDLED_RING,
    );
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn wake(n) = (n + perform Alarm.ring()) ?: n\n\
         let start = 1\n\
         let quiet = handle Alarm\n\
           ring => 0\n\
         in start.wake()\n",
    );
}

#[test]
fn a_ufcs_method_call_returning_a_callable_keeps_its_provenance() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn pick(n) = ring\n\
         let one = 1\n\
         let answer = one.pick()()\n",
        UNHANDLED_RING,
    );
}

#[test]
fn a_select_arm_body_contributes_its_row() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let chosen = select {\n\
           first => ring()\n\
           second => 0\n\
         }\n",
        &["Alarm.ring"],
    );
}

#[test]
fn a_record_update_keeps_the_callable_provenance_of_every_field() {
    // The updated record inherits the untouched fields from the original and
    // replaces the named ones; invoking either must resolve to a real callee.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Handlers = { onTick: fn() -> int, onDone: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn quiet() = 0\n\
         let base = Handlers { onTick: quiet, onDone: quiet }\n\
         let armed = base { onTick: ring }\n\
         let chosen = armed.onTick\n\
         let answer = chosen()\n",
        UNHANDLED_RING,
    );
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         type Handlers = { onTick: fn() -> int, onDone: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn quiet() = 0\n\
         let base = Handlers { onTick: ring, onDone: quiet }\n\
         let disarmed = base { onTick: quiet }\n\
         let chosen = disarmed.onTick\n\
         let answer = chosen()\n",
    );
}

#[test]
fn a_namespaced_declaration_is_indexed_and_checked_against_its_declared_row() {
    // A namespace nests the scope the declaration is collected under, so the
    // row contract has to be resolved through the qualified name to be checked
    // at all.
    assert_rejected_with(
        "namespace bells;\n\
         effect Alarm { ring: fn() -> int }\n\
         effect Clock { now: fn() -> int }\n\
         fn peal() -> int !Clock = perform Alarm.ring()\n",
        &["outside its declared row", "Alarm.ring"],
    );
    assert_accepted(
        "namespace bells;\n\
         effect Alarm { ring: fn() -> int }\n\
         fn peal() -> int !Alarm = perform Alarm.ring()\n",
    );
}

#[test]
fn a_spawned_fiber_carries_its_bodys_row_and_its_awaited_provenance() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let running = spawn ring()\n\
         let answer = await running\n",
        UNHANDLED_RING,
    );
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let answer = handle Alarm\n\
           ring => 42\n\
         in await (spawn ring())\n",
    );
}

#[test]
fn a_channel_payload_keeps_the_provenance_of_what_was_sent() {
    // What comes back out of `recv` is what went in through `send`: a callable
    // sent down a channel and invoked on the far side is still a KNOWN callee.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let channel = Channel()\n\
         let sent = send(channel, ring)\n\
         let received = recv(channel)\n\
         let answer = received()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn an_element_mapped_into_one_callback_and_invoked_in_another_fails_closed() {
    // `map` projects its callback's returned provenance onto the element, but
    // that element then crosses into a SEPARATE eager callback slot, where it
    // is no longer a statically tracked value path. Discharge refuses instead
    // of assuming the invocation is pure — the fail-closed direction is the
    // one that matters, since guessing wrong here would silently drop an
    // unhandled effect.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> Unit }\n\
         fn ring() = perform Alarm.ring()\n\
         let armed = map(range(0, 3), |n| => ring)\n\
         let done = forEach(armed, |armedRing| => armedRing())\n",
        &["effect provenance cannot be proven"],
    );
}

#[test]
fn an_indexed_element_of_an_unprovable_list_still_fails_closed() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let hidden = [ring]\n\
         fn pick(list) = list[0]\n\
         let answer = (pick(hidden))()\n",
        &["effect provenance cannot be proven"],
    );
}

// ---------------------------------------------------------------------------
// Hand-built AST nodes.
//
// Both surface lowerers desugar `x |> f` into a call and `t.m(..)` into a call,
// so neither an `Expr::Pipe` nor an `Expr::MethodCall` ever survives parsing.
// Discharge still handles them, because every other consumer of the AST does
// and a node that reached the checker unhandled would be a silent fail-open.
// Building the node directly is the only way to exercise that handling — the
// same approach `expr.rs` already takes for `infer_method_call`.
// ---------------------------------------------------------------------------

/// A base program whose `wake` performs, whose `pick` RETURNS the performer,
/// and whose `ring` is the performer itself.
const SYNTHETIC_BASE: &str = "effect Alarm { ring: fn() -> int }\n\
                              fn ring() = perform Alarm.ring()\n\
                              fn wake(n) = (n + perform Alarm.ring()) ?: n\n\
                              fn pick(n) = ring\n\
                              type Handlers = { onTick: fn() -> int }\n\
                              fn quiet() = 0\n\
                              let base = Handlers { onTick: quiet }\n";

fn check_with_synthetic(extra: Vec<Stmt>) -> Vec<TypeError> {
    let parsed = parse_program_with_flavor(SYNTHETIC_BASE, Flavor::Default);
    assert!(
        parsed.errors.is_empty(),
        "test fixture has syntax errors: {:?}",
        parsed.errors
    );
    let mut statements = parsed.program.statements;
    statements.extend(extra);
    check_program(&Program { statements })
}

fn binding(name: &str, value: Expr) -> Stmt {
    Stmt::Let {
        name: name.into(),
        mutable: false,
        ty: None,
        value,
        doc: None,
        position: None,
    }
}

fn call(function: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(function),
        arguments,
        named_arguments: Vec::new(),
    }
}

fn identifier(name: &str) -> Expr {
    Expr::Identifier(name.into())
}

fn assert_mentions(errors: &[TypeError], fragment: &str) {
    assert!(
        errors.iter().any(|error| error.message.contains(fragment)),
        "expected a diagnostic containing {fragment:?}, got: {errors:#?}"
    );
}

/// The two pipe shapes, as `Expr::Pipe` nodes: a bare callee on the right, and
/// a call on the right that the piped value is prepended to.
fn pipes(callee: &str) -> [Expr; 2] {
    [
        Expr::Pipe {
            left: Box::new(Expr::Integer(1)),
            right: Box::new(identifier(callee)),
        },
        Expr::Pipe {
            left: Box::new(Expr::Integer(1)),
            right: Box::new(call(identifier(callee), Vec::new())),
        },
    ]
}

#[test]
fn a_pipe_node_carries_the_row_of_whatever_stands_on_its_right() {
    for piped in pipes("wake") {
        assert_mentions(
            &check_with_synthetic(vec![binding("piped", piped)]),
            "Alarm.ring",
        );
    }
}

#[test]
fn a_pipe_node_carries_the_provenance_of_the_callable_it_yields() {
    // `pick` RETURNS `ring`, so the pipe's value — not just its row — has to
    // survive: invoking the result must resolve to a known performer rather
    // than to an unprovable dynamic call.
    for piped in pipes("pick") {
        let errors = check_with_synthetic(vec![
            binding("piped", piped),
            binding("answer", call(identifier("piped"), Vec::new())),
        ]);
        assert_mentions(&errors, "Alarm.ring");
    }
}

#[test]
fn a_method_call_node_carries_both_the_row_and_the_provenance_of_its_method() {
    // `1.wake()` names `wake` with the receiver prepended, exactly as the call
    // form does.
    assert_mentions(
        &check_with_synthetic(vec![binding(
            "woken",
            Expr::MethodCall {
                target: Box::new(Expr::Integer(1)),
                method: "wake".into(),
                arguments: Vec::new(),
                named_arguments: Vec::new(),
            },
        )]),
        "Alarm.ring",
    );
    let errors = check_with_synthetic(vec![
        binding(
            "picked",
            Expr::MethodCall {
                target: Box::new(Expr::Integer(1)),
                method: "pick".into(),
                arguments: Vec::new(),
                named_arguments: Vec::new(),
            },
        ),
        binding("answer", call(identifier("picked"), Vec::new())),
    ]);
    assert_mentions(&errors, "Alarm.ring");
}

#[test]
fn an_update_node_replaces_only_the_fields_it_names() {
    // The overridden field carries the performer; the untouched fields keep
    // whatever the base record already proved about them.
    let update = |field: &str, value: &str| Expr::Update {
        record: "base".into(),
        fields: vec![FieldAssignment {
            name: field.into(),
            value: identifier(value),
        }],
    };
    let errors = check_with_synthetic(vec![
        binding("armed", update("onTick", "ring")),
        binding(
            "chosen",
            Expr::FieldAccess {
                target: Box::new(identifier("armed")),
                field: "onTick".into(),
            },
        ),
        binding("answer", call(identifier("chosen"), Vec::new())),
    ]);
    assert_mentions(&errors, "Alarm.ring");
}

#[test]
fn a_structural_pattern_binds_each_field_it_names() {
    // `{ onTick }` destructures without naming the type, so the binding has to
    // be projected out of the matched record's own provenance.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Handlers = { onTick: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         let armed = Handlers { onTick: ring }\n\
         let answer = match armed {\n\
           { onTick } => onTick()\n\
         }\n",
        UNHANDLED_RING,
    );
}

#[test]
fn named_arguments_are_matched_to_their_parameter_by_name() {
    // A named argument arrives out of positional order, so both the call's
    // value slots and a performed operation's argument slots have to be filled
    // by NAME or the callable travelling in one would be lost.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn apply(callback) = callback()\n\
         let answer = apply(callback: ring)\n",
        UNHANDLED_RING,
    );
    assert_rejected_with(
        "effect Alarm { ring: fn(int) -> int }\n\
         fn wake() = perform Alarm.ring(n: 1)\n\
         let answer = wake()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn a_qualified_path_resolves_to_the_declaration_it_names() {
    // Inside a namespace the declaration is indexed under `bells::ring`, so a
    // path used as a VALUE has to resolve through the qualified name to keep
    // its provenance.
    assert_rejected_with(
        "namespace bells;\n\
         effect Alarm { ring: fn() -> int }\n\
         effect Clock { now: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         fn apply(callback) = callback()\n\
         fn drive() -> int !Clock = apply(bells::ring)\n",
        &["outside its declared row", "Alarm.ring"],
    );
}

#[test]
fn a_generic_effect_whose_instantiation_is_unresolved_still_reaches_the_entry() {
    // Nothing pins `T` here, so neither the perform site nor the handler site
    // has a resolved instantiation to key on. Discharge has to name the site
    // itself rather than silently collapse every unresolved instance into one
    // — two unrelated performs must not discharge each other's handler.
    assert_rejected_with(
        "effect Stash<T> { take: fn() -> T }\n\
         fn fetch() = perform Stash.take()\n\
         let answer = fetch()\n",
        &["Stash", "take"],
    );
}

#[test]
fn invoking_what_an_idle_channel_yields_fails_closed() {
    // Nothing was ever sent, so `recv` yields a callable of unknown
    // provenance. Invoking it must be refused rather than assumed pure.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         let idle = Channel()\n\
         let received = recv(idle)\n\
         let answer = received()\n",
        &["effect provenance cannot be proven"],
    );
}

#[test]
fn a_generic_effect_performed_in_dead_code_names_its_site_rather_than_a_type() {
    // `hoard` is never called, so inference never pins `T` and the perform
    // site has no resolved instantiation. Discharge keys on the SITE instead,
    // so two unresolved performs of the same operation stay distinct — one
    // handler must not silently discharge the other.
    assert_accepted(
        "effect Stash<T> { put: fn(T) -> Unit }\n\
         fn hoard(value) = perform Stash.put(value)\n\
         let answer = 0\n",
    );
}
