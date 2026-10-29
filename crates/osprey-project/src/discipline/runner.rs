//! Running a checker as a separate process ([TYPES-DISCIPLINE-PROTOCOL]):
//! the compiler links no discipline, a checker fault cannot take the compiler
//! down, and a checker can be any size.

use super::Discipline;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long the compiler sleeps between polls of a running checker.
const POLL: Duration = Duration::from_millis(20);
/// What a checker — or its compile — hears when it outlives its deadline.
pub(super) const TIME_LIMIT: &str = "exceeded its time limit";
/// How much of a crashed checker's stderr is quoted in the diagnostic.
const STDERR_TAIL: usize = 800;
/// Serial number making every build's private artifact path unique.
static NEXT_ARTIFACT: AtomicU64 = AtomicU64::new(0);

/// Something that answers a request line with a response line. The process
/// runner is the real one; tests substitute scripted answers.
pub trait CheckerRunner {
    /// Answer `request` for `discipline`, or explain why it could not.
    ///
    /// # Errors
    /// A message completing "checker `pkg` of discipline `name` ...".
    fn run(&self, discipline: &Discipline, request: &str) -> Result<String, String>;

    /// The identity of the binary `discipline` runs: the second half of the
    /// result-cache key of [TYPES-DISCIPLINE-PROTOCOL].
    ///
    /// # Errors
    /// A message completing "checker `pkg` of discipline `name` ..."; an
    /// unreadable binary must not degrade into a shared cache key.
    fn binary_digest(&self, discipline: &Discipline) -> Result<u64, String>;
}

/// Builds each checker once with the ordinary compiler and runs it under its
/// discipline's time limit.
#[derive(Debug, Clone)]
pub struct ProcessRunner {
    compiler: PathBuf,
    build_dir: PathBuf,
}

impl ProcessRunner {
    /// A runner building with `compiler` into `build_dir`.
    #[must_use]
    pub fn new(compiler: PathBuf, build_dir: PathBuf) -> Self {
        Self {
            compiler,
            build_dir,
        }
    }

    /// The executable for `discipline`, compiled on first use and reused
    /// while every build input is unchanged (content hashes in the file name):
    /// the entry module, the manifest, every source under the package's source
    /// roots and the compiler itself.
    pub(super) fn executable(
        &self,
        discipline: &Discipline,
        deadline: Instant,
    ) -> Result<PathBuf, String> {
        let output = self.artifact(discipline)?;
        if output.is_file() {
            return Ok(output);
        }
        std::fs::create_dir_all(&self.build_dir)
            .map_err(|error| format!("cannot create {}: {error}", self.build_dir.display()))?;
        build(&self.compiler, &discipline.checker, &output, deadline)?;
        Ok(output)
    }

    /// The cache path of `discipline`'s checker: its name under the digests of
    /// every build input and of the compiler itself.
    fn artifact(&self, discipline: &Discipline) -> Result<PathBuf, String> {
        Ok(self.build_dir.join(format!(
            "{}-{:016x}-{:016x}",
            discipline.name,
            super::inputs::build_digest(discipline)?,
            super::inputs::compiler_digest(&self.compiler)?
        )))
    }
}

impl CheckerRunner for ProcessRunner {
    fn run(&self, discipline: &Discipline, request: &str) -> Result<String, String> {
        let deadline = deadline_after(discipline.timeout)?;
        let executable = self.executable(discipline, deadline)?;
        run_executable(&executable, &discipline.root, request, deadline)
    }

    /// The bytes of the built checker, so a request is never answered across a
    /// rebuild of the binary that would answer it.
    fn binary_digest(&self, discipline: &Discipline) -> Result<u64, String> {
        let executable = self.executable(discipline, deadline_after(discipline.timeout)?)?;
        let bytes = std::fs::read(&executable).map_err(|error| {
            format!(
                "has no readable built checker at {}: {error}",
                executable.display()
            )
        })?;
        Ok(super::cache::digest(&bytes))
    }
}

/// The one deadline a request runs under.
///
/// # Errors
/// A limit the monotonic clock cannot reach: `Instant + Duration` panics, and
/// `timeout_seconds` is an unbounded, user-controlled manifest value.
fn deadline_after(timeout: Duration) -> Result<Instant, String> {
    Instant::now().checked_add(timeout).ok_or_else(|| {
        format!(
            "the clock cannot reach a time limit of {}s",
            timeout.as_secs()
        )
    })
}

/// Spawn the built `executable` in `root` and answer one request under
/// `deadline` ([TYPES-DISCIPLINE-PROTOCOL]): `run` goes through here, and
/// the real-process regressions call it with a script instead of a built
/// checker.
pub(super) fn run_executable(
    executable: &Path,
    root: &Path,
    request: &str,
    deadline: Instant,
) -> Result<String, String> {
    let mut command = Command::new(executable);
    let mut child = spawn(
        command
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .map_err(|error| format!("could not start ({error})"))?;
    feed(&mut child, request);
    wait(child, deadline)
}

/// Isolate each Unix process tree so a timeout can stop inherited pipe holders.
fn spawn(command: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = command.process_group(0);
    }
    command.spawn()
}

/// Stop this runner's isolated process group, including descendants holding pipes.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// The compiler invocation that builds `checker` into `output`.
fn compiler_command(compiler: &Path, checker: &Path, output: &Path) -> Command {
    let mut command = Command::new(compiler);
    let _ = command
        .arg(checker)
        .arg("--compile")
        .arg("--quiet")
        .arg("--no-warnings")
        .arg("-o")
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Compile `checker` into `output`, polling like `wait` so a hung compile
/// is killed at `deadline`, never blocking the compiler past the limit
/// ([TYPES-DISCIPLINE-PROTOCOL]).
fn build(compiler: &Path, checker: &Path, output: &Path, deadline: Instant) -> Result<(), String> {
    let artifact = private_artifact(output);
    match compile(compiler, checker, &artifact, deadline) {
        Ok(()) => publish(&artifact, output),
        Err(error) => {
            let _ = std::fs::remove_file(&artifact);
            Err(error)
        }
    }
}

/// Compile into `artifact`, leaving it only when the compiler exited
/// successfully and wrote it.
fn compile(
    compiler: &Path,
    checker: &Path,
    artifact: &Path,
    deadline: Instant,
) -> Result<(), String> {
    let mut child = spawn(&mut compiler_command(compiler, checker, artifact))
        .map_err(|error| format!("could not be built: {error}"))?;
    let pipes = Pipes::open(&mut child);
    let status =
        poll(&mut child, deadline).map_err(|error| format!("could not be built: {error}"))?;
    let Some(status) = status else {
        return Err(format!("could not be built: {TIME_LIMIT}"));
    };
    let text = collect(&pipes, deadline).ok_or_else(|| {
        stop(&mut child);
        format!("could not be built: {TIME_LIMIT}")
    })?;
    if status.success() && artifact.is_file() && text.stdout.is_ok() && text.stderr.is_ok() {
        return Ok(());
    }
    Err(format!("failed to build: {}", text.evidence()))
}

/// A private build path beside `output`: two invocations sharing a cache
/// directory never compile into, run, or overwrite each other's artifact.
fn private_artifact(output: &Path) -> PathBuf {
    let serial = NEXT_ARTIFACT.fetch_add(1, Ordering::Relaxed);
    let name = output
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    output.with_file_name(format!("{name}.{}.{serial}.tmp", std::process::id()))
}

/// Move the finished build into the cache path, so a checker never runs a
/// partially written binary and a failed build leaves no artifact behind.
fn publish(artifact: &Path, output: &Path) -> Result<(), String> {
    std::fs::rename(artifact, output).map_err(|error| {
        let _ = std::fs::remove_file(artifact);
        format!(
            "could not be built: cannot publish {}: {error}",
            output.display()
        )
    })
}

/// Write the request to the checker's stdin on its own thread, as `drain`
/// does for the pipes: a checker that never reads cannot block the compiler
/// on a full pipe, and the handle is dropped when the write ends, closing
/// the stream ([TYPES-DISCIPLINE-PROTOCOL]). A write error because the
/// checker exited early is not fatal while a well-formed response may still
/// arrive.
fn feed(child: &mut Child, request: &str) {
    if let Some(mut stdin) = child.stdin.take() {
        let request = request.to_string();
        let _ = std::thread::spawn(move || {
            let _ = stdin
                .write_all(request.as_bytes())
                .and_then(|()| stdin.write_all(b"\n"));
            drop(stdin);
        });
    }
}

/// Wait until `deadline` for the checker, then kill it and report the
/// limit. Both pipes are collected on their own threads and the deadline
/// bounds that collection too, so a checker leaving a background child with
/// the write ends open cannot hold the compiler past its limit.
fn wait(mut child: Child, deadline: Instant) -> Result<String, String> {
    let pipes = Pipes::open(&mut child);
    let Some(status) = poll(&mut child, deadline)? else {
        return Err(TIME_LIMIT.to_string());
    };
    let Some(text) = collect(&pipes, deadline) else {
        stop(&mut child);
        return Err(TIME_LIMIT.to_string());
    };
    finish(status.success(), text)
}

/// Poll `child` every `POLL` until it exits or `deadline` passes, killing it
/// at the deadline: `Ok(None)` means the limit hit.
fn poll(child: &mut Child, deadline: Instant) -> Result<Option<ExitStatus>, String> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                stop(child);
                let _ = child.wait();
                return Ok(None);
            }
            Err(error) => return Err(format!("could not be waited for ({error})")),
        }
    }
}

fn finish(success: bool, text: Text) -> Result<String, String> {
    match (success, text) {
        (
            true,
            Text {
                stdout: Ok(stdout),
                stderr: Ok(_),
            },
        ) if !stdout.trim().is_empty() => Ok(stdout),
        (_, text) => Err(verdict(success, &text)),
    }
}

fn verdict(success: bool, text: &Text) -> String {
    let answered = match &text.stdout {
        Ok(stdout) => !stdout.trim().is_empty(),
        Err(_) => false,
    };
    format!(
        "exited {} with {}: {}",
        if success { "cleanly" } else { "with an error" },
        if answered {
            "a response"
        } else {
            "no response"
        },
        text.evidence()
    )
}

/// The two pipes of one finished process, drained on their own threads and
/// handed over channels: a reader can stop at a deadline instead of waiting
/// for every holder of the write end to close.
struct Pipes {
    stdout: Receiver<Result<String, String>>,
    stderr: Receiver<Result<String, String>>,
}

impl Pipes {
    fn open(child: &mut Child) -> Self {
        Self {
            stdout: drain(child.stdout.take(), "stdout"),
            stderr: drain(child.stderr.take(), "stderr"),
        }
    }
}

/// What a finished process's pipes reported: decoded text, or why the bytes
/// could not be decoded.
struct Text {
    stdout: Result<String, String>,
    stderr: Result<String, String>,
}

impl Text {
    /// The diagnostic: whichever pipe failed to decode, so a binary
    /// diagnostic never reads as no output at all.
    fn evidence(&self) -> String {
        match (&self.stdout, &self.stderr) {
            (_, Err(stderr)) | (Err(stderr), Ok(_)) => stderr.clone(),
            (Ok(_), Ok(stderr)) => tail(stderr),
        }
    }
}

/// Both pipes' text, or `None` when a holder of a write end outlives
/// `deadline`: the deadline bounds the drain as well as the process.
fn collect(pipes: &Pipes, deadline: Instant) -> Option<Text> {
    Some(Text {
        stdout: receive(&pipes.stdout, deadline)?,
        stderr: receive(&pipes.stderr, deadline)?,
    })
}

fn receive(
    pipe: &Receiver<Result<String, String>>,
    deadline: Instant,
) -> Option<Result<String, String>> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    pipe.recv_timeout(remaining).ok()
}

fn drain(
    stream: Option<impl Read + Send + 'static>,
    name: &'static str,
) -> Receiver<Result<String, String>> {
    let (sender, receiver) = mpsc::channel();
    // Deliberately detached: a pipe a background child holds open past the
    // deadline must not delay the compiler while its holder exits.
    let _ = std::thread::spawn(move || {
        let mut text = String::new();
        let read = match stream {
            Some(mut stream) => stream.read_to_string(&mut text).map(|_| text),
            None => Ok(text),
        };
        let _ = sender.send(decoded(name, read));
    });
    receiver
}

/// The stream's text, or why its bytes are not UTF-8, which the caller must
/// report: a process writing a binary diagnostic used to look like one that
/// wrote nothing.
fn decoded(name: &'static str, read: std::io::Result<String>) -> Result<String, String> {
    read.map_err(|error| match error.kind() {
        std::io::ErrorKind::InvalidData => format!("could not decode {name}: invalid UTF-8"),
        _ => format!("could not read {name}: {error}"),
    })
}

/// The last [`STDERR_TAIL`] bytes of `stderr` from a character boundary: a cut
/// inside a three-byte scalar moves forward to the next scalar, so a slice
/// never splits a character.
fn tail(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return "(no stderr)".to_string();
    }
    let start = trimmed.len().saturating_sub(STDERR_TAIL);
    match trimmed.get(start..) {
        Some(bounded) => bounded.to_string(),
        None => next_boundary(trimmed, start).to_string(),
    }
}

/// `text` from the first character boundary after a mid-character `start`.
fn next_boundary(text: &str, start: usize) -> &str {
    text.char_indices()
        .map(|(index, _)| index)
        .find(|index| *index > start)
        .and_then(|index| text.get(index..))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod runner_tests;
