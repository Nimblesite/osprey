//! `input()` must never block a program's own termination ([BUILTIN-INPUT]).
//!
//! The spec is unambiguous: input "returns the empty string `""` rather than
//! blocking or failing" when stdin is "empty or not connected". The C runtime
//! disagrees — `osp_input` is a bare `getchar()` loop, so an fd that is open
//! but silent (a pipe an editor holds and never writes, an idle terminal) is
//! neither EOF nor data and the read never returns.
//!
//! That is not a hypothetical. `tests/regressions/basics/math/comprehensive_math`
//! calls `input()` twice, and the `VSCode` "Compile and Run" command spawns the
//! compiler through `execFile`, which opens a stdin pipe and never ends it.
//! The command hangs forever with no output, because stdout is block-buffered
//! and nothing is flushed before the read parks. Two of the extension's three
//! spawn sites remember to close stdin; the third does not, which is precisely
//! why "every caller must remember" is not a working contract.
//!
//! A timeout is the only honest oracle here: the defect IS unbounded waiting,
//! and no assertion over a value can observe a process that never returns one.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Long enough that a loaded machine compiling and linking a program cannot
/// trip it, short enough that a red run reports in seconds rather than hanging
/// the suite it is supposed to be protecting.
const RUN_BUDGET: Duration = Duration::from_secs(30);

/// How often the poll loop re-checks a child that has not exited yet.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
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

/// Run `program` through `--run` with stdin held OPEN and silent — the state an
/// editor's `execFile` leaves it in. `Stdio::piped()` is the whole point: the
/// write end stays alive in this process, so the child sees neither data nor
/// EOF, which is the condition [BUILTIN-INPUT] says must still yield `""`.
fn run_with_open_stdin(program: &Path) -> Wait {
    let child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(program)
        .arg("--run")
        .arg("--quiet")
        .current_dir(repo_root())
        .stdin(Stdio::piped())
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

#[test]
fn input_does_not_block_when_stdin_is_open_and_silent() {
    let program = repo_root()
        .join("tests")
        .join("regressions")
        .join("basics")
        .join("math")
        .join("comprehensive_math.test.osp");
    match run_with_open_stdin(&program) {
        Wait::Exited { success, stdout } => {
            assert!(
                success,
                "the program must pass with an open, silent stdin; stdout was:\n{stdout}"
            );
        }
        Wait::TimedOut => panic!(
            "`input()` blocked forever on an open, silent stdin. \
             [BUILTIN-INPUT] requires it to return \"\" instead of blocking, \
             and a test that never terminates cannot be run from an editor."
        ),
        Wait::Failed { reason } => panic!("{reason}"),
    }
}
