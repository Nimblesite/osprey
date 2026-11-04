//! The Osprey HTTP client against a hostile raw-TCP origin, end to end.
//!
//! A plain `std::net` origin plays every attack byte for byte — dripping,
//! flooding, or holding the socket open — while a real Osprey program, run by
//! the real `osprey` binary, fetches each and prints the answer. Every line is
//! checked against [HTTP-CHUNKED-DECODE] and [HTTP-RESPONSE-FRAMING] for both
//! syntaxes and both memory backends, so a crash, partial body, hang, or
//! silent acceptance cannot pass unnoticed. [HTTP-RESPONSE-HANDLE]

#![cfg(unix)]

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[path = "cases/http_raw_attacks.rs"]
mod attacks;

use attacks::ATTACKS;

/// 64 MiB, `HTTP_MAX_RESPONSE_BYTES` in the C runtime.
const MIB64: usize = 64 * 1024 * 1024;
/// The default client timeout; hold-open cases get a patient client.
const DEFAULT_TIMEOUT_MS: i32 = 300;
const HOLD_OPEN_TIMEOUT_MS: i32 = 20_000;
/// Complete framing must answer long before the patient timeout.
const HOLD_OPEN_ANSWERED_WITHIN: Duration = Duration::from_secs(15);
/// A drip is cut at ten timeouts (~1 s) and an honest one lands ~1.6 s in.
const DRIP_FLOOR: Duration = Duration::from_millis(700);
const DRIP_CEILING: Duration = Duration::from_secs(4);
/// Whole-run ceiling: a compile, three 64 MiB exchanges, one drip cut at
/// ~1 s, one slow drip at ~1.6 s, ms for the rest; a waited-out 20 s timeout
/// also blows past it.
const RUN_CEILING: Duration = Duration::from_secs(45);

const CHUNK_TOO_BIG: &str = "http response: chunked body: chunk size exceeds the remaining body";
const CHUNK_BAD_SIZE: &str = "http response: chunked body: invalid chunk size";
const CHUNK_NO_CRLF: &str = "http response: chunked body: chunk data not followed by CRLF";
const CHUNK_BAD_TRAILER: &str = "http response: chunked body: invalid trailer";
const CHUNK_TRUNCATED: &str = "http response: chunked body: truncated";
const HEAD_TRUNCATED: &str = "http response: head: truncated";
const HEAD_MALFORMED: &str = "http response: head: malformed";
const HEAD_TE_10: &str = "http response: head: Transfer-Encoding on HTTP/1.0";
const BODY_TRUNCATED: &str = "http response: body: truncated";
const BODY_BAD_LENGTH: &str = "http response: body: invalid Content-Length";
const NOT_HTTP_1: &str = "http response: status line: not HTTP/1.x";
const NUL_BYTE: &str = "http response: NUL byte: a string cannot hold it";
const SIZE_OVER_LIMIT: &str = "http response: size: over the limit";
const READ_OVER_TIME: &str = "http response: read: over the time limit";

/// What the client must print: the framed answer, or the refusal's Error message.
enum Expected {
    Response(i32, Body),
    Refused(&'static str),
}

/// A body asserted as text, or — too big to print — as its length.
enum Body {
    Text(&'static str),
    Len(usize),
}

/// One hostile reply and everything the origin and the assertions need to know.
struct Attack {
    name: &'static str,
    reply: &'static [u8],
    fill_to: Option<usize>,
    hold_open: bool,
    drip_ms: u64,
    timeout_ms: i32,
    expect: Expected,
}

/// The default-flavor client. Recursion is the only loop ([BUILTIN-ITER]);
/// each attack gets its own client and timeout, prints one interpolated line,
/// and frees what it took. [HTTP-RESPONSE-HANDLE]
const OSP_CLIENT: &str = r#"fn timeoutFor(i) = match i {
@@ARMS@@    _ => 300
}

fn shown(index, status, body) = {
    let huge = length(body) > 1024
    match huge {
        true => "${index} ${status} len=${length(body)}"
        false => "${index} ${status} ${body}"
    }
}

fn answered(index, responseId) = match httpResponseBody(responseId) {
    Success { value } => {
        let status = httpResponseStatus(responseId)
        let _freed = httpResponseFree(responseId)
        shown(index, status, value)
    }
    Error { message } => {
        let _freed = httpResponseFree(responseId)
        "${index} Error ${message}"
    }
}

fn answer(index, response) = match response {
    Success { value } => answered(index, value)
    Error { message } => "${index} Error ${message}"
}

fn fetch(index) = {
    let client = (httpCreateClient("http://127.0.0.1:@@PORT@@", timeoutFor(index)) ?: -1)
    let printed = answer(index, httpGetResponse(client, "/a/${index}", ""))
    let _closed = httpCloseClient(client)
    print(printed)
}

fn run(index, last) = {
    let done = index > last
    match done {
        true => 0
        false => {
            fetch(index)
            run(index + 1, last)
        }
    }
}

fn main() = {
    handle Arith { overflow _ _ _ wrapped => wrapped }
    run(0, @@COUNT@@)
}
"#;

/// The ML twin: same attacks, same lines, minimal brackets.
const OSPML_CLIENT: &str = r#"timeoutFor i = match i
@@ARMS@@    _ => 300

shown (index, status, body) =
    huge = length(body) > 1024
    match huge
        true => "${index} ${status} len=${length(body)}"
        false => "${index} ${status} ${body}"

answered (index, responseId) = match httpResponseBody responseId
    Success value =>
        status = httpResponseStatus responseId
        _freed = httpResponseFree responseId
        shown (index, status, value)
    Error message =>
        _freed = httpResponseFree responseId
        "${index} Error ${message}"

answer (index, response) = match response
    Success value => answered (index, value)
    Error message => "${index} Error ${message}"

fetch index =
    client = httpCreateClient "http://127.0.0.1:@@PORT@@" (timeoutFor index) ?: -1
    printed = answer (index, httpGetResponse client "/a/${index}" "")
    _closed = httpCloseClient client
    print printed

run (index, last) =
    done = index > last
    match done
        true => 0
        false =>
            fetch index
            run (index + 1, last)

main () =
    handle Arith
        overflow _ _ _ wrapped => wrapped
    run (0, @@COUNT@@)
"#;

/// The client for one syntax, with port, count and timeout arms substituted.
fn client_program(syntax: &str, port: u16, table: &[Attack]) -> String {
    let template = match syntax {
        "ospml" => OSPML_CLIENT,
        _ => OSP_CLIENT,
    };
    template
        .replace("@@ARMS@@", &timeout_arms(table))
        .replace("@@PORT@@", &port.to_string())
        .replace("@@COUNT@@", &(table.len() - 1).to_string())
}

/// Only attacks without the default timeout need an arm; the rest take `_`.
fn timeout_arms(table: &[Attack]) -> String {
    let mut arms = String::new();
    for (index, attack) in table.iter().enumerate() {
        if attack.timeout_ms != DEFAULT_TIMEOUT_MS {
            let _ = writeln!(arms, "    {index} => {}", attack.timeout_ms);
        }
    }
    arms
}

/// The exact line the client must print for one attack: the framed answer as `<index> <status> <body>` (a huge body as its length), or `<index> Error <message>` for a refusal.
fn expected_line(index: usize, expect: &Expected) -> String {
    match expect {
        Expected::Response(status, Body::Text(body)) => format!("{index} {status} {body}"),
        Expected::Response(status, Body::Len(len)) => format!("{index} {status} len={len}"),
        Expected::Refused(message) => format!("{index} Error {message}"),
    }
}

/// The hostile origin: a stop flag, and the per-attack connection durations.
struct Origin {
    stop_flag: Arc<AtomicBool>,
    durations: Arc<Mutex<Vec<Duration>>>,
}
/// Bind and accept; every connection is its own thread. Bind failure
/// answers `None`, which the caller asserts on.
fn spawn_origin(table: &'static [Attack]) -> Option<(Origin, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let durations = Arc::new(Mutex::new(vec![Duration::ZERO; table.len()]));
    let origin = Origin {
        stop_flag: Arc::clone(&stop),
        durations: Arc::clone(&durations),
    };
    let _ = thread::spawn(move || serve(&listener, table, &stop, &durations));
    Some((origin, port))
}

/// Accept until told to stop; a refused connection is never an error here.
fn serve(
    listener: &TcpListener,
    table: &'static [Attack],
    stop: &Arc<AtomicBool>,
    durations: &Arc<Mutex<Vec<Duration>>>,
) {
    let _ = listener.set_nonblocking(true);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let durations = Arc::clone(durations);
                let _ = thread::spawn(move || answer_connection(stream, table, &durations));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

/// Record how long attack `index` kept its connection.
fn record(durations: &Arc<Mutex<Vec<Duration>>>, index: usize, took: Duration) {
    if let Ok(mut slots) = durations.lock() {
        if let Some(slot) = slots.get_mut(index) {
            *slot = took;
        }
    }
}

/// One connection's thread: play its attack, then record its lifetime.
fn answer_connection(
    stream: TcpStream,
    table: &'static [Attack],
    durations: &Arc<Mutex<Vec<Duration>>>,
) {
    if let Some((index, took)) = play_attack(stream, table) {
        record(durations, index, took);
    }
}

/// Play one attack: answer `/a/<index>` with its bytes, hold or close, and
/// report the attack's index and the connection's lifetime. The stream may
/// inherit the listener's non-blocking mode; the script needs blocking I/O.
fn play_attack(mut stream: TcpStream, table: &'static [Attack]) -> Option<(usize, Duration)> {
    let _ = stream.set_nonblocking(false);
    let index = read_attack_index(&mut stream)?;
    let attack = table.get(index)?;
    let started = Instant::now();
    send_reply(&mut stream, attack);
    if attack.hold_open {
        hold_until_closed(&mut stream);
    }
    Some((index, started.elapsed()))
}

/// Read the request head and take the attack index from its `/a/<index>` path.
fn read_attack_index(stream: &mut TcpStream) -> Option<usize> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut head = Vec::new();
    let mut chunk = [0u8; 512];
    while head.len() < 8192 && !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => head.extend_from_slice(chunk.get(..read)?),
        }
    }
    let head = String::from_utf8_lossy(&head);
    head.split_whitespace()
        .nth(1)?
        .strip_prefix("/a/")?
        .parse()
        .ok()
}

/// Write the attack's bytes, optionally one at a time, then any filler; a
/// write error means the client has hung up.
fn send_reply(stream: &mut TcpStream, attack: &Attack) {
    if attack.drip_ms == 0 {
        let _ = stream.write_all(attack.reply);
    } else {
        for byte in attack.reply {
            if stream.write_all(std::slice::from_ref(byte)).is_err() {
                return; // the client has its refusal and left
            }
            thread::sleep(Duration::from_millis(attack.drip_ms));
        }
    }
    if let Some(total) = attack.fill_to {
        fill_with(stream, attack.reply.len(), total);
    }
}

/// The size-limit filler: static, so nothing huge lives on the stack.
const FILLER: &[u8] = &[b'x'; 65536];

/// Pad the reply with filler to a total size; a failed write means the client hung up.
fn fill_with(stream: &mut TcpStream, mut sent: usize, total: usize) {
    while sent < total {
        let take = (total - sent).min(FILLER.len());
        let Some(chunk) = FILLER.get(..take) else {
            break;
        };
        if stream.write_all(chunk).is_err() {
            break;
        }
        sent += take;
    }
}

/// Hold the socket open until the client disconnects.
fn hold_until_closed(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut sink = [0u8; 64];
    let _ = stream.read(&mut sink);
}

/// Repo root: `crates/osprey-cli` -> `../..`, where the C runtime archives live.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Write the client to a unique temp dir; the extension carries the flavor.
fn write_client(source: &str, syntax: &str, memory: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "osprey_http_raw_{syntax}_{memory}_{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("client.{syntax}"));
    let _ = std::fs::write(&path, source);
    path
}

/// Run it with the real binary from the repo root; a spawn failure
/// surfaces as a code-less result.
fn run_client(source: &str, syntax: &str, memory: &str) -> (Option<i32>, String, String, Duration) {
    let started = Instant::now();
    let client = write_client(source, syntax, memory);
    let spawned = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&client)
        .args(["--run", "--quiet", &format!("--memory={memory}")])
        .current_dir(repo_root())
        .output();
    let (code, stdout, stderr) = match spawned {
        Ok(output) => (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ),
        Err(error) => (None, String::new(), format!("spawn failed: {error}")),
    };
    let took = started.elapsed();
    if let Some(dir) = client.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
    (code, stdout, stderr, took)
}

/// One printed line against its attack's spec answer, naming the attack.
fn assert_line(index: usize, line: &str, attack: &Attack) {
    assert_eq!(
        line,
        expected_line(index, &attack.expect),
        "attack {index} ({}) answered wrongly",
        attack.name
    );
}

/// Assert one whole run: a clean exit, the run inside its ceiling, and every
/// line exactly as its attack's spec line demands.
fn assert_run(table: &[Attack], code: Option<i32>, stdout: &str, stderr: &str, elapsed: Duration) {
    assert_eq!(code, Some(0), "the client must exit 0; stderr:\n{stderr}");
    assert!(
        elapsed < RUN_CEILING,
        "the run took {elapsed:?}, over the {RUN_CEILING:?} ceiling"
    );
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        table.len(),
        "one line per attack; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    for (index, (line, attack)) in lines.iter().zip(table.iter()).enumerate() {
        assert_line(index, line, attack);
    }
}

/// Wait for every connection to record; a never-filled slot fails as 0 ns.
fn await_durations(origin: &Origin) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let Ok(slots) = origin.durations.lock() else {
            return;
        };
        if slots.iter().all(|took| !took.is_zero()) {
            return;
        }
        drop(slots);
        thread::sleep(Duration::from_millis(10));
    }
}

/// One attack's origin-side duration against its bounds.
fn assert_prompt(took: Option<&Duration>, index: usize, attack: &Attack) {
    let Some(took) = took else { return };
    if attack.hold_open && attack.timeout_ms == HOLD_OPEN_TIMEOUT_MS {
        assert!(
            *took < HOLD_OPEN_ANSWERED_WITHIN,
            "attack {index} ({}) answered in {took:?}; framing must end the read long before the {HOLD_OPEN_TIMEOUT_MS} ms timeout",
            attack.name
        );
    }
    if attack.drip_ms > 0 {
        assert!(
            *took > DRIP_FLOOR && *took < DRIP_CEILING,
            "attack {index} ({}) took {took:?}, outside the drip bounds",
            attack.name
        );
    }
}

/// Origin-side evidence: a held socket answered well inside the patient
/// timeout, and a drip neither cut early nor dragging on.
fn assert_durations(origin: &Origin, table: &[Attack]) {
    await_durations(origin);
    let Ok(slots) = origin.durations.lock() else {
        return;
    };
    for (index, attack) in table.iter().enumerate() {
        assert_prompt(slots.get(index), index, attack);
    }
}

/// One full pass: origin up, program generated and run, everything asserted.
fn exercise(memory: &str, syntax: &str) {
    let spawned = spawn_origin(ATTACKS);
    assert!(spawned.is_some(), "the hostile origin could not bind");
    let Some((origin, port)) = spawned else {
        return;
    };
    let source = client_program(syntax, port, ATTACKS);
    let (code, stdout, stderr, elapsed) = run_client(&source, syntax, memory);
    assert_run(ATTACKS, code, &stdout, &stderr, elapsed);
    assert_durations(&origin, ATTACKS);
    origin.stop_flag.store(true, Ordering::Relaxed);
}

#[test]
fn hostile_origin_answers_in_default_syntax_under_gc() {
    exercise("gc", "osp");
}

#[test]
fn hostile_origin_answers_in_ml_syntax_under_gc() {
    exercise("gc", "ospml");
}

#[test]
fn hostile_origin_answers_in_default_syntax_under_arc() {
    exercise("arc", "osp");
}

#[test]
fn hostile_origin_answers_in_ml_syntax_under_arc() {
    exercise("arc", "ospml");
}
