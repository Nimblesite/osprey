//! One invocation checks a program once: the gate retains its solve, and no
//! target boundary may run another. [TYPE-EFFECT-PROOF-COUNT]
use super::*;
use std::process::ExitCode;

const PLAIN: &str = "fn pick(a: int, b: int) = a\nfn main() = print(pick(1, 2))\n";
const STAGED: &str = "static effect Size { width: fn() -> int }\n\
    fn pick(a: int, b: int) = a\n\
    fn main() = {\n    handle static Size {\n        width => 21\n    }\n    print(perform Size.width())\n}\n";
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
