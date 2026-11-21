//! Multiplicity — the second axis of `docs/specs/language/0017-AlgebraicEffects.md`
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
use staging::{assert_rejected, compile_for_target, compile_staged};

// --- Multiplicity: the second axis of the same declaration [MULTI-AXIS] -----

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
    control step: fn(int) -> int
    control abort quit: fn(string) -> int
}
fn run() = {
    let a = perform Job.step(1)
    let b = perform Job.quit("stop")
    satAdd(a, b)
}
fn main() = {
    let v = {
        handle Job {
            step n => resume(n)
            quit reason => 0
        }
        run()
    }
    print("v=${v}")
}
"#;

/// `MIXED_ABORT` with the `abort` keyword (and so the explicit `control`) removed.
fn mixed_undeclared() -> String {
    MIXED_ABORT.replacen("control abort quit", "quit", 1)
}

#[test]
fn the_undeclared_control_pays_for_a_continuation_at_every_perform_site() {
    // [MULTI-COST] The cost table is a contract, not an aspiration, in the same
    // way [STAGE-RESIDUE] is: an `abort` perform site must not allocate a
    // continuation, because the declaration says the arm never resumes.
    //
    // That saving is step 5 of docs/plans/0016-algebraic-effects-and-handlers.md
    // and is blocked on plan 0026 — see the test below for why. What is measurable
    // NOW is the baseline it must beat: without the keyword, BOTH perform sites
    // pay, including the one whose arm never resumes. Pinning it here means the
    // phase-4 measurement is a comparison against a recorded number rather than
    // against a memory.
    assert_eq!(
        suspend_sites(&mixed_undeclared()),
        2,
        "without the keyword every perform site of a resuming region pays for a continuation"
    );
}

/// Every multiplicity violation the checker rejects: the program, the needles
/// its diagnostics must all contain, and what the rejection means.
const REJECTED: &[(&str, &[&str], &str)] = &[
    // resuming an undeclared operation twice is a compile error.
    // [MULTI-COMPAT] [MULTI-HANDLE-ONCE] The one place this axis narrows. An
    // undecorated operation is `once`, which is what the runtime already
    // enforces, so a program that resumes one continuation twice aborts at run
    // time TODAY ("fatal: continuation already resumed") and must be rejected
    // at compile time instead — the same program rejected earlier, not a
    // program that stops working.
    //
    // The check must be syntactic over the arm, never deferred to the runtime
    // guard in src/toolchain/compiler/runtime/effects_coro.c: that guard is a defensive
    // backstop, and a language that can see the arm has no excuse to use it as
    // the normal rejection path.
    (
        r#"
effect Choose { control pick: fn() -> int }
fn both() = satAdd(perform Choose.pick(), 1)
fn main() = {
    let total = {
        handle Choose {
            pick => {
                let a = resume(10)
                let b = resume(20)
                satAdd(a, b)
            }
        }
        both()
    }
    print("total=${total}")
}
"#,
        &["Choose.pick", "once"],
        "expected a compile-time one-shot rejection naming the operation",
    ),
    // an abort arm that resumes is rejected.
    // [MULTI-HANDLE-ABORT] An arm for an `abort` operation must not resume:
    // its value answers the whole region and the `perform` never returns.
    (
        r#"
effect Fail { control abort fail: fn(string) -> int }
fn risky() = satAdd(perform Fail.fail("nope"), 1)
fn main() = {
    let outcome = {
        handle Fail {
            fail reason => resume(0)
        }
        risky()
    }
    print("outcome=${outcome}")
}
"#,
        &["Fail.fail", "abort"],
        "expected the abort-arm rejection naming the arm",
    ),
    // multiplicity on a static operation is rejected.
    // [MULTI-AXIS-STATIC] Static effects sit outside the lattice:
    // [STAGE-STATIC-TAIL] pins them at exactly-once-in-tail-position, the one
    // point where a continuation need not exist. Accepting a multiplicity
    // there would imply a choice the stage has already made.
    (
        r#"
static effect Parallel { control once forEach: fn(int) -> int }
fn main() = {
    let v = {
        handle static Parallel {
            forEach n => n
        }
        perform Parallel.forEach(4)
    }
    print("${v}")
}
"#,
        &["Parallel.forEach", "multiplicity"],
        "expected multiplicity-on-static rejection",
    ),
    // an abort declaration is rejected until a dropped continuation unwinds.
    // [MULTI-HANDLE-ABORT-MODE] [MULTI-COST-ABORT] `abort` inverts the meaning
    // of a `resume`-free arm: it must ABANDON the region rather than substitute
    // its value. Abandoning ends the suspended computation with `pthread_exit`,
    // and a killed thread runs no epilogue — the discarded frames' owned heap
    // operands are never released
    // (docs/specs/language/0017-AlgebraicEffects.md, "Known limits of abandoning a
    // region"). Compiling the arm under today's substituting rule would make
    // the `perform` RETURN, which is the opposite of what the declaration says.
    //
    // So the declaration is rejected until plan 0026 supplies the unwinding.
    // Rejecting is not a placeholder: it is the truthful answer, and the one
    // the broken-code process requires over a silently wrong one.
    (
        MIXED_ABORT,
        &["Job.quit", "unwind"],
        "expected a truthful `abort`-not-yet-unwound rejection",
    ),
    // a many handler over a non replayable body is rejected at the handle site.
    // [MULTI-REPLAY-CHECK] [MULTI-FALSIFY] gate 2. Resuming twice re-runs the
    // remainder of the handled computation, so every non-`E` entry of that
    // expression's row must be `replayable`. Without this check the axis buys
    // nothing: `many` would silently send the email twice, which is the defect
    // no effect row reports today.
    (
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Email { control send: fn(string) -> Unit }
fn placeOrder(id) = {
    let chosen = perform Choice.pick(id)
    perform Email.send("order ${chosen} confirmed")
    chosen
}
fn main() = {
    let best = {
        handle Email {
            send body => resume(print(body))
        }
        handle Choice {
            pick option => resume(option)
        }
        placeOrder(3)
    }
    print("best=${best}")
}
"#,
        &["Email.send", "replayable"],
        "expected the replayability rejection naming the offending effect",
    ),
];

#[test]
fn every_multiplicity_violation_is_rejected_naming_its_operation() {
    for (source, needles, why) in REJECTED {
        assert_rejected(source, Flavor::Default, needles, why);
    }
}

/// The positive `many` shape: one continuation, resumed for every alternative,
/// each resume an independent branch from the choice point
/// ([MULTI-HANDLE-MANY], plan 0016 phase 3). A `many` handler compiles and
/// runs with real multi-shot resumptions — this replaces the old rejection
/// assertion for plain `many`.
const MANY_POSITIVE: &str = r#"
effect Choice { control many pick: fn(int) -> int }
fn answer(seed) = {
    let chosen = perform Choice.pick(seed)
    "a${chosen}"
}
fn main() = {
    let collected = {
        handle Choice {
            pick bound => {
                let low = resume(bound)
                let high = resume(9)
                "${low}&${high}"
            }
        }
        answer(4)
    }
    print("collected=${collected}")
}
"#;

#[test]
fn a_many_handler_compiles_and_runs_independent_branches() {
    // [MULTI-FALSIFY] gate 3: a pure search must produce all alternatives.
    // Compilation proves the re-entrant rejection is lifted; the run proves
    // the second resume re-enters the SAME captured remainder rather than
    // consuming a dead stack — the branch answers "a4" and "a9" both.
    let ir = compile_staged(MANY_POSITIVE);
    assert!(
        ir.contains("__osprey_coro_suspend_many"),
        "a many perform must suspend through the reusable-continuation path"
    );
    let dir = std::env::temp_dir().join(format!("osprey_many_positive_{}", std::process::id()));
    let Ok(()) = std::fs::create_dir_all(&dir) else {
        panic!("could not create temp dir");
    };
    let input = dir.join("many_positive.osp");
    std::fs::write(&input, MANY_POSITIVE).expect("write program");
    let ran = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(input.as_os_str())
        .arg("--run")
        .output()
        .expect("run osprey");
    let _ = std::fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert!(ran.status.success(), "many program failed: {stdout}");
    assert!(
        stdout.contains("collected=a4&a9"),
        "both branch answers must be collected, got: {stdout}"
    );
}

const ONCE_ON_WASM: &str = r#"
effect Async { control once await: fn(int) -> int }
fn pipeline() = satAdd(perform Async.await(1), 1)
fn main() = {
    let v = {
        handle Async {
            await t => resume(t)
        }
        pipeline()
    }
    print("v=${v}")
}
"#;

const MANY_ON_WASM: &str = r#"
effect Choice { control many pick: fn(int) -> int }
fn search(bound) = perform Choice.pick(bound)
fn main() = {
    let best = {
        handle Choice {
            pick bound => resume(bound)
        }
        search(3)
    }
    print("best=${best}")
}
"#;

#[test]
fn wasm32_names_the_continuation_operation_it_cannot_yet_run() {
    // [MULTI-WASM] The stack-switching proposal specifies ONE-SHOT
    // continuations, so `once` is the multiplicity `wasm32` acquires when that
    // proposal lands — with no change to user code. Until then the rejection
    // must name the operation, replacing the undifferentiated
    // "resumable algebraic effects (`resume`)" the target checker emits today.
    // A row that cannot say which effects will start working is a row that
    // cannot be planned against.
    //
    // The current frontend rejects `many` before any target gate. This is a
    // representation limitation, not a permanent wasm ban: portable reusable
    // frames or CPS may satisfy the same source contract.
    for (source, operation) in [(ONCE_ON_WASM, "Async.await"), (MANY_ON_WASM, "Choice.pick")] {
        let (ok, err) = compile_for_target(source, "wasm32");
        assert!(!ok, "wasm32 cannot run {operation}'s continuation yet");
        assert!(
            err.contains(operation),
            "the rejection must name {operation}, got: {err}"
        );
    }
}
