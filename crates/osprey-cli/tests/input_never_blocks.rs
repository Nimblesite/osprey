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
    Exited {
        success: bool,
        stdout: String,
        stderr: String,
    },
    TimedOut,
    Failed {
        reason: String,
    },
}

/// Read whatever pipe a child was given, or nothing when it was closed.
fn drain(pipe: Option<impl Read>) -> String {
    let mut text = String::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_string(&mut text);
    }
    text
}

/// Wait for `child`, killing it and reporting `TimedOut` once the budget is
/// spent. `wait_with_output` cannot be used: it blocks forever on exactly the
/// case under test, so the timeout has to wrap a polling `try_wait`.
fn wait_bounded(mut child: Child) -> Wait {
    let deadline = Instant::now() + RUN_BUDGET;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Wait::Exited {
                    success: status.success(),
                    stdout: drain(child.stdout.take()),
                    stderr: drain(child.stderr.take()),
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
            ..
        } if !stdout.contains(READ_EVIDENCE) => format!(
            "`{launcher}` exited cleanly, but the program never reported reading \
             stdin. Either it no longer calls `input()` — in which case this test \
             proves nothing — or the read did not return end-of-file. Printed:\n{stdout}"
        ),
        Wait::Exited { success: true, .. } => String::new(),
        Wait::Exited {
            success: false,
            stdout,
            ..
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
            "--memory=gc".as_ref(),
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

// [TESTING-CLI-RUN]: a case may launch a compiler of its own, so `osprey test`
// tells every child which compiler runs it and which backend it selected. A
// stale inherited value is the same defect class as an unclosed descriptor: the
// case silently measures a program nobody just built.

/// What a suite child is told about the compiler that launched it.
const TEST_COMPILER_ENV: &str = "OSPREY_TEST_COMPILER";
const TEST_MEMORY_ENV: &str = "OSPREY_TEST_MEMORY";

/// Values an earlier process blessed. Neither names a working compiler, so a
/// child that merely inherited its environment reports them.
const STALE_COMPILER: &str = "/stale/parent/osprey";
const STALE_MEMORY: &str = "stale-backend";

/// Flags `osprey test` receives, and the backend its children must report. A
/// suite run without a flag reports the native default, GC.
const CONTEXT_CASES: &[(&[&str], &str)] = &[
    (&["--memory=arc"], "arc"),
    (&["--memory=gc"], "gc"),
    (&[], "gc"),
];

/// The case written into a scratch directory. It records its context the way any
/// Osprey program would — `spawnProcess` redirects both variables into a fixture,
/// then `readFile` reads that fixture back — so a value reaches the evidence only
/// by reaching the child's environment. `quiet` is the shipped callback idiom.
const CONTEXT_PROGRAM: &str = r#"
fn quiet(_pid, _kind, text) = {
    let _dropped = text
}

fn contextCase() = {
    let fixture = "FIXTURE_PATH"
    let spawned = spawnProcess("(printenv OSPREY_TEST_COMPILER; printenv OSPREY_TEST_MEMORY) > '" + fixture + "'", quiet)
    let finished = match spawned {
        Success { value } => {
            let code = awaitProcess(value)
            cleanupProcess(value)
            code
        }
        Error => -1
    }
    let found = lines(readFile(fixture) ?: "")
    checkAll("the suite child records both variables", [
        finished == 0,
        length(found) == 2
    ])
    let compiler = listGet(found, 0) ?: ""
    let memory = listGet(found, 1) ?: ""
    print("PROBE_COMPILER=${compiler}\nPROBE_MEMORY=${memory}")
}

test("the suite child records both variables", contextCase)
"#;

/// A scratch directory that removes itself, so a red assertion leaves nothing.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch_dir(label: &str) -> Result<Scratch, String> {
    let dir = std::env::temp_dir().join(format!(
        "osprey-test-context-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    Ok(Scratch(dir))
}

/// Run `osprey test <program>` under a parent that blessed stale context.
fn run_test_runner(program: &Path, backend: &[&str]) -> Result<Wait, String> {
    let child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg("test")
        .arg(program)
        .arg("--quiet")
        .args(backend)
        .env(TEST_COMPILER_ENV, STALE_COMPILER)
        .env(TEST_MEMORY_ENV, STALE_MEMORY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    match child {
        Ok(child) => Ok(wait_bounded(child)),
        Err(error) => Err(format!("could not spawn `osprey test`: {error}")),
    }
}

/// The compiler and backend a case recorded, or which one it never printed.
fn recorded(stdout: &str) -> Result<(&str, &str), String> {
    let after = |name| stdout.lines().find_map(|line| line.strip_prefix(name));
    match (after("PROBE_COMPILER="), after("PROBE_MEMORY=")) {
        (Some(compiler), Some(backend)) => Ok((compiler, backend)),
        (None, _) => Err(String::from("the suite case never reported its compiler.")),
        (_, None) => Err(String::from("the suite case never reported its backend.")),
    }
}

/// The recorded compiler must be this executable, not the parent's stale path.
fn verify_compiler(recorded: &str) -> Result<(), String> {
    if recorded == STALE_COMPILER {
        return Err(format!(
            "the suite child inherited the stale compiler `{recorded}`."
        ));
    }
    let named = std::fs::canonicalize(recorded)
        .map_err(|error| format!("the suite child reported `{recorded}`, not a file: {error}"))?;
    let running = std::fs::canonicalize(env!("CARGO_BIN_EXE_osprey"))
        .map_err(|error| format!("could not resolve the running compiler: {error}"))?;
    if named != running {
        return Err(format!(
            "the suite child reported `{recorded}`, but this compiler is {}.",
            running.display()
        ));
    }
    Ok(())
}

/// The recorded backend must be the one `osprey test` selected.
fn verify_backend(recorded: &str, selected: &str) -> Result<(), String> {
    if recorded == selected {
        return Ok(());
    }
    Err(format!(
        "`osprey test` selected {selected} but exposed `{recorded}`."
    ))
}

/// Compare what a suite case recorded against the compiler that ran it.
fn verify_context(memory: &str, success: bool, stdout: &str, stderr: &str) -> Result<(), String> {
    let complaint =
        |reason: String| format!("{reason}\n`osprey test` stdout:\n{stdout}\nstderr:\n{stderr}");
    if !success {
        return Err(complaint(String::from(
            "a suite case failed while recording its context.",
        )));
    }
    let (compiler, backend) = recorded(stdout).map_err(complaint)?;
    verify_compiler(compiler).map_err(complaint)?;
    verify_backend(backend, memory).map_err(complaint)
}

/// Prove, for one backend, that a suite case saw this compiler and that backend.
fn probe_context(memory: &str, backend: &[&str]) -> Result<(), String> {
    let scratch = scratch_dir(memory)?;
    let program = scratch.0.join("context.test.osp");
    let fixture = scratch.0.join("context.txt");
    let source = CONTEXT_PROGRAM.replace("FIXTURE_PATH", &fixture.to_string_lossy());
    std::fs::write(&program, source)
        .map_err(|error| format!("could not write the case: {error}"))?;
    match run_test_runner(&program, backend)? {
        Wait::Exited {
            success,
            stdout,
            stderr,
        } => verify_context(memory, success, &stdout, &stderr),
        Wait::TimedOut => Err(format!(
            "`osprey test {memory} --quiet` did not finish within {RUN_BUDGET:?}."
        )),
        Wait::Failed { reason } => Err(reason),
    }
}

#[cfg(unix)]
#[test]
fn a_suite_child_exposes_its_own_compiler_and_backend_to_nested_compilers() {
    for (backend, memory) in CONTEXT_CASES {
        assert_eq!(
            probe_context(memory, backend),
            Ok(()),
            "every suite child must see this compiler and the backend `osprey test` selected"
        );
    }
}
