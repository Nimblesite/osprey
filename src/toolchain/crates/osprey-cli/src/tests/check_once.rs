//! One invocation checks a program once: the gate retains its solve, and no
//! target boundary may run another. [TYPE-EFFECT-PROOF-COUNT]
use super::*;
use std::process::ExitCode;

const PLAIN: &str = "fn pick(a: int, b: int) = a\nfn main() = print(pick(1, 2))\n";
const STAGED: &str = "static effect Size { width: fn() -> int }\n\
    fn pick(a: int, b: int) = a\n\
    fn main() = {\n    handle static Size {\n        width => 21\n    }\n    print(perform Size.width())\n}\n";
/// A helper handing its callback to a signal query gets a specialized copy.
const FORWARDING: &str = "static effect Count { read: fn() -> int }\n\
    fn work() = perform Count.read()\n\
    fn watch(callback, label: string) = signalRefresh({ dependencies: [\"<unknown>\"], revision: 0, value: 0 }, [\"<unknown>\"], callback).value\n\
    fn main() = {\n    handle static Count {\n        read => 7\n    }\n    print(watch(work, \"a\"))\n    print(work())\n}\n";
/// The same through generic binders, which the copy has to erase.
const GENERIC_FORWARDING: &str = "static effect Count { read: fn() -> int }\n\
    fn work() = perform Count.read()\n\
    fn watch<T>(callback: fn() -> T, seed: T) = signalRefresh({ dependencies: [\"<unknown>\"], revision: 0, value: seed }, [\"<unknown>\"], callback).value\n\
    fn main() = {\n    handle static Count {\n        read => 7\n    }\n    print(watch(work, 1))\n}\n";
/// A runtime handler beside a static one, its arguments left to inference.
const INFERRED: &str = "static effect Size { width: fn() -> int }\n\
    effect Box<T> { take: fn() -> T }\n\
    fn main() = {\n    handle static Size {\n        width => 21\n    }\n    handle Box {\n        take => perform Size.width()\n    }\n    print(perform Box<int>.take())\n}\n";
const BROWSER: &str = "fn osprey_web_dispatch(event: string) -> int = 0\nfn main() = print(1)\n";

fn script(source: &str) -> CompilationInput {
    let parsed = osprey_syntax::parse_program(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    CompilationInput::script("p.osp", source.to_string(), parsed.program)
}

fn target_cli(mode: &str, target: &str) -> Cli {
    Cli {
        memory: "arc".to_string(),
        target: target.to_string(),
        warnings: Warnings::Skip,
        ..cli("p.osp", mode, Policy::allow_all())
    }
}

/// The solves one gate needs, which is what the whole invocation may spend.
fn gate_solves(input: &CompilationInput) -> usize {
    let start = osprey_types::effect_check_count();
    let _ = osprey_types::check_and_publish(input.program(), &[]);
    osprey_types::effect_check_count() - start
}

fn invocation_solves(cli: &Cli, input: &CompilationInput) -> (usize, ExitCode) {
    let start = osprey_types::effect_check_count();
    let code = dispatch(cli, input);
    (osprey_types::effect_check_count() - start, code)
}

#[test]
fn every_target_and_mode_spends_only_the_gate_solve() {
    let host = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux-x64"
    };
    let targets = [host, "ios-sim", "android-arm64", "android-x64", "native"];
    let mut extra = Vec::new();
    for (name, source) in [("plain", PLAIN), ("staged", STAGED)] {
        let input = script(source);
        let budget = gate_solves(&input);
        assert!(budget >= 1, "{name}: the gate must check");
        for target in targets {
            for mode in ["--check", "--llvm"] {
                let (solves, code) = invocation_solves(&target_cli(mode, target), &input);
                assert_eq!(code, ExitCode::SUCCESS, "{name} {target} {mode}");
                if solves != budget {
                    extra.push(format!(
                        "{name} {target} {mode}: {solves} solves, budget {budget}"
                    ));
                }
            }
        }
    }
    assert!(extra.is_empty(), "\n{}", extra.join("\n"));
}

#[test]
fn a_plain_program_is_checked_exactly_once_on_the_host_boundary() {
    let input = script(PLAIN);
    let host = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux-x64"
    };
    for mode in ["--check", "--llvm"] {
        assert_eq!(
            invocation_solves(&target_cli(mode, host), &input).0,
            1,
            "{mode}"
        );
    }
}

/// A staged program is one program: discharging its handlers rewrites it for
/// the backend, and must not prove it again. [TYPE-EFFECT-PROOF-COUNT]
#[test]
fn a_staged_program_is_checked_exactly_once_on_every_boundary() {
    let host = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux-x64"
    };
    let mut extra = Vec::new();
    for (name, source) in [
        ("static", STAGED),
        ("forwarding", FORWARDING),
        ("generic forwarding", GENERIC_FORWARDING),
        ("inferred", INFERRED),
    ] {
        let input = script(source);
        for target in [host, "ios-sim", "android-arm64"] {
            for mode in ["--check", "--llvm"] {
                let (solves, code) = invocation_solves(&target_cli(mode, target), &input);
                assert_eq!(code, ExitCode::SUCCESS, "{name} {target} {mode}");
                if solves != 1 {
                    extra.push(format!("{name} {target} {mode}: {solves} solves"));
                }
            }
        }
    }
    assert!(extra.is_empty(), "\n{}", extra.join("\n"));
}

/// One solve must still reject what the source's own call breaks, though the
/// copy it redirects to has lost the annotations and the callback argument.
#[test]
fn a_forwarded_call_the_source_rejects_stays_rejected() {
    let wrong_seed = GENERIC_FORWARDING.replace("watch(work, 1)", "watch(work, \"no\")");
    let wrong_callback = FORWARDING.replace(
        "callback, label: string",
        "callback: fn() -> string, label: string",
    );
    for (name, source) in [("seed", wrong_seed), ("callback", wrong_callback)] {
        let input = script(&source);
        for mode in ["--check", "--llvm"] {
            let code = invocation_solves(&target_cli(mode, "macos"), &input).1;
            assert_eq!(code, ExitCode::FAILURE, "{name} {mode}");
        }
    }
}

/// Discharge cannot reach an operation behind a record field. The source proof
/// sees a handler around the call; only the residual shows nothing answers it.
#[test]
fn a_static_operation_discharge_cannot_reach_is_still_rejected() {
    let source = "static effect A { read: fn() -> int }\n\
        type Box = { call: fn() -> int }\n\
        fn read() = perform A.read()\n\
        fn boxed() = Box { call: read }\n\
        fn main() = {\n    let box = boxed()\n    handle static A {\n        read => 2\n    }\n    print(box.call())\n}\n";
    let input = script(source);
    for mode in ["--check", "--llvm"] {
        let code = invocation_solves(&target_cli(mode, "macos"), &input).1;
        assert_eq!(code, ExitCode::FAILURE, "{mode}");
    }
}

#[test]
fn a_browser_boundary_spends_only_the_gate_solve() {
    let input = script(BROWSER);
    let budget = gate_solves(&input);
    for mode in ["--check", "--llvm"] {
        let (solves, code) = invocation_solves(&target_cli(mode, "wasm32"), &input);
        assert_eq!(code, ExitCode::SUCCESS, "{mode}");
        assert_eq!(solves, budget, "{mode}");
    }
}

/// Reusing the solve must not lose the export's own effect proof.
#[test]
fn an_export_with_an_unhandled_effect_still_fails_with_one_solve_budget() {
    let source = "effect Alarm { ring: fn() -> int }\n\
        fn exposed() = perform Alarm.ring()\n\
        fn main() = {\n    handle Alarm {\n        ring => 1\n    }\n    print(exposed())\n}\n";
    let input = script(source);
    let budget = gate_solves(&input);
    for target in ["ios-sim", "android-arm64"] {
        for mode in ["--check", "--llvm"] {
            let (solves, code) = invocation_solves(&target_cli(mode, target), &input);
            assert_eq!(code, ExitCode::FAILURE, "{target} {mode}");
            assert_eq!(solves, budget, "{target} {mode}");
        }
    }
}
