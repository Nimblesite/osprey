//! Direct process launch with bounded, concurrently drained output.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{field, number, strings};
use crate::envelope::Request;
use crate::ffi;

const STDERR_LIMIT: usize = 65_536;
const READ_BLOCK: usize = 65_536;

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

enum Event {
    Bytes(Stream, Vec<u8>),
    Finished(Result<(), String>),
}

struct Launched {
    status: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct Collected {
    bytes: Vec<u8>,
    size: u64,
}

/// Runs one executable without a shell and writes its UTF-8 stdout to a slice.
pub fn run(request: &Request) -> Result<Value, String> {
    let executable = field(request, "executable")?;
    if !Path::new(executable).is_absolute() {
        return Err("Executable must be an absolute path".to_owned());
    }
    let launched = launch(
        executable,
        &strings(request, "arguments")?,
        number(request, "timeout")?,
        number(request, "limit")?,
    )?;
    let stdout = String::from_utf8(launched.stdout).map_err(|_| "Not UTF-8 text".to_owned())?;
    ffi::put_slice(field(request, "slice")?, &stdout)?;
    Ok(json!({"status": launched.status, "stderr": stderr_text(&launched.stderr)}))
}

/// Launches a child and enforces its wall-clock deadline and per-stream byte limit.
fn launch(
    executable: &str,
    arguments: &[&str],
    timeout: u64,
    limit: u64,
) -> Result<Launched, String> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|problem| problem.to_string())?;
    let stdout = child.stdout.take().ok_or("run stdout is unavailable")?;
    let stderr = child.stderr.take().ok_or("run stderr is unavailable")?;
    let (sender, receiver) = mpsc::channel();
    let outbox = sender.clone();
    std::thread::spawn(move || drain(stdout, Stream::Stdout, outbox));
    std::thread::spawn(move || drain(stderr, Stream::Stderr, sender));
    collect(&mut child, receiver, timeout, limit)
}

/// Drains one pipe in blocks so neither output stream can deadlock the child.
fn drain(mut pipe: impl Read, stream: Stream, sender: Sender<Event>) {
    let mut buffer = vec![0; READ_BLOCK];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(Event::Finished(Ok(())));
                return;
            }
            Ok(size) => {
                if sender
                    .send(Event::Bytes(stream, buffer[..size].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(problem) => {
                let _ = sender.send(Event::Finished(Err(problem.to_string())));
                return;
            }
        }
    }
}

/// Collects both pipes until EOF, killing the child on timeout or excess output.
fn collect(
    child: &mut Child,
    receiver: Receiver<Event>,
    timeout: u64,
    limit: u64,
) -> Result<Launched, String> {
    let started = Instant::now();
    let duration = Duration::from_millis(timeout.max(1));
    let mut stdout = Collected {
        bytes: Vec::new(),
        size: 0,
    };
    let mut stderr = Collected {
        bytes: Vec::new(),
        size: 0,
    };
    let mut open = 2;
    let mut failure = None;
    while open > 0 {
        let Some(remaining) = duration.checked_sub(started.elapsed()) else {
            stop(child);
            return Err(format!("Timed out after {timeout} ms"));
        };
        match receiver.recv_timeout(remaining) {
            Ok(Event::Bytes(stream, bytes)) => absorb(
                child,
                stream,
                bytes,
                limit,
                &mut stdout,
                &mut stderr,
                &mut failure,
            ),
            Ok(Event::Finished(result)) => {
                open -= 1;
                if let Err(problem) = result {
                    stop(child);
                    return Err(problem);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                stop(child);
                return Err(format!("Timed out after {timeout} ms"));
            }
            Err(RecvTimeoutError::Disconnected) => {
                stop(child);
                return Err("run output channel closed".to_owned());
            }
        }
    }
    let status = child.wait().map_err(|problem| problem.to_string())?;
    match failure {
        Some(problem) => Err(problem),
        None => Ok(Launched {
            status: status.code().unwrap_or(-1),
            stdout: stdout.bytes,
            stderr: stderr.bytes,
        }),
    }
}

/// Adds one block to the selected bounded stream and stops an overflowing child.
fn absorb(
    child: &mut Child,
    stream: Stream,
    bytes: Vec<u8>,
    limit: u64,
    stdout: &mut Collected,
    stderr: &mut Collected,
    failure: &mut Option<String>,
) {
    if failure.is_some() {
        return;
    }
    let (collected, keep) = match stream {
        Stream::Stdout => (stdout, usize::MAX),
        Stream::Stderr => (stderr, STDERR_LIMIT),
    };
    collected.size = collected.size.saturating_add(bytes.len() as u64);
    if collected.size > limit {
        *failure = Some(format!(
            "{} bytes exceeds the {limit} byte limit",
            collected.size
        ));
        let _ = child.kill();
        return;
    }
    let remaining = keep.saturating_sub(collected.bytes.len());
    collected
        .bytes
        .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
}

/// Kills and reaps a child after a deadline or output-limit failure.
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Decodes stderr lossily and retains the Swift adapter's 64 KiB prefix.
fn stderr_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(STDERR_LIMIT)]).into_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::adapters::answer;

    use super::stderr_text;
    use crate::ffi;

    #[cfg(unix)]
    /// Returns a run request for a Unix executable.
    fn run_request(
        executable: &str,
        arguments: &[&str],
        timeout: u64,
        limit: u64,
    ) -> crate::envelope::Request {
        super::super::request(
            "run",
            &[
                ("executable", json!(executable)),
                ("arguments", json!(arguments)),
                ("timeout", json!(timeout)),
                ("limit", json!(limit)),
                ("slice", json!(format!("run-test-{}-{}", executable, limit))),
            ],
        )
    }

    #[cfg(unix)]
    #[test]
    /// Runs an executable directly and publishes stdout.
    fn run_captures_stdout_status_and_stderr() {
        let request = run_request("/bin/echo", &["hello"], 1_000, 100);
        let result = answer(&request).and_then(|value| {
            assert_eq!(value, json!({"status": 0, "stderr": ""}));
            ffi::get_slice("run-test-/bin/echo-100").ok_or_else(|| "missing slice".to_owned())
        });
        assert_eq!(result.as_deref(), Ok("hello\n"));
    }

    #[cfg(unix)]
    #[test]
    /// Kills a process that exceeds its millisecond deadline.
    fn run_kills_a_timed_out_process() {
        let started = std::time::Instant::now();
        let result = answer(&run_request("/bin/sleep", &["2"], 20, 100));
        assert_eq!(result.err().as_deref(), Some("Timed out after 20 ms"));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    /// Rejects output beyond the requested byte limit.
    fn run_rejects_output_over_limit() {
        let result = answer(&run_request("/bin/echo", &["too large"], 1_000, 3));
        assert!(result
            .err()
            .is_some_and(|problem| problem.contains("exceeds the 3 byte limit")));
    }

    #[test]
    /// Truncates stderr at the protocol's 64 KiB boundary.
    fn run_truncates_stderr() {
        assert_eq!(stderr_text(&vec![b'x'; STDERR_TEST_SIZE]).len(), 65_536);
    }

    const STDERR_TEST_SIZE: usize = 65_537;
}
