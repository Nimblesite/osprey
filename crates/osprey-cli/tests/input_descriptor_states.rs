//! Every state standard input can be in, and what [BUILTIN-INPUT] says `input()`
//! must answer for it.
//!
//! There are four states and four different right answers, and this compiler has
//! shipped the wrong one for two of them:
//!
//! * **closed, or never connected** — `""`, promptly. A launcher that cannot
//!   supply input closes the descriptor, and closed is what EOF means.
//! * **a line, then EOF** — the line. "End-of-file ends a line just as a newline
//!   does", so a final line with no terminator is still a line.
//! * **connected but silent** — *wait*. "Elapsed silence is not end-of-file: a
//!   producer that is connected but has not written yet is waited for." A
//!   revision on this branch answered `""` once a wall-clock grace expired,
//!   which handed a slow writer's line to the void and let the program report
//!   success on input it never read. Silently-wrong output, by construction.
//! * **a pause mid-line** — the whole line, joined across the pause. Same defect,
//!   worse symptom: a truncated line parses as a *different* value rather than
//!   an absent one.
//!
//! Two things make these assertions mean what they say. Each case runs a
//! **pre-compiled** binary instead of `--run`, so a slow compile on a loaded
//! machine can never be mistaken for a parked read. And the waiting cases first
//! synchronise on the program's own readiness marker: `osp_input` flushes stdout
//! on its way into the read, so seeing that marker is proof of *position* — the
//! program is inside `input()` — not merely proof that it is still alive.
//!
//! The waiting case also asserts the other half, which is what keeps a wait from
//! being a hang: once the writer closes, the program must finish. An editor that
//! holds a child's stdin open forever is the non-conforming launcher, and both
//! of ours now close it (`vscode-extension/client/src/extension.ts`, and
//! `Stdio::null` in `crates/osprey-cli/src/test_cmd.rs`).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Long enough that a loaded machine cannot trip it, short enough that a red run
/// reports in seconds rather than hanging the suite it is meant to protect.
const RUN_BUDGET: Duration = Duration::from_secs(30);

/// Compiling and linking is slower than running, and only happens once.
const COMPILE_BUDGET: Duration = Duration::from_mins(3);

/// How often a poll loop re-checks a child that has not moved yet.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long a writer stays silent before speaking. This has to exceed any grace
/// a future revision might reintroduce — the one this file buried was one
/// second — while still keeping the suite quick.
const SILENT_GAP: Duration = Duration::from_secs(3);

/// How long a parked child is watched before concluding it is genuinely waiting
/// rather than merely slow. The readiness marker has already been seen by then,
/// so the program is known to be inside the read.
const PARKED_OBSERVATION: Duration = Duration::from_secs(3);

/// Printed before the read. `osp_input` flushes stdout on its way into the
/// descriptor, so this line surfacing is the signal that the program is parked
/// in `input()` and not somewhere earlier.
const READY_MARKER: &str = "ready";

/// Echoes one line of stdin inside a delimiter, so a delivered line, a truncated
/// line and an absent line are three visibly different results rather than one
/// ambiguous empty string.
const ECHO_SOURCE: &str = "print(\"ready\")\nprint(\"got[${input()}]\")\n";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The echo program, compiled once for the whole test binary.
///
/// Returns the error as a value rather than panicking: this is not a `#[test]`
/// function, and the assertion belongs with the other assertions.
/// The compiled program AND how long compiling it actually took. The duration
/// is recorded here, at the one place the build happens, because the budget
/// assertion below cannot measure it from outside: whichever test touches the
/// `OnceLock` first pays the compile and every later caller gets a cached
/// lookup, so a test that timed its own call would measure nothing at all
/// whenever it lost that race — and pass.
fn echo_build() -> Result<(PathBuf, Duration), String> {
    static BUILT: OnceLock<Result<(PathBuf, Duration), String>> = OnceLock::new();
    BUILT.get_or_init(build_echo_binary).clone()
}

fn echo_binary() -> Result<PathBuf, String> {
    echo_build().map(|(path, _)| path)
}

fn build_echo_binary() -> Result<(PathBuf, Duration), String> {
    let started = Instant::now();
    compile_echo_binary().map(|path| (path, started.elapsed()))
}

fn compile_echo_binary() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("osprey_input_states_{}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let source = dir.join("echo.osp");
    std::fs::write(&source, ECHO_SOURCE)
        .map_err(|e| format!("could not write {}: {e}", source.display()))?;
    let out = dir.join("echo");
    let compile = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&source)
        .args(["--compile", "--quiet", "-o"])
        .arg(&out)
        .current_dir(repo_root())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not spawn the compiler: {e}"))?;
    compile_result(&compile, out)
}

fn compile_result(compile: &std::process::Output, out: PathBuf) -> Result<PathBuf, String> {
    if compile.status.success() {
        return Ok(out);
    }
    Err(format!(
        "compiling the echo program failed:\n{}{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    ))
}

/// A running echo program, with its stdout draining into `seen` on a helper
/// thread so the test can watch output arrive without blocking on EOF — which,
/// for the parked case, never comes.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    seen: Arc<Mutex<String>>,
}

impl Running {
    /// Spawn the echo program. `stdin` is piped when `feedable`, so the write end
    /// stays alive in this process and the child sees neither data nor EOF; when
    /// not, it is closed outright and the child must see a real EOF.
    fn start(feedable: bool) -> Result<Self, String> {
        let program = echo_binary()?;
        let mut child = Command::new(&program)
            .current_dir(repo_root())
            .stdin(if feedable {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not spawn {}: {e}", program.display()))?;
        let seen = match child.stdout.take() {
            Some(pipe) => drain(pipe),
            None => return Err("the child was spawned without a stdout pipe".to_owned()),
        };
        let stdin = child.stdin.take();
        Ok(Self { child, stdin, seen })
    }

    fn output(&self) -> String {
        match self.seen.lock() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Block until the readiness marker surfaces, proving the child is inside
    /// `input()`. `false` means it never got there within the budget.
    fn parked_in_input(&self) -> bool {
        let deadline = Instant::now() + RUN_BUDGET;
        while Instant::now() < deadline {
            if self.output().contains(READY_MARKER) {
                return true;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        false
    }

    fn write(&mut self, bytes: &str) -> Result<(), String> {
        let pipe = self
            .stdin
            .as_mut()
            .ok_or_else(|| "stdin was already closed".to_owned())?;
        pipe.write_all(bytes.as_bytes())
            .and_then(|()| pipe.flush())
            .map_err(|e| format!("could not write to the child's stdin: {e}"))
    }

    /// Drop the write end. This is the conforming launcher's obligation, and the
    /// only thing that turns a wait into an end-of-file.
    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// `Some(true)` on a clean exit, `Some(false)` on a failing one, `None` if the
    /// child was still running when `budget` ran out.
    fn exited_within(&mut self, budget: Duration) -> Option<bool> {
        let deadline = Instant::now() + budget;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status.success()),
                Ok(None) if Instant::now() >= deadline => return None,
                Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(_) => return None,
            }
        }
    }
}

impl Drop for Running {
    /// A parked child outlives its test by design; nothing may outlive the suite.
    fn drop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Copy `pipe` into a shared string as it arrives. A dedicated thread is what
/// makes partial output readable: `read_to_string` would only return at EOF, and
/// the whole point of the parked case is that EOF has not happened.
fn drain(mut pipe: ChildStdout) -> Arc<Mutex<String>> {
    let seen = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&seen);
    // The handle is deliberately dropped: the thread ends when the pipe closes,
    // and nothing here may join on a pipe that the parked case never closes.
    drop(std::thread::spawn(move || {
        let mut chunk = [0_u8; 256];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                return;
            }
            if let Ok(mut held) = sink.lock() {
                held.push_str(&String::from_utf8_lossy(
                    chunk.get(..read).unwrap_or_default(),
                ));
            }
        }
    }));
    seen
}

/// Run the echo program against a writer that performs `feed`, and return what it
/// printed. Every feeding case shares this: park, feed, close, wait, report.
fn echo_after(feed: impl FnOnce(&mut Running) -> Result<(), String>) -> Result<String, String> {
    let mut run = Running::start(true)?;
    if !run.parked_in_input() {
        return Err(format!(
            "the program never reached `input()`; it printed:\n{}",
            run.output()
        ));
    }
    feed(&mut run)?;
    run.close_stdin();
    match run.exited_within(RUN_BUDGET) {
        Some(true) => Ok(run.output()),
        Some(false) => Err(format!("the program failed; it printed:\n{}", run.output())),
        None => Err(format!(
            "the program never finished after its writer closed stdin; it printed:\n{}",
            run.output()
        )),
    }
}

#[test]
fn a_closed_descriptor_reads_as_end_of_file() {
    let mut run = match Running::start(false) {
        Ok(run) => run,
        Err(reason) => panic!("{reason}"),
    };
    let exited = run.exited_within(RUN_BUDGET);
    let printed = run.output();
    assert_eq!(
        exited,
        Some(true),
        "[BUILTIN-INPUT]: a closed stdin is end-of-file and must be answered at \
         once, not waited on. The program printed:\n{printed}"
    );
    assert!(
        printed.contains("got[]"),
        "[BUILTIN-INPUT]: end-of-file with nothing read is `\"\"`. Printed:\n{printed}"
    );
}

#[test]
fn a_line_followed_by_end_of_file_is_delivered_whole() {
    match echo_after(|run| run.write("hello\n")) {
        Ok(printed) => assert!(
            printed.contains("got[hello]"),
            "[BUILTIN-INPUT]: the line must arrive without its newline. Printed:\n{printed}"
        ),
        Err(reason) => panic!("{reason}"),
    }
}

#[test]
fn end_of_file_ends_a_line_that_carries_no_newline() {
    match echo_after(|run| run.write("noeol")) {
        Ok(printed) => assert!(
            printed.contains("got[noeol]"),
            "[BUILTIN-INPUT]: \"end-of-file ends a line just as a newline does\", so an \
             unterminated final line is still a line, not a discarded one. Printed:\n{printed}"
        ),
        Err(reason) => panic!("{reason}"),
    }
}

#[test]
fn a_writer_silent_past_any_grace_still_has_its_line_delivered() {
    let result = echo_after(|run| {
        std::thread::sleep(SILENT_GAP);
        run.write("slow\n")
    });
    match result {
        Ok(printed) => assert!(
            printed.contains("got[slow]"),
            "[BUILTIN-INPUT]: \"elapsed silence is not end-of-file\". A writer that has \
             not spoken yet is connected, and answering `\"\"` for it DISCARDS the line \
             it was about to send. Printed:\n{printed}"
        ),
        Err(reason) => panic!("{reason}"),
    }
}

#[test]
fn a_pause_in_the_middle_of_a_line_does_not_truncate_it() {
    let result = echo_after(|run| {
        run.write("sl")?;
        std::thread::sleep(SILENT_GAP);
        run.write("ow\n")
    });
    match result {
        Ok(printed) => assert!(
            printed.contains("got[slow]"),
            "[BUILTIN-INPUT]: \"a slow writer's line still arrives whole\". Truncating at \
             the pause is worse than dropping the line: `sl` parses as a DIFFERENT value \
             rather than an absent one. Printed:\n{printed}"
        ),
        Err(reason) => panic!("{reason}"),
    }
}

#[test]
fn an_open_silent_descriptor_is_waited_on_and_released_by_its_close() {
    let mut run = match Running::start(true) {
        Ok(run) => run,
        Err(reason) => panic!("{reason}"),
    };
    assert!(
        run.parked_in_input(),
        "the program never reached `input()`; it printed:\n{}",
        run.output()
    );
    assert_eq!(
        run.exited_within(PARKED_OBSERVATION),
        None,
        "[BUILTIN-INPUT]: an open descriptor that has not spoken yet is NOT end-of-file. \
         Finishing here means a grace expired and the program answered `\"\"` for a writer \
         that still had a line to send."
    );
    run.close_stdin();
    let released = run.exited_within(RUN_BUDGET);
    let printed = run.output();
    assert_eq!(
        released,
        Some(true),
        "a wait that a close cannot release is a hang. Printed:\n{printed}"
    );
    assert!(
        printed.contains("got[]"),
        "[BUILTIN-INPUT]: closing with nothing written is end-of-file with nothing read, \
         which is `\"\"`. Printed:\n{printed}"
    );
}

#[test]
fn the_echo_program_compiles_within_its_budget() {
    let complaint = match echo_build() {
        Ok((path, _)) if !path.exists() => format!(
            "the compiler reported success but produced no binary at {}",
            path.display()
        ),
        Ok((_, took)) if took >= COMPILE_BUDGET => {
            format!("compiling a two-line program took {took:?}")
        }
        Ok(_) => String::new(),
        Err(reason) => reason,
    };
    assert!(complaint.is_empty(), "{complaint}");
}
