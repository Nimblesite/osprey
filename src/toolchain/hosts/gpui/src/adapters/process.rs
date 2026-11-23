//! Direct process launch with bounded, concurrently drained output.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

use serde_json::json;

use super::{number, text};
use crate::ffi;
use crate::rows::Ask;

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

#[derive(Default)]
struct Collected {
    bytes: Vec<u8>,
    size: u64,
}

/// Runs one executable without a shell and writes its UTF-8 stdout to a slice.
pub fn run(ask: &Ask) -> Result<String, String> {
    cancellable(ask, &AtomicBool::new(false))
}

pub(crate) fn cancellable(ask: &Ask, cancelled: &AtomicBool) -> Result<String, String> {
    execute(ask, None, cancelled)
}

/// Writes bounded UTF-8 input while draining both outputs concurrently.
pub(super) fn with_input(ask: &Ask, input: String) -> Result<String, String> {
    if input.len() > 2 * 1024 * 1024 {
        return Err("Process input exceeds the 2 MiB limit".to_owned());
    }
    execute(ask, Some(input), &AtomicBool::new(false))
}

fn execute(ask: &Ask, input: Option<String>, cancelled: &AtomicBool) -> Result<String, String> {
    let executable = text(ask, 1, "executable")?;
    if !Path::new(&executable).is_absolute() {
        return Err("Executable must be an absolute path".to_owned());
    }
    let launched = launch(
        &executable,
        &ask.texts(2),
        number(ask, 3, "timeout")?,
        number(ask, 4, "limit")?,
        input,
        cancelled,
    )?;
    let stdout = String::from_utf8(launched.stdout).map_err(|_| "Not UTF-8 text".to_owned())?;
    ffi::put_slice(&text(ask, 6, "slice")?, &stdout)?;
    Ok(json!({"status": launched.status, "stderr": stderr_text(&launched.stderr)}).to_string())
}

/// Launches a child and enforces its wall-clock deadline and per-stream byte limit.
fn launch(
    executable: &str,
    arguments: &[String],
    timeout: u64,
    limit: u64,
    input: Option<String>,
    cancelled: &AtomicBool,
) -> Result<Launched, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("Run cancelled".into());
    }
    let mut command = Command::new(executable);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|problem| problem.to_string())?;
    let stdout = child.stdout.take().ok_or("run stdout is unavailable")?;
    let stderr = child.stderr.take().ok_or("run stderr is unavailable")?;
    let (sender, receiver) = mpsc::sync_channel(4);
    let streams = if let Some(input) = input {
        let Some(mut stdin) = child.stdin.take() else {
            stop(&mut child);
            return Err("Process input pipe is unavailable".to_owned());
        };
        let inbox = sender.clone();
        std::thread::spawn(move || {
            let result = stdin
                .write_all(input.as_bytes())
                .map_err(|error| error.to_string());
            drop(stdin);
            let _ = inbox.send(Event::Finished(result));
        });
        3
    } else {
        2
    };
    let outbox = sender.clone();
    std::thread::spawn(move || drain(stdout, Stream::Stdout, outbox));
    std::thread::spawn(move || drain(stderr, Stream::Stderr, sender));
    collect(&mut child, receiver, timeout, limit, streams, cancelled)
}

/// Drains one pipe in blocks so neither output stream can deadlock the child.
fn drain(mut pipe: impl Read, stream: Stream, sender: SyncSender<Event>) {
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
    streams: usize,
    cancelled: &AtomicBool,
) -> Result<Launched, String> {
    let started = Instant::now();
    let duration = Duration::from_millis(timeout.max(1));
    let (mut stdout, mut stderr) = (Collected::default(), Collected::default());
    let mut open = streams;
    let mut failure = None;
    while open > 0 {
        match receiver.recv_timeout(
            remaining(child, started, duration, timeout, cancelled)?.min(Duration::from_millis(40)),
        ) {
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
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                stop(child);
                return Err("run output channel closed".to_owned());
            }
        }
    }
    let status = wait_before_deadline(child, started, duration, timeout, cancelled)?;
    match failure {
        Some(problem) => Err(problem),
        None => Ok(Launched {
            status: status.code().unwrap_or(-1),
            stdout: stdout.bytes,
            stderr: stderr.bytes,
        }),
    }
}

/// Output EOF is not process exit; preserve the deadline after both pipes close.
fn wait_before_deadline(
    child: &mut Child,
    started: Instant,
    duration: Duration,
    timeout: u64,
    cancelled: &AtomicBool,
) -> Result<std::process::ExitStatus, String> {
    loop {
        if let Some(status) = child.try_wait().map_err(|problem| problem.to_string())? {
            return Ok(status);
        }
        let left = remaining(child, started, duration, timeout, cancelled)?;
        std::thread::sleep(left.min(Duration::from_millis(2)));
    }
}

/// The time left before the deadline, or the stopped child's timeout error.
fn remaining(
    child: &mut Child,
    started: Instant,
    duration: Duration,
    timeout: u64,
    cancelled: &AtomicBool,
) -> Result<Duration, String> {
    if cancelled.load(Ordering::Acquire) {
        stop_group(child);
        return Err("Run cancelled".into());
    }
    duration
        .checked_sub(started.elapsed())
        .ok_or_else(|| timed_out(child, timeout))
}

fn timed_out(child: &mut Child, timeout: u64) -> String {
    stop_group(child);
    format!("Timed out after {timeout} ms")
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

/// Each Run starts a new process group; its renderer may itself be running an encoder.
fn stop_group(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: launch set this unreaped child's process group to its own positive PID.
        unsafe { libc::kill(-pid, libc::SIGKILL) };
    }
    stop(child);
}

/// Kills and reaps a child after a deadline or output-limit failure.
pub(super) fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Decodes stderr lossily and retains the Swift adapter's 64 KiB prefix.
fn stderr_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(STDERR_LIMIT)]).into_owned()
}

#[cfg(test)]
mod tests {
    // The run and session tests drive Unix executables; their helpers come with them.
    #[cfg(unix)]
    use serde_json::json;

    #[cfg(unix)]
    use crate::adapters::answer;

    use super::stderr_text;
    #[cfg(unix)]
    use crate::ffi;
    #[cfg(unix)]
    use crate::rows::{Ask, Field};

    #[cfg(unix)]
    /// Returns a run ask for a Unix executable.
    fn run_request(executable: &str, arguments: &[&str], timeout: u64, limit: u64) -> Ask {
        let mut fields = vec![Field {
            field: 1,
            number: 0,
            text: executable.to_owned(),
        }];
        fields.extend(arguments.iter().map(|argument| Field {
            field: 2,
            number: 0,
            text: (*argument).to_owned(),
        }));
        fields.push(Field {
            field: 3,
            number: timeout as i64,
            text: String::new(),
        });
        fields.push(Field {
            field: 4,
            number: limit as i64,
            text: String::new(),
        });
        fields.push(Field {
            field: 6,
            number: 0,
            text: format!("run-test-{executable}-{limit}"),
        });
        Ask {
            code: 1,
            ticket: "run-test".to_owned(),
            fields,
        }
    }

    #[cfg(unix)]
    #[test]
    /// Runs an executable directly and publishes stdout.
    fn run_captures_stdout_status_and_stderr() {
        let request = run_request("/bin/echo", &["hello"], 1_000, 100);
        let result = answer(&request).and_then(|value| {
            assert_eq!(value, json!({"status": 0, "stderr": ""}).to_string());
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
        let cancel = super::AtomicBool::new(false);
        let request = run_request("/bin/sh", &["-c", "sleep 12 & wait"], 15_000, 100);
        std::thread::scope(|scope| {
            let run = scope.spawn(|| super::cancellable(&request, &cancel));
            std::thread::sleep(std::time::Duration::from_millis(120));
            cancel.store(true, super::Ordering::Release);
            assert_eq!(
                run.join().ok().and_then(Result::err).as_deref(),
                Some("Run cancelled")
            );
        });
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
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

    #[cfg(unix)]
    #[test]
    /// A child that closes both output pipes must still obey its deadline.
    fn run_deadline_survives_closed_output_pipes() {
        let started = std::time::Instant::now();
        let request = run_request("/bin/sh", &["-c", "exec 1>&- 2>&-; exec sleep 2"], 20, 100);
        let result = answer(&request);
        assert!(result.is_err_and(|problem| problem.contains("Timed out")));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    /// Input larger than a pipe buffer must not deadlock while output is produced.
    fn input_drains_large_echo_and_enforces_bounds() {
        let input = "samples".repeat(100_000);
        let request = run_request("/bin/cat", &[], 2_000, 1_000_000);
        let result = super::with_input(&request, input.clone());
        assert!(result.is_ok());
        assert_eq!(ffi::get_slice("run-test-/bin/cat-1000000"), Some(input));
        assert!(super::with_input(&request, "x".repeat(2 * 1024 * 1024 + 1)).is_err());
        let sleeping = run_request("/bin/sleep", &["2"], 20, 100);
        assert!(super::with_input(&sleeping, "x".repeat(100_000)).is_err());
    }

    #[cfg(unix)]
    #[test]
    /// The public capability boundary transfers a slice without exposing it in argv.
    fn process_input_host_call_uses_real_stdin() {
        let fields = [
            (1, "process"),
            (2, "input"),
            (3, "executable"),
            (3, "/bin/cat"),
            (3, "stdinSlice"),
            (3, "process-input-test"),
            (6, "process-input-output"),
        ];
        let mut request = Ask {
            code: 11,
            ticket: "stdin-test".to_owned(),
            fields: fields
                .into_iter()
                .map(|(field, text)| Field {
                    field,
                    text: text.to_owned(),
                    number: 0,
                })
                .collect(),
        };
        request.fields.extend([
            Field {
                field: 4,
                number: 1000,
                text: String::new(),
            },
            Field {
                field: 5,
                number: 1000,
                text: String::new(),
            },
        ]);
        assert!(ffi::put_slice("process-input-test", "a sample buffer\n").is_ok());
        assert!(answer(&request).is_ok());
        assert_eq!(
            ffi::get_slice("process-input-output").as_deref(),
            Some("a sample buffer\n")
        );
        request.fields.push(Field {
            field: 3,
            text: "unpaired".to_owned(),
            number: 0,
        });
        assert!(answer(&request).is_err());
    }

    #[cfg(unix)]
    #[test]
    /// [SYNTH-HOST-STREAM-SESSION] Real pipes retain state, bound stalls, and reap children.
    fn persistent_sessions_exchange_and_fail_closed() -> Result<(), String> {
        use super::super::process_session::Session;
        let mut session = Session::open("/bin/cat", &[])?;
        let pid = session.id();
        for input in ["first".to_owned(), "x".repeat(700_000), "third".to_owned()] {
            assert_eq!(session.exchange(&input, 2_000, 1_000_000)?, input);
            assert_eq!(session.id(), pid);
        }
        assert!(session.exchange("two\nlines", 100, 100).is_err());
        assert_eq!(session.exchange("still alive", 1_000, 100)?, "still alive");
        assert!(session.exchange("too large", 1_000, 3).is_err());
        assert!(session.exchange("dead", 100, 100).is_err());
        let mut stalled = Session::open("/bin/sleep", &["2".to_owned()])?;
        let started = std::time::Instant::now();
        assert!(stalled.exchange(&"x".repeat(100_000), 20, 100).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(stalled.exited()?);
        // A worker that closes its output, then one that floods stderr: each fails and is reaped.
        for (script, timeout) in [
            ("exec 1>&- 2>&-; exec sleep 2", 100),
            ("while :; do printf diagnostic >&2; done", 1_000),
        ] {
            let mut session = Session::open("/bin/sh", &["-c".to_owned(), script.to_owned()])?;
            assert!(session.exchange("hello", timeout, 100).is_err());
            assert!(session.exited()?);
        }
        assert!(Session::open("cat", &[]).is_err());
        Ok(())
    }

    const STDERR_TEST_SIZE: usize = 65_537;
}
