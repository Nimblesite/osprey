//! The compiler's own launchers must never park a program on `input()`.
//!
//! [BUILTIN-INPUT] splits the obligation in two, and this file owns the launcher
//! half. The runtime half — that a connected-but-silent writer is *waited for*,
//! that a pause mid-line does not truncate, that EOF ends a line — lives in
//! `input_descriptor_states.rs`, which pins each descriptor state against the
//! spec table.
//!
//! The launcher obligation is one sentence: "A launcher that cannot supply input
//! must close the child's standard input." Waiting on a writer that will speak is
//! correct; waiting on a descriptor nobody will ever write to is a hang, and the
//! fix belongs to whoever opened it. `osprey ... --run` hands its own stdin
//! straight to the program, so a closed stdin must reach it as a real EOF; and
//! `osprey test` runs cases it cannot answer for, so `test_cmd.rs` gives every
//! child `Stdio::null` rather than a descriptor that is neither data nor EOF.
//!
//! This is the exact shape that hung the editor: `execFile` opens a stdin pipe
//! and never ends it, so a compiler that ever reads stdin parks on a descriptor
//! with no writer, with nothing on stdout because it is block-buffered. Two of
//! the extension's three spawn sites remembered to close stdin and the third did
//! not — which is why "every caller must remember" is not a working contract, and
//! why each launcher gets an assertion here rather than a code review.
//!
//! A timeout is the only honest oracle for a hang: the defect IS unbounded
//! waiting, and no assertion over a value can observe a process that never
//! produces one.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Long enough that a loaded machine compiling and linking a program cannot trip
/// it, short enough that a red run reports in seconds rather than hanging the
/// suite it is supposed to be protecting.
const RUN_BUDGET: Duration = Duration::from_mins(2);

/// How often the poll loop re-checks a child that has not exited yet.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A committed program that calls `input()` — twice, and once inside an
/// assertion — so a launcher that leaves stdin unwritable parks on it.
fn program_that_reads_stdin() -> PathBuf {
    repo_root()
        .join("tests")
        .join("regressions")
        .join("basics")
        .join("math")
        .join("comprehensive_math.test.osp")
}

/// Outcome of waiting on a child for at most `RUN_BUDGET`. `Failed` carries a
/// spawn/poll error out to the `#[test]` body rather than panicking here: these
/// helpers are not test functions, so a panic in them is a denied restriction
/// lint — and the assertion belongs with the other assertions anyway.
enum Wait {
    Exited { success: bool, stdout: String },
    TimedOut,
    Failed { reason: String },
}

/// Wait for `child`, killing it and reporting `TimedOut` once the budget is
/// spent. `wait_with_output` cannot be used: it blocks forever on exactly the
/// case under test, so the timeout has to wrap a polling `try_wait`.
fn wait_bounded(mut child: Child) -> Wait {
    let deadline = Instant::now() + RUN_BUDGET;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                return Wait::Exited {
                    success: status.success(),
                    stdout,
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Wait::TimedOut;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(error) => {
                return Wait::Failed {
                    reason: format!("could not poll the compiler child: {error}"),
                };
            }
        }
    }
}

/// Run the compiler with `args` and the given `stdin`, bounded by `RUN_BUDGET`.
fn run_compiler(args: &[&std::ffi::OsStr], stdin: Stdio) -> Wait {
    let child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args(args)
        .current_dir(repo_root())
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(child) => wait_bounded(child),
        Err(error) => Wait::Failed {
            reason: format!("could not spawn the compiler: {error}"),
        },
    }
}

/// Printed by the program under test with whatever `input()` returned. A
/// launcher that closes stdin makes that `""` — and asserting the marker is
/// what keeps these tests from going VACUOUS: without it, a program that no
/// longer calls `input()` at all would exit 0 and pass, testing nothing.
const READ_EVIDENCE: &str = "stdin=[]";

/// Assert `outcome` finished cleanly, naming the launcher that would otherwise
/// have parked and what the program managed to print before it did.
fn assert_finished(outcome: Wait, launcher: &str) {
    // Each way of failing becomes a complaint rather than its own panic, so
    // one assertion carries all three.
    let complaint = match outcome {
        Wait::Exited {
            success: true,
            stdout,
        } if !stdout.contains(READ_EVIDENCE) => format!(
            "`{launcher}` exited cleanly, but the program never reported reading \
             stdin. Either it no longer calls `input()` — in which case this test \
             proves nothing — or the read did not return end-of-file. Printed:\n{stdout}"
        ),
        Wait::Exited { success: true, .. } => String::new(),
        Wait::Exited {
            success: false,
            stdout,
        } => format!(
            "`{launcher}` must run a program that reads stdin to completion; stdout was:\n{stdout}"
        ),
        Wait::TimedOut => format!(
            "`{launcher}` parked a program on `input()`. [BUILTIN-INPUT] requires a \
             launcher that cannot supply input to CLOSE the child's standard input, so \
             the read sees a real end-of-file instead of a descriptor that is neither \
             data nor EOF."
        ),
        Wait::Failed { reason } => reason,
    };
    assert!(complaint.is_empty(), "{complaint}");
}

#[test]
fn run_delivers_a_closed_stdin_to_the_program_as_end_of_file() {
    let program = program_that_reads_stdin();
    let outcome = run_compiler(
        &[
            program.as_os_str(),
            "--run".as_ref(),
            "--quiet".as_ref(),
            "--memory=default".as_ref(),
        ],
        Stdio::null(),
    );
    assert_finished(outcome, "osprey --run");
}

#[test]
fn the_test_runner_closes_each_case_s_stdin_even_when_its_own_is_open() {
    let program = program_that_reads_stdin();
    // `Stdio::piped()` on the RUNNER is the whole point: the write end stays
    // alive in this process, so an inherited descriptor would be neither data
    // nor EOF. `test_cmd.rs` must hand the case `Stdio::null` regardless.
    let outcome = run_compiler(
        &["test".as_ref(), program.as_os_str(), "--quiet".as_ref()],
        Stdio::piped(),
    );
    assert_finished(outcome, "osprey test");
}
