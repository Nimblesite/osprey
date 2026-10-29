//! Running a checker as a separate process ([TYPES-DISCIPLINE-PROTOCOL]):
//! the compiler links no discipline, a checker fault cannot take the compiler
//! down, and a checker can be any size.

use super::Discipline;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// How long the compiler sleeps between polls of a running checker.
const POLL: Duration = Duration::from_millis(20);
/// What a checker — or its compile — hears when it outlives its deadline.
const TIME_LIMIT: &str = "exceeded its time limit";
/// How much of a crashed checker's stderr is quoted in the diagnostic.
const STDERR_TAIL: usize = 800;

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
        let output = self.build_dir.join(format!(
            "{}-{:016x}-{:016x}",
            discipline.name,
            super::inputs::build_digest(discipline)?,
            super::inputs::compiler_digest(&self.compiler)?
        ));
        if output.is_file() {
            return Ok(output);
        }
        std::fs::create_dir_all(&self.build_dir)
            .map_err(|error| format!("cannot create {}: {error}", self.build_dir.display()))?;
        build(&self.compiler, &discipline.checker, &output, deadline)?;
        Ok(output)
    }
}

impl CheckerRunner for ProcessRunner {
    fn run(&self, discipline: &Discipline, request: &str) -> Result<String, String> {
        let deadline = Instant::now() + discipline.timeout;
        let executable = self.executable(discipline, deadline)?;
        run_executable(&executable, &discipline.root, request, deadline)
    }

    /// The bytes of the built checker, so a request is never answered across a
    /// rebuild of the binary that would answer it.
    fn binary_digest(&self, discipline: &Discipline) -> Result<u64, String> {
        let executable = self.executable(discipline, Instant::now() + discipline.timeout)?;
        let bytes = std::fs::read(&executable).map_err(|error| {
            format!(
                "has no readable built checker at {}: {error}",
                executable.display()
            )
        })?;
        Ok(super::cache::digest(&bytes))
    }
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
    let mut child = Command::new(executable)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start ({error})"))?;
    feed(&mut child, request);
    wait(child, deadline)
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
        .arg(output);
    command
}

/// Compile `checker` into `output`, polling like `wait` so a hung compile
/// is killed at `deadline`, never blocking the compiler past the limit
/// ([TYPES-DISCIPLINE-PROTOCOL]).
fn build(compiler: &Path, checker: &Path, output: &Path, deadline: Instant) -> Result<(), String> {
    let mut child = compiler_command(compiler, checker, output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not be built: {error}"))?;
    let stderr = drain(child.stderr.take());
    let _ = drain(child.stdout.take());
    let status = poll(&mut child, deadline).map_err(|error| format!("could not be built: {error}"));
    let stderr = join(stderr);
    match status? {
        Some(status) if status.success() && output.is_file() => Ok(()),
        Some(_) => Err(format!("failed to build: {}", tail(&stderr))),
        None => Err(format!("could not be built: {TIME_LIMIT}")),
    }
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
/// limit. Both pipes are drained on their own threads so a large erased
/// module cannot block the checker on a full pipe while the compiler polls.
fn wait(mut child: Child, deadline: Instant) -> Result<String, String> {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    match poll(&mut child, deadline)? {
        Some(status) => finish(status.success(), join(stdout), &join(stderr)),
        None => Err(TIME_LIMIT.to_string()),
    }
}

/// Poll `child` every `POLL` until it exits or `deadline` passes, killing it
/// at the deadline: `Ok(None)` means the limit hit.
fn poll(child: &mut Child, deadline: Instant) -> Result<Option<ExitStatus>, String> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            Err(error) => return Err(format!("could not be waited for ({error})")),
        }
    }
}

fn finish(success: bool, stdout: String, stderr: &str) -> Result<String, String> {
    if success && !stdout.trim().is_empty() {
        return Ok(stdout);
    }
    Err(format!(
        "exited {} with {}: {}",
        if success { "cleanly" } else { "with an error" },
        if stdout.trim().is_empty() {
            "no response"
        } else {
            "a response"
        },
        tail(stderr)
    ))
}

fn drain(stream: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stream) = stream {
            let _ = stream.read_to_string(&mut text);
        }
        text
    })
}

fn join(handle: std::thread::JoinHandle<String>) -> String {
    handle.join().unwrap_or_default()
}

fn tail(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return "(no stderr)".to_string();
    }
    let start = trimmed.len().saturating_sub(STDERR_TAIL);
    let start = trimmed
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index >= start)
        .unwrap_or(0);
    trimmed[start..].to_string()
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod runner_tests;
