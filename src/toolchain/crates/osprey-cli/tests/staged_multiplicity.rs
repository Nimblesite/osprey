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
    // a many arm that mutates a captured cell INDIRECTLY, through a lambda
    // bound outside the arm whose body assigns it. [MULTI-REPLAY-STATE]
    // Branches share the one cell, and hiding the write behind a called
    // local function is exactly the helper the rule must see through — the
    // direct form below is the control that proves the assertion can fire.
    (
        r#"
effect Choice { control many pick: fn(int) -> string }
effect Journal { replayable note: fn(string) -> Unit }
fn main() = {
    mut log = ""
    let note = fn(t) => {
        handle Journal {
            note _t => { log = "${log}${t}" }
        }
        perform Journal.note(t)
    }
    let out = {
        handle Choice {
            pick _b => {
                note("arm")
                let a = resume("1")
                note("|mid${a}")
                let b = resume("2")
                "(${a},${b})"
            }
        }
        perform Choice.pick(0)
    }
    print(out)
    print("log=[${log}]")
}
"#,
        &["Choice.pick", "captures mutable binding `log`"],
        "expected the indirect mutation through a called lambda to be rejected",
    ),
    // the control: the same mutation written directly in the many arm.
    // [MULTI-REPLAY-STATE] A write the arm performs on a visible cell is the
    // plain form of the rule; it stays rejected when the scope resolution is
    // fixed so only genuinely visible cells count.
    (
        r#"
effect Choice { control many pick: fn(int) -> string }
effect Journal { replayable note: fn(string) -> Unit }
fn main() = {
    mut log = ""
    let out = {
        handle Choice {
            pick _b => {
                handle Journal {
                    note _t => { log = "${log}x" }
                }
                perform Journal.note("y")
                "${resume(1)},${resume(2)}"
            }
        }
        perform Choice.pick(0)
    }
    print(out)
    print("log=[${log}]")
}
"#,
        &["Choice.pick", "captures mutable binding `log`"],
        "expected the direct mutation in the arm to keep its rejection",
    ),
];

/// [FLAVOR-PYTHON-HANDLER] All three frontends must name the same operation
/// and reject it for the same reason, including through a named handler.
#[test]
fn multiplicity_rejections_agree_across_three_flavors() {
    for (case, sources) in [
        ("once", [
            "effect Choose { control pick: fn() -> int }\nfn trial() = {\nhandle Choose { pick => satAdd(resume(1), resume(2)) }\nperform Choose.pick()\n}\nprint(trial())\n",
            "effect Choose\n    control pick : Unit => int\ntrial () =\n    handle Choose\n        pick => satAdd (resume 1) (resume 2)\n    perform Choose.pick ()\nprint (trial ())\n",
            "@effect\nclass Choose:\n    @control\n    def pick() -> int: ...\n@handler(Choose)\nclass choose:\n    def pick():\n        return satAdd(resume(1), resume(2))\nprint(choose(lambda: perform(Choose.pick())))\n",
        ]),
        ("abort", [
            "effect Fail { control abort stop: fn() -> int }\nfn trial() = {\nhandle Fail { stop => resume(1) }\nperform Fail.stop()\n}\nprint(trial())\n",
            "effect Fail\n    control abort stop : Unit => int\ntrial () =\n    handle Fail\n        stop => resume 1\n    perform Fail.stop ()\nprint (trial ())\n",
            "@effect\nclass Fail:\n    @control\n    @abort\n    def stop() -> int: ...\n@handler(Fail)\nclass fail:\n    def stop():\n        return resume(1)\nprint(fail(lambda: perform(Fail.stop())))\n",
        ]),
        ("static", [
            "static effect Choice { control once pick: fn() -> int }\nfn trial() = {\nhandle static Choice { pick => 3 }\nperform Choice.pick()\n}\nprint(trial())\n",
            "static effect Choice\n    control once pick : Unit => int\ntrial () =\n    handle static Choice\n        pick => 3\n    perform Choice.pick ()\nprint (trial ())\n",
            "@static_effect\nclass Choice:\n    @control\n    @once\n    def pick() -> int: ...\n@static_handler(Choice)\nclass choose:\n    def pick():\n        return 3\nprint(choose(lambda: perform(Choice.pick())))\n",
        ]),
        ("replay", [
            "effect Choice { control many pick: fn() -> int }\neffect Audit { write: fn(int) -> Unit }\nfn work() = { let n = perform Choice.pick()\nperform Audit.write(n)\nn }\nfn trial() = { handle Audit { write n => print(n) }\nhandle Choice { pick => resume(1) }\nwork() }\nprint(trial())\n",
            "effect Choice\n    control many pick : Unit => int\neffect Audit\n    write : int => Unit\nwork () =\n    n = perform Choice.pick ()\n    perform Audit.write n\n    n\ntrial () =\n    handle Audit\n        write n => print n\n    handle Choice\n        pick => resume 1\n    work ()\nprint (trial ())\n",
            "@effect\nclass Choice:\n    @control\n    @many\n    def pick() -> int: ...\n@effect\nclass Audit:\n    def write(n: int) -> None: ...\n@handler(Choice)\nclass choose:\n    def pick():\n        return resume(1)\n@handler(Audit)\nclass silent:\n    def write(n):\n        print(n)\ndef work():\n    n = perform(Choice.pick())\n    perform(Audit.write(n))\n    return n\nprint(silent(lambda: choose(work)))\n",
        ]),
    ] {
        let [default, ml, python] = sources;
        if case == "replay" {
            for valid in [
                python.replace("    def write(n: int)", "    @replayable\n    def write(n: int)"),
                python.replace("silent(lambda: choose(work))", "choose(lambda: silent(work))"),
            ] {
                let errors = staging::diagnostics(&valid, Flavor::Python);
                assert!(errors.is_empty(), "{errors}");
            }
        }
        let expected = staging::diagnostics(default, Flavor::Default);
        assert!(!expected.is_empty(), "{case}: Default accepted the invalid use");
        for (source, flavor) in [(ml, Flavor::Ml), (python, Flavor::Python)] {
            assert_eq!(staging::diagnostics(source, flavor), expected, "{case}: {flavor}");
        }
    }
}

#[test]
fn every_multiplicity_violation_is_rejected_naming_its_operation() {
    for (source, needles, why) in REJECTED {
        assert_rejected(source, Flavor::Default, needles, why);
    }
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

#[test]
fn wasm32_names_the_once_continuation_it_cannot_yet_run() {
    // [MULTI-WASM] The stack-switching proposal specifies ONE-SHOT
    // continuations, so a `once` continuation outside every `many` region is
    // what wasm32 acquires when that proposal lands. Until then the rejection
    // names the operation. `many` is never part of that limit.
    let (ok, err) = compile_for_target(ONCE_ON_WASM, "wasm32");
    assert!(!ok, "wasm32 cannot run Async.await's continuation yet");
    assert!(
        err.contains("Async.await"),
        "the rejection must name Async.await, got: {err}"
    );
}
