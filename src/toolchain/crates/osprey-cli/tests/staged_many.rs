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

#[path = "common/many_permutations.rs"]
mod many_permutations;
#[path = "common/many_programs.rs"]
mod many_programs;
#[path = "common/staging.rs"]
mod staging;

use many_permutations::MANY_PERMUTATIONS;
use many_programs::MANY_RUNS;
use osprey_syntax::Flavor;
use staging::{assert_rejected, compile_for_target, compile_for_target_with, compile_staged};

/// Every multi-shot program: the base runs, then their permutations.
fn every_run() -> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    MANY_RUNS.iter().chain(MANY_PERMUTATIONS)
}

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
    for (name, source, expected) in every_run() {
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
    for (name, source, _) in every_run() {
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
        range(0, 2) |> fold(0, fn(sum, n) => satAdd(sum, perform Choice.pick(n)))
    }
    print("${all}")
}
"#,
        &["Choice.pick", "cannot cross"],
        "a request inside an iterator stage's callback has no continuation to capture",
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
    (
        // [MULTI-REPLAY-CHECK] [MULTI-REPLAY-COARSE] A channel operation that
        // sits in the captured remainder after a request is replayed by every
        // resumption: the message is duplicated (or the second branch waits on
        // a channel whose one message the first took). The channel use AFTER
        // the region stays legal — only the replayed remainder is rejected.
        r#"
effect Choice { control many pick: fn(int) -> string }
fn main() = {
    let box = Channel(4)
    let out = {
        handle Choice {
            pick _b => "${resume("1")}|${resume("2")}"
        }
        {
            let c = perform Choice.pick(0)
            let _sent = send(box, c)
            c
        }
    }
    let drained = select { v = recv(box) => v timeout(50) => "empty" }
    print("out=${out} drained=${drained}")
}
"#,
        &["`send`", "replays"],
        "a channel operation inside the captured remainder is replayed per branch and must be rejected",
    ),
    (
        // [MULTI-REPLAY-CHECK] A fiber spawned and awaited after the request
        // would start once per branch: both are named.
        r#"
effect Choice { control many pick: fn(int) -> int }
fn work() = 4
fn main() = {
    let all = {
        handle Choice {
            pick _b => satAdd(resume(1), resume(2))
        }
        {
            let c = perform Choice.pick(0)
            let f = spawn work()
            satAdd(c, await(f))
        }
    }
    print("${all}")
}
"#,
        &["`spawn`", "`await`", "replays"],
        "a fiber started in the replayed remainder would start once per branch",
    ),
    (
        // [MULTI-REPLAY-CHECK] A recv and a select after the request would
        // take one message per branch.
        r#"
effect Choice { control many pick: fn(int) -> int }
fn main() = {
    let box = Channel(4)
    let all = {
        handle Choice {
            pick _b => satAdd(resume(1), resume(2))
        }
        {
            let c = perform Choice.pick(0)
            let _taken = recv(box)
            let _either = select { v = recv(box) => v timeout(5) => 0 }
            c
        }
    }
    print("${all}")
}
"#,
        &["`recv`", "`select`", "replays"],
        "a receive in the replayed remainder would take one message per branch",
    ),
    (
        // [MULTI-REPLAY-CHECK] A mask in the replayed remainder would skip a
        // handler once per branch.
        r#"
effect Choice { control many pick: fn(int) -> int }
effect Store { replayable get: fn(string) -> int }
fn main() = {
    let all = {
        handle Store { get _k => 5 }
        {
            handle Store { get _k => 6 }
            handle Choice {
                pick _b => satAdd(resume(1), resume(2))
            }
            {
                let c = perform Choice.pick(0)
                satAdd(c, mask Store { perform Store.get("k") })
            }
        }
    }
    print("${all}")
}
"#,
        &["`mask`", "replays"],
        "a mask in the replayed remainder is repeated per branch",
    ),
    (
        // A request inside a mask body has no continuation the region can
        // capture: the mask is a boundary the closure cannot span.
        r#"
effect Choice { control many pick: fn(int) -> int }
fn main() = {
    let all = {
        handle Choice {
            pick _b => satAdd(resume(1), resume(2))
        }
        {
            handle Choice { pick n => resume(n) }
            mask Choice { perform Choice.pick(1) }
        }
    }
    print("${all}")
}
"#,
        &["cannot span a fiber, channel or mask operation"],
        "a captured request inside a mask has no continuation to capture",
    ),
    (
        // A capturing function reached through a local alias cannot be followed.
        r#"
effect Choice { control many pick: fn(int) -> int }
fn ask(n) = perform Choice.pick(n)
fn main() = {
    let all = {
        handle Choice {
            pick _b => satAdd(resume(1), resume(2))
        }
        {
            let f = ask
            f(2)
        }
    }
    print("${all}")
}
"#,
        &["`ask`", "cannot cross"],
        "a capturing function called through an alias cannot be followed",
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
    // An app-library target builds the program as an app (`--entry-only`);
    // as a full library every scalar function is a C export, and one that
    // performs an unhandled effect is refused by the export contract (#281),
    // never by the many-region gate.
    for (name, source, _) in every_run() {
        let (ok, err) = compile_for_target(source, "wasm32");
        assert!(ok, "wasm32 must build `{name}`, got: {err}");
        for target in ["macos", "ios-sim", "android-arm64"] {
            let (ok, err) = compile_for_target_with(source, target, &["--entry-only"]);
            assert!(ok, "{target} must build the app `{name}`, got: {err}");
            let (ok, err) = compile_for_target(source, target);
            assert!(
                ok || (err.contains("at library export") && err.contains("add a matching `handle`")),
                "{target} library build of `{name}` may refuse only an effectful export, got: {err}"
            );
        }
    }
}

#[test]
fn the_compiler_special_cases_no_effect_operation_or_use_case() {
    // [EFFECTS-INV-MULTISHOT-GENERAL] One mechanism, keyed on the declared
    // multiplicity alone. The lowering's source may spell no effect or
    // operation name: every name in its output comes from the program.
    let lowering = concat!(env!("CARGO_MANIFEST_DIR"), "/../osprey-types/src/many");
    let Ok(entries) = std::fs::read_dir(lowering) else {
        panic!("the lowering lives in {lowering}");
    };
    let named: Vec<&str> = every_run()
        .flat_map(|(_, source, _)| source.lines())
        .filter_map(|line| line.trim().strip_prefix("effect "))
        .filter_map(|rest| rest.split([' ', '{']).next())
        .collect();
    assert!(
        named.len() >= 6,
        "the cases declare several unrelated effects"
    );
    for entry in entries.flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let identifiers: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .collect();
        for name in &named {
            assert!(
                !identifiers.contains(name),
                "{:?} names the effect `{name}`: the compiler may know no effect",
                entry.path()
            );
        }
        let lowered = text.to_lowercase();
        for use_case in ["retry", "backtrack", "paginat", "authoriz"] {
            assert!(
                !lowered.contains(use_case),
                "{:?} mentions `{use_case}`: the compiler may know no use case",
                entry.path()
            );
        }
    }
}
