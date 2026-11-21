//! Multi-shot (`many`) continuations, end to end.
//!
//! A `many` handler may answer one request any number of times, and every
//! answer runs its own branch of the captured remainder
//! ([MULTI-HANDLE-MANY]). The compiler writes that remainder as an ordinary
//! closure, so these programs must print the same bytes under GC and ARC and
//! must build for every target, wasm32 included ([MULTI-WASM-MANY],
//! [CI-MULTISHOT-WASM], [EFFECTS-INV-MULTISHOT-PORTABLE]).
//!
//! The declaration rules of the multiplicity axis live in
//! `staged_multiplicity.rs`; the two share `common::staging`.

#[path = "common/staging.rs"]
mod staging;

use osprey_syntax::Flavor;
use staging::{assert_rejected, compile_for_target, compile_staged};

/// Every program below resumes one captured continuation more than once.
/// Each pairs its source with the exact output a correct run prints
/// ([MULTI-HANDLE-MANY], [MULTI-FALSIFY] gates 3 and 4, plan 0016 phase 3).
const MANY_RUNS: &[(&str, &str, &str)] = &[
    (
        "every alternative is resumed and each branch keeps its own answer",
        r#"
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
"#,
        "collected=a4&a9\n",
    ),
    (
        // A shared helper takes capturing callbacks with no annotation, a
        // lambda bound by `let` makes the request, work before a capture runs
        // once, and the right side of `&&` asks only when the left allows it.
        "a shared helper, a bound lambda and a lazy operator inside one search",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Trace { replayable note: fn(string) -> Unit }
fn both(first, second) = satAdd(first(), second())
fn pair(seed) = {
    let ask = fn(n) => perform Choice.pick(n)
    let left = ask(seed)
    perform Trace.note("L${left} ")
    let right = both(fn() => ask(left), fn() => 100)
    match left > 1 && perform Choice.pick(0) == 0 {
        true => "big${left}:${right}"
        false => "small${left}:${right}"
    }
}
fn main() = {
    mut log = ""
    let all = {
        handle Trace {
            note text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => {
                let a = resume(bound)
                let b = resume(satAdd(bound, 1))
                "${a}|${b}"
            }
        }
        pair(1)
    }
    print("all=${all} log=${log}")
}
"#,
        "all=small1:101|small1:102|big2:102|small2:102|big2:103|small2:103 log=L1 L2 \n",
    ),
    (
        // `Stop` is a `control` effect handled between the request and its
        // `many` handler, so it is captured with it: an arm that never resumes
        // abandons only its own branch, and `finally` runs once per branch.
        "a non-resuming control effect and a finalizer inside the captured region",
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Stop { control replayable quit: fn(string) -> int }
effect Audit { replayable close: fn(string) -> Unit }
fn work(seed) = {
    let n = perform Choice.pick(seed)
    match n > 5 {
        true => perform Stop.quit("too big ${n}")
        false => satMul(n, 10)
    }
}
fn main() = {
    mut log = ""
    let total = {
        handle Audit {
            close text => { log = "${log}${text}" }
        }
        handle Choice {
            pick bound => satAdd(satAdd(resume(bound), resume(6)), resume(2))
        }
        {
            handle Stop {
                quit _why => satSub(0, 1)
                finally => perform Audit.close("done;")
            }
            work(1)
        }
    }
    print("total=${total} log=${log}")
}
"#,
        "total=29 log=done;done;done;\n",
    ),
    (
        // A value operation of a captured effect, a return clause applied to
        // each branch, and a handler that answers without resuming at all.
        "a value operation, a return clause and zero resumptions",
        r#"
effect Ask {
    control many choose: fn(string) -> bool
    replayable hint: fn() -> string
}
fn decide() = {
    let label = perform Ask.hint()
    match perform Ask.choose(label) {
        true => "yes-${label}"
        false => "no-${label}"
    }
}
fn main() = {
    let both = {
        handle Ask {
            choose _label => "${resume(true)}+${resume(false)}"
            hint => "h"
            return answer => "<${answer}>"
        }
        decide()
    }
    let none = {
        handle Ask {
            choose _label => "skipped"
            hint => "h"
        }
        decide()
    }
    print("both=${both} none=${none}")
}
"#,
        "both=<yes-h>+<no-h> none=skipped\n",
    ),
    (
        // The handler is a value applied to the search: backtracking keeps the
        // newest version that the rest of the plan accepts.
        "a handler value backtracks to the first answer the plan accepts",
        r#"
effect Versions { control many pick: fn(string) -> string }
fn plan() = {
    let serde = perform Versions.pick("serde")
    let toml = perform Versions.pick("toml")
    match serde == "2" && toml == "2" {
        true => "conflict"
        false => "serde ${serde}, toml ${toml}"
    }
}
let newestThatWorks = handler Versions {
    pick _ => match resume("2") {
        "conflict" => resume("1")
        found => found
    }
}
print(newestThatWorks(plan))
"#,
        "serde 2, toml 1\n",
    ),
];

/// Run `source` under `memory` through the real CLI and return its stdout.
fn run_under(source: &str, memory: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "osprey_many_{}_{memory}_{}",
        std::process::id(),
        name.len()
    ));
    let Ok(()) = std::fs::create_dir_all(&dir) else {
        return "could not create temp dir".to_string();
    };
    let input = dir.join("many.osp");
    let _ = std::fs::write(&input, source);
    let ran = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(input.as_os_str())
        .args(["--run", "--quiet", "--no-warnings"])
        .arg(format!("--memory={memory}"))
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    match ran {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        Ok(out) => format!("FAILED: {}", String::from_utf8_lossy(&out.stderr)),
        Err(error) => format!("FAILED: {error}"),
    }
}

#[test]
fn many_regions_run_every_branch_under_gc_and_arc() {
    for (name, source, expected) in MANY_RUNS {
        for memory in ["gc", "arc"] {
            assert_eq!(
                run_under(source, memory, name),
                *expected,
                "{name} under {memory}"
            );
        }
    }
}

#[test]
fn a_many_region_is_closures_with_no_suspended_stack_or_thread() {
    // [EFFECTS-INV-MULTISHOT-PORTABLE] [CI-MULTISHOT-WASM] A continuation the
    // backend could resume only once is a suspended stack: the coroutine
    // runtime. A `many` region must not reach it at all, on any target.
    for (name, source, _) in MANY_RUNS {
        let ir = compile_staged(source);
        assert!(
            ir.contains("__manyBind"),
            "{name}: the region was not rewritten into closures"
        );
        for banned in ["__osprey_coro_", "pthread_"] {
            assert!(
                !ir.contains(banned),
                "{name}: a `many` region must not use `{banned}`"
            );
        }
    }
}

/// What a closure cannot express is rejected by name, never miscompiled.
const UNSPLITTABLE: &[(&str, &[&str], &str)] = &[
    (
        r#"
effect Choice { control many pick: fn(int) -> int }
fn main() = {
    let all = {
        handle Choice {
            pick _bound => satAdd(resume(1), resume(2))
        }
        foldList([1, 2], 0, fn(sum, n) => satAdd(sum, perform Choice.pick(n)))
    }
    print("${all}")
}
"#,
        &["Choice.pick", "cannot cross"],
        "a request inside a builtin's callback has no continuation to capture",
    ),
    (
        r#"
effect Choice { control many pick: fn(int) -> int }
fn ask(n) = perform Choice.pick(n)
fn main() = {
    let all = {
        handle Choice {
            pick _bound => satAdd(resume(1), resume(2))
        }
        {
            let stored = { run: ask }
            stored.run(3)
        }
    }
    print("${all}")
}
"#,
        &["`ask`", "cannot cross"],
        "a capturing function stored in a record cannot be followed",
    ),
    (
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Audit { replayable close: fn(string) -> Unit }
fn main() = {
    let all = {
        handle Choice {
            pick _bound => satAdd(resume(1), resume(2))
        }
        {
            handle Audit {
                close text => print(text)
                finally => print("${perform Choice.pick(9)}")
            }
            perform Choice.pick(3)
        }
    }
    print("${all}")
}
"#,
        &["finally", "no continuation exists"],
        "cleanup runs where a branch ends, so it cannot make a captured request",
    ),
    (
        r#"
effect Pick<t> { control many replayable any: fn(t) -> t }
fn main() = {
    let all = {
        handle Pick<int> {
            any _bound => satAdd(resume(1), resume(2))
        }
        perform Pick.any(1)
    }
    print("${all}")
}
"#,
        &["Pick", "type parameters"],
        "a captured effect needs one request layout",
    ),
];

#[test]
fn what_a_many_continuation_cannot_cross_is_rejected_by_name() {
    for (source, needles, why) in UNSPLITTABLE {
        assert_rejected(source, Flavor::Default, needles, why);
    }
}

#[test]
fn every_cross_target_accepts_every_many_region() {
    // [MULTI-WASM-MANY] [CI-MULTISHOT-WASM] wasm32 fully supports multi-shot
    // continuations, and so does every other cross target: the capability
    // gate must accept each program above. This test is the hard requirement;
    // a rejection here is a compiler defect, never a reason to skip.
    for target in ["wasm32", "macos", "ios-sim", "android-arm64"] {
        for (name, source, _) in MANY_RUNS {
            let (ok, err) = compile_for_target(source, target);
            assert!(ok, "{target} must build `{name}`, got: {err}");
        }
    }
}
