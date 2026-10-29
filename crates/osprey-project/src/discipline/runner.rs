//! Running a checker as a separate process ([TYPES-DISCIPLINE-PROTOCOL]):
//! the compiler links no discipline, a checker fault cannot take the compiler
//! down, and a checker can be any size.

use super::Discipline;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long the compiler sleeps between polls of a running checker.
const POLL: Duration = Duration::from_millis(20);
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
    /// while the checker source is unchanged (content hash in the file name).
    fn executable(&self, discipline: &Discipline) -> Result<PathBuf, String> {
        let source = std::fs::read(&discipline.checker).map_err(|error| {
            format!(
                "has no readable checker at {}: {error}",
                discipline.checker.display()
            )
        })?;
        let output = self.build_dir.join(format!(
            "{}-{:016x}",
            discipline.name,
            super::cache::digest(&source)
        ));
        if output.is_file() {
            return Ok(output);
        }
        std::fs::create_dir_all(&self.build_dir)
            .map_err(|error| format!("cannot create {}: {error}", self.build_dir.display()))?;
        build(&self.compiler, &discipline.checker, &output)?;
        Ok(output)
    }
}

impl CheckerRunner for ProcessRunner {
    fn run(&self, discipline: &Discipline, request: &str) -> Result<String, String> {
        let executable = self.executable(discipline)?;
        let mut child = Command::new(&executable)
            .current_dir(&discipline.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not start ({error})"))?;
        feed(&mut child, request)?;
        wait(child, discipline.timeout)
    }
}

fn build(compiler: &Path, checker: &Path, output: &Path) -> Result<(), String> {
    let result = Command::new(compiler)
        .arg(checker)
        .arg("--compile")
        .arg("--quiet")
        .arg("--no-warnings")
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|error| format!("could not be built: {error}"))?;
    if result.status.success() && output.is_file() {
        return Ok(());
    }
    Err(format!(
        "failed to build: {}",
        tail(&String::from_utf8_lossy(&result.stderr))
    ))
}

fn feed(child: &mut Child, request: &str) -> Result<(), String> {
    let Some(mut stdin) = child.stdin.take() else {
        return Err("had no stdin".to_string());
    };
    stdin
        .write_all(request.as_bytes())
        .and_then(|()| stdin.write_all(b"\n"))
        .map_err(|error| format!("did not read its request ({error})"))
}

/// Wait up to `timeout` for the checker, then kill it and report the limit.
/// Both pipes are drained on their own threads so a large erased module
/// cannot block the checker on a full pipe while the compiler polls.
fn wait(mut child: Child, timeout: Duration) -> Result<String, String> {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "exceeded its time limit of {} seconds",
                    timeout.as_secs()
                ));
            }
            Err(error) => return Err(format!("could not be waited for ({error})")),
        }
    };
    finish(success, join(stdout), &join(stderr))
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
