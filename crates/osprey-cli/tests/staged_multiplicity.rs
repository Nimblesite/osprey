//! Multiplicity — the second axis of `docs/specs/0035-StagedEffects.md`
//! ([MULTI-AXIS]).
//!
//! Stage says WHEN a request is answered; multiplicity says HOW MANY TIMES. A
//! handler that resumes twice re-runs the remainder of the handled
//! computation, so a body that sends an email sends it twice; a handler that
//! never resumes ends the computation where it stands. Those are different
//! programs, they cost different amounts to represent, and they run on
//! different targets — so an effect row that cannot tell them apart is
//! under-specified.
//!
//! The stage axis lives in `staged_effects.rs`; the two share
//! `common::staging` because they are two axes of one declaration.

#[path = "common/staging.rs"]
mod staging;

use osprey_syntax::Flavor;
use staging::{compile_for_target, compile_staged, diagnostics};

// --- Multiplicity: the second axis of the same declaration [MULTI-AXIS] -----

#[test]
fn resuming_an_undeclared_operation_twice_is_a_compile_error() {
    // [MULTI-COMPAT] [MULTI-HANDLE-ONCE] The one place this axis narrows. An
    // undecorated operation is `once`, which is what the runtime already
    // enforces, so a program that resumes one continuation twice aborts at run
    // time TODAY ("fatal: continuation already resumed") and must be rejected
    // at compile time instead — the same program rejected earlier, not a
    // program that stops working.
    //
    // The check must be syntactic over the arm, never deferred to the runtime
    // guard in compiler/runtime/effects_coro.c: that guard is a defensive
    // backstop, and a language that can see the arm has no excuse to use it as
    // the normal rejection path.
    let source = r#"
effect Choose { pick: fn() -> int }
fn both() = (perform Choose.pick() + 1) ?: 0
fn main() = {
    let total = handle Choose
        pick => {
            let a = resume(10)
            let b = resume(20)
            a + b ?: 0
        }
    in both()
    print("total=${total}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("Choose.pick") && errors.contains("once"),
        "expected a compile-time one-shot rejection naming the operation, got: {errors:?}"
    );
}

#[test]
fn an_abort_arm_that_resumes_is_rejected() {
    // [MULTI-HANDLE-ABORT] An arm for an `abort` operation must not resume:
    // its value answers the whole region and the `perform` never returns.
    let source = r#"
effect Fail { abort fail: fn(string) -> int }
fn risky() = (perform Fail.fail("nope") + 1) ?: 0
fn main() = {
    let outcome = handle Fail
        fail reason => resume(0)
    in risky()
    print("outcome=${outcome}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("Fail.fail") && errors.contains("abort"),
        "expected the abort-arm rejection naming the arm, got: {errors:?}"
    );
}

#[test]
fn multiplicity_on_a_static_operation_is_rejected() {
    // [MULTI-AXIS-STATIC] Static effects sit outside the lattice:
    // [STAGE-STATIC-TAIL] pins them at exactly-once-in-tail-position, the one
    // point where a continuation need not exist. Accepting a multiplicity
    // there would imply a choice the stage has already made.
    let source = r#"
static effect Parallel { once forEach: fn(int) -> int }
fn main() = {
    let v = handle static Parallel
        forEach n => n
    in perform Parallel.forEach(4)
    print("${v}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("Parallel.forEach") && errors.contains("multiplicity"),
        "expected multiplicity-on-static rejection, got: {errors:?}"
    );
}

/// The `__osprey_coro_suspend` call sites in `source`'s IR — one per `perform`
/// that pays for a continuation.
fn suspend_sites(source: &str) -> usize {
    compile_staged(source)
        .lines()
        .filter(|line| line.contains("call") && line.contains("__osprey_coro_suspend"))
        .count()
}

/// One effect, one arm that resumes and one that never does. `MIXED_ABORT`
/// declares the non-resuming operation `abort`; `MIXED_UNDECLARED` is the same
/// program without the keyword, and is the control that proves the assertion
/// below can fail.
const MIXED_ABORT: &str = r#"
effect Job {
    step: fn(int) -> int
    abort quit: fn(string) -> int
}
fn run() = {
    let a = perform Job.step(1)
    let b = perform Job.quit("stop")
    a + b ?: 0
}
fn main() = {
    let v = handle Job
        step n => resume(n)
        quit reason => 0
    in run()
    print("v=${v}")
}
"#;

const MIXED_UNDECLARED: &str = r#"
effect Job {
    step: fn(int) -> int
    quit: fn(string) -> int
}
fn run() = {
    let a = perform Job.step(1)
    let b = perform Job.quit("stop")
    a + b ?: 0
}
fn main() = {
    let v = handle Job
        step n => resume(n)
        quit reason => 0
    in run()
    print("v=${v}")
}
"#;

#[test]
fn the_undeclared_control_pays_for_a_continuation_at_every_perform_site() {
    // [MULTI-COST] The cost table is a contract, not an aspiration, in the same
    // way [STAGE-RESIDUE] is: an `abort` perform site must not allocate a
    // continuation, because the declaration says the arm never resumes.
    //
    // That saving is phase 4 of docs/plans/0028-resumption-multiplicity.md and
    // is blocked on plan 0026 — see the test below for why. What is measurable
    // NOW is the baseline it must beat: without the keyword, BOTH perform sites
    // pay, including the one whose arm never resumes. Pinning it here means the
    // phase-4 measurement is a comparison against a recorded number rather than
    // against a memory.
    assert_eq!(
        suspend_sites(MIXED_UNDECLARED),
        2,
        "without the keyword every perform site of a resuming region pays for a continuation"
    );
}

#[test]
fn an_abort_declaration_is_rejected_until_a_dropped_continuation_unwinds() {
    // [MULTI-HANDLE-ABORT-MODE] [MULTI-COST-ABORT] `abort` inverts the meaning
    // of a `resume`-free arm: it must ABANDON the region rather than substitute
    // its value. Abandoning ends the suspended computation with `pthread_exit`,
    // and a killed thread runs no epilogue — the discarded frames' owned heap
    // operands are never released
    // (docs/specs/0017-AlgebraicEffects.md, "Known limits of abandoning a
    // region"). Compiling the arm under today's substituting rule would make
    // the `perform` RETURN, which is the opposite of what the declaration says.
    //
    // So the declaration is rejected until plan 0026 supplies the unwinding.
    // Rejecting is not a placeholder: it is the truthful answer, and the one
    // the broken-code process requires over a silently wrong one.
    let errors = diagnostics(MIXED_ABORT, Flavor::Default);
    assert!(
        errors.contains("Job.quit") && errors.contains("unwind"),
        "expected a truthful `abort`-not-yet-unwound rejection, got: {errors:?}"
    );
}

#[test]
fn a_many_arm_is_rejected_because_no_re_entrant_continuation_exists() {
    // [MULTI-HANDLE-MANY] [MULTI-COST] Native resume is ONE suspended stack,
    // switched to and never switched back; a live pthread stack cannot be
    // cloned, so `many` has no representation until plan 0016 builds one.
    //
    // Accepting the declaration anyway would answer a re-entrant request with a
    // continuation that cannot be re-entered — a silently wrong answer, which
    // the broken-code process forbids. So the rejection IS the contract, and it
    // must name the operation and the plan that lifts it.
    //
    // The [MULTI-HANDLE-MANY-LEXICAL] relaxation this program needs (a lambda
    // the arm CONSUMES before returning does have a live continuation) is
    // phase 5, and lands with the runtime that makes the program runnable.
    let source = r#"
effect Choice { many pick: fn(int) -> int }
fn search(bound) = perform Choice.pick(bound)
fn main() = {
    let best = handle Choice
        pick bound => range(0, bound) |> fold(0, |carried, option| => {
            let answered = resume(option)
            match answered > carried { true => answered false => carried }
        })
    in search(3)
    print("best=${best}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("Choice.pick") && errors.contains("re-entrant continuation"),
        "expected a truthful `many`-has-no-representation rejection, got: {errors:?}"
    );
}

#[test]
fn a_many_handler_over_a_non_replayable_body_is_rejected_at_the_handle_site() {
    // [MULTI-REPLAY-CHECK] [MULTI-FALSIFY] gate 2. Resuming twice re-runs the
    // remainder of the handled computation, so every non-`E` entry of that
    // expression's row must be `replayable`. Without this check the axis buys
    // nothing: `many` would silently send the email twice, which is the defect
    // no effect row reports today.
    let source = r#"
effect Choice { many pick: fn(int) -> int }
effect Email { send: fn(string) -> Unit }
fn placeOrder(id) = {
    let chosen = perform Choice.pick(id)
    perform Email.send("order ${chosen} confirmed")
    chosen
}
fn main() = {
    let best = handle Email
        send body => resume(print(body))
    in handle Choice
        pick option => resume(option)
    in placeOrder(3)
    print("best=${best}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("Email.send") && errors.contains("replayable"),
        "expected the replayability rejection naming the offending effect, got: {errors:?}"
    );
}

const ONCE_ON_WASM: &str = r#"
effect Async { once await: fn(int) -> int }
fn pipeline() = (perform Async.await(1) + 1) ?: 0
fn main() = {
    let v = handle Async
        await t => resume(t)
    in pipeline()
    print("v=${v}")
}
"#;

const MANY_ON_WASM: &str = r#"
effect Choice { many pick: fn(int) -> int }
fn search(bound) = perform Choice.pick(bound)
fn main() = {
    let best = handle Choice
        pick bound => resume(bound)
    in search(3)
    print("best=${best}")
}
"#;

#[test]
fn wasm32_names_the_once_operation_it_cannot_yet_run() {
    // [MULTI-WASM] The stack-switching proposal specifies ONE-SHOT
    // continuations, so `once` is the multiplicity `wasm32` acquires when that
    // proposal lands — with no change to user code. Until then the rejection
    // must name the operation, replacing the undifferentiated
    // "resumable algebraic effects (`resume`)" the target checker emits today.
    // A row that cannot say which effects will start working is a row that
    // cannot be planned against.
    let (ok, err) = compile_for_target(ONCE_ON_WASM, "wasm32");
    assert!(!ok, "wasm32 cannot run a one-shot continuation yet");
    assert!(
        err.contains("Async.await"),
        "the rejection must name the operation, got: {err}"
    );
}

#[test]
fn wasm32_rejects_many_before_the_target_gate_is_reached() {
    // [MULTI-WASM] Multi-shot is out of the stack-switching proposal's scope,
    // so `many` is the one multiplicity `wasm32` will never acquire — a
    // PERMANENT rejection, unlike `once` above.
    //
    // That distinction is not observable yet: no target can run `many`, so the
    // checker rejects the handle site before the per-target gate is consulted.
    // The test asserts what actually happens and names the operation, so the
    // day plan 0016 makes `many` run natively this is the test that notices
    // wasm32 needs its own permanent wording.
    let (ok, err) = compile_for_target(MANY_ON_WASM, "wasm32");
    assert!(!ok, "wasm32 cannot run a multi-shot continuation");
    assert!(
        err.contains("Choice.pick"),
        "the rejection must name the operation, got: {err}"
    );
}
