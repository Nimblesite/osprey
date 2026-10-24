//! Native replay permuted against [SYMEX-REPLAY]
//! (docs/specs/tooling/SymbolicExecution.md): every exit byte, every escaping
//! file path, every fatal signal, chosen exit statuses, nested files, raw
//! stdin, and a fresh scratch directory per finding.

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::findings::{Class, Replay};
use super::replay;
use super::spec_findings_tests::{sample, Dir};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

/// Every exit byte is the program's own choice and stays klee-only, 128 and
/// above included; every signal reproduces, under its name or its number.
#[cfg(unix)]
#[test]
fn every_exit_byte_maps_to_one_verdict() {
    for code in 0..=u8::MAX {
        assert_eq!(
            replay::outcome(replay::raw_status(i32::from(code), 0)),
            Replay::KleeOnly,
            "exit {code}"
        );
    }
    for signal in 1..=31 {
        match replay::outcome(replay::raw_status(0, signal)) {
            Replay::Reproduced(name) => assert!(
                name.starts_with("SIG") || name == format!("signal {signal}"),
                "{signal}: {name}"
            ),
            other => panic!("signal {signal} gave {other:?}"),
        }
    }
}

/// A recorded file outside the scratch directory is refused before anything
/// runs: the executable here does not exist, so reaching it would be an error.
#[test]
fn every_escaping_file_path_is_refused_before_the_program_runs() {
    let mut escapes = vec![
        "../x",
        "a/../../x",
        "./../x",
        "a/../b",
        "..",
        "../../../../etc/passwd",
    ];
    if cfg!(unix) {
        escapes.extend(["/tmp/x", "/etc/passwd"]);
    }
    if cfg!(windows) {
        escapes.extend(["C:\\x", "C:/Windows/x", "\\\\server\\share\\x"]);
    }
    let dir = Dir::new("escape");
    let workdir = dir.0.join("work");
    for path in escapes {
        let mut finding = sample(Class::Memory, None, None, "m");
        finding.files = vec![
            ("ok.txt".into(), b"fine".to_vec()),
            (path.into(), b"bad".to_vec()),
        ];
        let verdict = replay::run_one(Path::new("/nonexistent/program"), &workdir, &finding)
            .expect("refused, not run");
        assert!(
            matches!(&verdict, Replay::Skipped(reason) if reason.contains(&format!("file {path} is outside"))),
            "{path}: {verdict:?}"
        );
    }
}

/// Unmodelled findings only exist under KLEE and are skipped, never run —
/// including when every finding is unmodelled. Pins a defect: when nothing is
/// replayable `replay` returns early and leaves them `not-run`, so the report
/// disagrees with a mixed run, which marks the same finding skipped.
#[test]
fn unmodelled_findings_are_skipped_even_when_nothing_else_is_replayed() {
    let source = "print(\"hi\")\n";
    let parsed = osprey_syntax::parse_program(source);
    let input =
        crate::project::CompilationInput::script("hunt.osp", source.to_string(), parsed.program);
    let mut findings = vec![sample(
        Class::Unmodelled,
        Some(("hunt.osp", 1)),
        None,
        "unmodelled: fork",
    )];
    replay::replay(&input, &mut findings).expect("nothing to build");
    assert!(
        matches!(&findings[0].replay, Replay::Skipped(reason) if reason.contains("unmodelled")),
        "{:?}",
        findings[0].replay
    );
}

#[cfg(unix)]
fn script(dir: &Dir, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.0.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// The verdict of replaying a memory finding against `exe` in `dir`.
#[cfg(unix)]
fn verdict_of(dir: &Dir, exe: &Path) -> Replay {
    replay::run_one(
        exe,
        &dir.0.join("work"),
        &sample(Class::Memory, None, None, "m"),
    )
    .expect("replay")
}

/// Each fatal signal the native program dies of is reported by name.
#[cfg(unix)]
#[test]
fn every_fatal_signal_reproduces_by_name() {
    let dir = Dir::new("signals");
    for signal in ["SEGV", "ABRT", "FPE", "ILL", "TRAP", "BUS"] {
        let exe = script(&dir, signal, &format!("kill -{signal} $$"));
        let verdict = verdict_of(&dir, &exe);
        assert_eq!(
            verdict,
            Replay::Reproduced(format!("SIG{signal}")),
            "{signal}"
        );
    }
}

#[cfg(unix)]
fn assert_klee_only_exits(name: &str, statuses: &[u8]) {
    let dir = Dir::new(name);
    for status in statuses {
        let exe = script(&dir, &format!("exit{status}"), &format!("exit {status}"));
        assert_eq!(verdict_of(&dir, &exe), Replay::KleeOnly, "exit {status}");
    }
}

/// A clean exit with any status the program chose is `klee-only`.
#[cfg(unix)]
#[test]
fn every_chosen_exit_status_up_to_128_is_klee_only() {
    assert_klee_only_exits("statuses-low", &[0, 1, 2, 3, 42, 127, 128]);
}

/// Pins a defect: "a clean exit — including a non-zero `main` the program
/// chose itself — is `klee-only`", but `child_exit_code` folds a signal into
/// `128 + n`, so a program that exits 139 is reported as a reproduced SIGSEGV.
#[cfg(unix)]
#[test]
fn a_chosen_exit_status_above_128_is_still_klee_only() {
    assert_klee_only_exits("statuses-high", &[129, 134, 139, 255]);
}

/// Nested recorded files land at their relative paths and stdin carries
/// every byte, including NUL and the high half.
#[cfg(unix)]
#[test]
fn nested_files_and_raw_stdin_reach_the_native_program() {
    let dir = Dir::new("nested");
    let exe = script(
        &dir,
        "check",
        "[ \"$(cat deep/er/a.txt)\" = deep ] && [ \"$(od -An -tx1 | tr -d ' \\n')\" = 00ff0a41 ] && kill -SEGV $$\nexit 0",
    );
    let mut finding = sample(Class::Memory, None, None, "m");
    finding.files = vec![
        ("deep/er/a.txt".into(), b"deep".to_vec()),
        ("top.txt".into(), Vec::new()),
    ];
    finding.stdin = vec![0, 0xff, b'\n', b'A'];
    let work = dir.0.join("work");
    assert_eq!(
        replay::run_one(&exe, &work, &finding).expect("replay"),
        Replay::Reproduced("SIGSEGV".into())
    );
    assert!(
        work.join("top.txt").exists(),
        "an empty recorded file is still created"
    );
    finding.stdin = vec![0, 0xff, b'\n', b'B'];
    assert_eq!(
        replay::run_one(&exe, &work, &finding).expect("replay"),
        Replay::KleeOnly
    );
}

/// Each replay starts from an empty scratch directory: a file one finding
/// wrote is gone for the next.
#[cfg(unix)]
#[test]
fn each_replay_starts_from_a_clean_directory() {
    let dir = Dir::new("fresh");
    let exe = script(&dir, "probe", "[ -e stale.txt ] && kill -SEGV $$\nexit 0");
    let work = dir.0.join("work");
    let mut first = sample(Class::Memory, None, None, "m");
    first.files = vec![("stale.txt".into(), b"x".to_vec())];
    assert_eq!(
        replay::run_one(&exe, &work, &first).expect("replay"),
        Replay::Reproduced("SIGSEGV".into())
    );
    let second = sample(Class::Memory, None, None, "m");
    assert_eq!(
        replay::run_one(&exe, &work, &second).expect("replay"),
        Replay::KleeOnly
    );
}

/// A program that exits without reading stdin is not a replay failure.
#[cfg(unix)]
#[test]
fn a_program_that_ignores_a_large_stdin_still_replays() {
    let dir = Dir::new("ignores");
    let exe = script(&dir, "quick", "exit 0");
    let mut finding = sample(Class::Memory, None, None, "m");
    finding.stdin = vec![b'x'; 1 << 20];
    assert_eq!(
        replay::run_one(&exe, &dir.0.join("work"), &finding).expect("replay"),
        Replay::KleeOnly
    );
}
