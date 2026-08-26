//! Focused ownership regressions for values crossing a fiber or channel
//! boundary.
//!
//! Every case here reads the same oracle: the ARC exit census on stderr. That
//! is deliberate. An ownership defect changes NOTHING a program prints — the
//! value is still there, still correct, still formatted the same — so no
//! assertion over output can see it, and a golden cannot either. Only the
//! count of objects still alive when the process ends can.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Long enough that a loaded machine cannot trip it, short enough that a
/// regression reports in seconds. Every failure this file hunts lives in
/// `send`/`recv`, and BOTH of those block by design: a defect that loses a
/// queued value or refuses a valid handle parks the child on a condition
/// nothing will signal. An unbounded wait turns that into a test binary that
/// never returns and a CI job that dies on its wall clock with no failing test
/// named — which reads as flaky infrastructure, not as the regression it is.
const RUN_BUDGET: Duration = Duration::from_mins(1);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A pipe being copied into a shared buffer on its own thread, so a child that
/// fills a pipe while the poll loop waits cannot deadlock the two together.
struct Drain {
    reader: std::thread::JoinHandle<()>,
    held: Arc<Mutex<Vec<u8>>>,
}

impl Drain {
    fn start(mut pipe: impl Read + Send + 'static) -> Self {
        let held = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&held);
        let reader = std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if pipe.read_to_end(&mut buffer).is_ok() {
                if let Ok(mut sink) = sink.lock() {
                    *sink = buffer;
                }
            }
        });
        Self { reader, held }
    }

    /// What has arrived so far. Used only for the timeout report, where the
    /// writer is still alive and there is nothing to wait for.
    fn so_far(&self) -> Vec<u8> {
        match self.held.lock() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Everything the pipe carried. Joining is what makes that true: the child
    /// exiting closes its end, so the reader finishes promptly -- and WITHOUT
    /// the join, a test that reads the ARC census can see an empty buffer for a
    /// program that printed one, which reads as a leak that is not there.
    fn finish(self) -> Vec<u8> {
        let Self { reader, held } = self;
        let _ = reader.join();
        let carried = match held.lock() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        carried
    }
}

/// Wait for `child` within [`RUN_BUDGET`], killing it if it overruns.
fn wait_bounded(child: &mut Child, out: Drain, err: Drain) -> io::Result<Output> {
    let deadline = Instant::now() + RUN_BUDGET;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Output {
                status,
                stdout: out.finish(),
                stderr: err.finish(),
            });
        }
        if Instant::now() >= deadline {
            child.kill()?;
            let _ = child.wait()?;
            return Err(io::Error::other(format!(
                "the program did not finish within {RUN_BUDGET:?}; it is parked, \
                 not slow. stdout so far:\n{}\nstderr so far:\n{}",
                String::from_utf8_lossy(&out.so_far()),
                String::from_utf8_lossy(&err.so_far())
            )));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn run_arc(source: &str) -> io::Result<Output> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo_root())
        .args(["/dev/stdin", "--run", "--quiet", "--memory=arc"])
        .env("OSPREY_ARC_DEBUG", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let out = match child.stdout.take() {
        Some(pipe) => Drain::start(pipe),
        None => return Err(io::Error::other("no stdout pipe")),
    };
    let err = match child.stderr.take() {
        Some(pipe) => Drain::start(pipe),
        None => return Err(io::Error::other("no stderr pipe")),
    };
    match child.stdin.take() {
        // Closed by the drop, so the program reads a real end-of-file.
        Some(mut pipe) => pipe.write_all(source.as_bytes())?,
        None => return Err(io::Error::other("no stdin pipe")),
    }
    wait_bounded(&mut child, out, err)
}

#[test]
fn awaited_overflow_error_transfer_is_released_under_arc() -> io::Result<()> {
    let source = r"
fn checked(value: int) -> Result<int, MathError> = value + 1

let observed = await(spawn checked(9223372036854775807))
print(toString(observed))
";

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Error(integer overflow)\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn repeated_awaits_each_receive_a_live_result_under_arc() -> io::Result<()> {
    let source = r"
fn checked(value: int) -> Result<int, MathError> = value + 1

let task = spawn checked(41)
let first = await(task)
print(toString(first))
let second = await(task)
print(toString(second))
";

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Success(42)\nSuccess(42)\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn repeated_awaits_each_receive_a_live_string_under_arc() -> io::Result<()> {
    let source = r#"
fn greet(value: int) -> string = "hello ${value}"

let task = spawn greet(7)
let first = await(task)
print(first)
let second = await(task)
print(second)
"#;

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "hello 7\nhello 7\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn scalar_return_releases_map_on_nonreading_match_arm_under_arc() -> io::Result<()> {
    let source = r#"
fn conditionalBlock(value) = {
    let scoreMap = { "test1": 84, "test2": 90 }
    let doubled = value * 2
    match doubled {
        Success { dValue } => match dValue {
            84 => match scoreMap["test1"] {
                Success { sValue } => {
                    let added = sValue + 10
                    match added {
                        Success { value } => value
                        Error { message } => 0
                    }
                }
                Error { message } => 0
            }
            _ => 0
        }
        Error { message } => 0
    }
}

test("map branch scalar", fn() => checkAll("map branch scalar", [
    conditionalBlock(10) == 0
]))
"#;

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn sibling_match_arm_maps_release_independently_under_arc() -> io::Result<()> {
    let source = r#"
fn siblingOwner(flag) = match flag {
    true => {
        let left = { "left": 1 }
        mapLength(left)
    }
    false => {
        let right = { "right": 2 }
        mapLength(right)
    }
}

test("sibling owners", fn() => checkAll("sibling owners", [
    siblingOwner(true) == 1,
    siblingOwner(false) == 1
]))
"#;

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// The ARC census line, or the whole stderr when it is not there at all.
fn arc_census(stderr: &str) -> &str {
    stderr
        .lines()
        .find(|line| line.starts_with("[osp-arc] exit:"))
        .unwrap_or(stderr)
}

// [CONCURRENCY-CHANNEL] [GC-ARC-PERCEUS] A channel TAKES OWNERSHIP of what it
// is sent: `send` retains the value so the receiver's copy is alive no matter
// what the sender does next. That is a debt, and a program can end without the
// receive that pays it -- a producer that outruns its consumer, a fiber that
// stops early, or simply a queue nobody drained. The channel's own teardown is
// the only thing left that can release it.
#[test]
fn a_value_never_received_is_released_when_its_channel_is_torn_down() -> io::Result<()> {
    // Two sends, one receive: the queue ends the program holding exactly one
    // value, so a teardown that releases nothing and one that releases only
    // what was received are both visible, and neither can be confused with a
    // send that never retained.
    let source = r"
let queue = Channel(4)
send(queue, [1, 2, 3])
send(queue, [4, 5])
let taken = recv(queue)
print(toString(listLength(taken)))
";

    let output = run_arc(source)?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr={stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
    assert!(
        stderr.contains("[osp-arc] exit: 0 live objects"),
        "the list still queued must be released by the channel's teardown, \
         not left alive: {}",
        arc_census(&stderr)
    );
    Ok(())
}

// [CONCURRENCY-CHANNEL] A non-positive capacity is REJECTED: "`Channel(capacity)`
// creates a FIFO channel whose positive integer `capacity` is the number of
// buffered values. Zero and negative capacities are rejected; the runtime does
// not implement rendezvous channels."
//
// Rejected has to mean the program cannot go on as though it had a channel.
// Today it does not: `channel_create` answers `-1`, the language keeps treating
// that as a `Channel<T>`, `send` returns without enqueueing anything, and
// `recv` returns `-1` AS THE VALUE — which codegen converts to a pointer.
// Measured: adding one `recv` to any case below exits 139 (SIGSEGV) with no
// diagnostic, on the default backend and under ARC alike.
//
// The capacity is reached four ways ON PURPOSE. A check that only rejects a
// literal `Channel(0)` satisfies the first case and leaves the other three
// poisoning exactly the same handle, so a fix that inspects the call site
// rather than the value cannot pass this.
const REFUSED_CAPACITIES: [(&str, &str, &str); 4] = [
    ("a literal zero", "let ch = Channel(0)", "Channel(0)"),
    ("a literal negative", "let ch = Channel(-1)", "Channel(-1)"),
    ("a computed zero", "let ch = channelOf(0)", "Channel(0)"),
    (
        "a computed negative",
        "let ch = channelOf(-1)",
        "Channel(-1)",
    ),
];

#[test]
fn a_non_positive_capacity_channel_is_refused_rather_than_handed_over() -> io::Result<()> {
    for (label, binding, named) in REFUSED_CAPACITIES {
        // The print comes BEFORE any use of the handle, on purpose. A guard
        // that only fired inside `send` would still let this line out, and the
        // `recv` poison — the one that segfaults — would survive untouched.
        // Only a refusal at CREATION keeps `created` off stdout.
        let source = format!(
            "fn channelOf(n) = Channel(n)\n{binding}\nprint(\"created\")\nsend(ch, [1, 2, 3])\n"
        );
        let output = run_arc(&source)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "{label}: a non-positive capacity must be refused. stdout={stdout:?} stderr={stderr}"
        );
        assert_eq!(
            stdout, "",
            "{label}: the program reached its first print, so the handle was \
             created and only its USE was refused"
        );
        // A truthful error, not a bare abort: the capacity that was asked for
        // and the code the runtime answered, so the reader knows which of the
        // two refusals happened.
        assert!(
            stderr.contains("FATAL") && stderr.contains(named) && stderr.contains("code -1"),
            "{label}: the refusal must name what was asked for and why. stderr={stderr}"
        );
    }
    Ok(())
}

// The other half of the same rule, so a "fix" that refuses every capacity is
// not mistaken for one that refuses the wrong ones. One is the smallest
// POSITIVE capacity and must work end to end, value intact.
#[test]
fn the_smallest_positive_capacity_still_carries_a_value() -> io::Result<()> {
    let source = r"
let ch = Channel(1)
send(ch, [1, 2, 3])
let back = recv(ch)
print(toString(listLength(back)))
";

    let output = run_arc(source)?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr={stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
    assert!(
        stderr.contains("[osp-arc] exit: 0 live objects"),
        "a received value must be released like any other: {}",
        arc_census(&stderr)
    );
    Ok(())
}
