//! The profiler through the real language server [PROF-LSP-HOST]: `osprey lsp`
//! is spawned over stdio, as the editor spawns it, and a running Osprey program
//! is listed, attached to and profiled with `osprey.lspProfiler.*` commands.
// The endpoint and its Unix socket are POSIX-only [PROF-ATTACH-ENDPOINT].
#![cfg(unix)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed setup step must fail the end-to-end test that needed it"
)]

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[path = "cases/live_fixture.rs"]
mod live_fixture;
#[path = "cases/nesting.rs"]
mod nesting;

use live_fixture::{
    listed_samples, listed_session, Fixture, Flavor, HOT_FUNCTION, POLL, RATE_HZ,
    RECORDING_DEADLINE,
};
use nesting::{nested_source, DEPTH_BOUNDARIES};

/// `osprey lsp` with its stdio, killed on drop.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    /// The server sees the fixture's `TMPDIR`, so it finds its endpoint where
    /// every program's endpoint lives: `$TMPDIR/osprey-diagnostics-<uid>`.
    fn start(fixture: &Fixture) -> Self {
        Self::start_in(&fixture.tmp)
    }

    fn start_in(directory: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .arg("lsp")
            .env("TMPDIR", directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("osprey lsp starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, message: &Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write");
        self.stdin.flush().expect("flush");
    }

    fn read(&mut self) -> Value {
        let mut length = 0;
        loop {
            let mut header = String::new();
            assert!(
                self.stdout.read_line(&mut header).expect("header") > 0,
                "the server closed its stdout"
            );
            match header.trim_end().strip_prefix("Content-Length: ") {
                Some(n) => length = n.parse().expect("length"),
                None if header.trim_end().is_empty() => break,
                None => {}
            }
        }
        let mut body = vec![0; length];
        self.stdout.read_exact(&mut body).expect("body");
        serde_json::from_slice(&body).expect("json")
    }

    /// Send a request and return its response, skipping notifications.
    fn request(&mut self, id: i64, method: &str, params: &Value) -> Value {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let message = self.read();
            if message.get("id") == Some(&json!(id)) {
                return message;
            }
        }
    }

    fn command(&mut self, id: i64, name: &str, params: &Value) -> Value {
        let command =
            json!({ "command": format!("osprey.lspProfiler.{name}"), "arguments": [params] });
        let reply = self.request(id, "workspace/executeCommand", &command);
        reply
            .get("result")
            .cloned()
            .unwrap_or_else(|| panic!("{name} failed: {reply}"))
    }

    /// Poll `cpu.list` until `session` has recorded `enough` samples.
    fn await_samples(&mut self, session: &Value, enough: i64) {
        let deadline = Instant::now() + RECORDING_DEADLINE;
        for id in 1_000.. {
            let list = self.command(id, "cpu.list", &json!({}));
            if listed_samples(listed_session(&list, session)) >= enough {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{enough} samples not recorded in {RECORDING_DEADLINE:?}: {list}"
            );
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

fn terminate_server(pid: u32) {
    assert!(Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("terminate unresponsive test server")
        .success());
}

fn with_server_deadline(
    mut server: Server,
    deadline: Duration,
    run: impl FnOnce(&mut Server) -> Value + Send + 'static,
) -> Result<Value, std::sync::mpsc::RecvTimeoutError> {
    let pid = server.child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let reply = run(&mut server);
        let _ = sender.send(reply);
    });
    let reply = receiver.recv_timeout(deadline);
    if matches!(reply, Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
        terminate_server(pid);
    }
    let _ = worker.join();
    reply
}

/// `shutdown` then `exit`, and the server leaves by itself, cleanly and on
/// time. Killing it instead can land while it writes its coverage profile, and
/// one truncated profile fails the whole CI coverage merge.
fn timely_shutdown(server: Server, id: i64) -> Result<Value, std::sync::mpsc::RecvTimeoutError> {
    with_server_deadline(server, Duration::from_secs(3), move |server| {
        let reply = server.request(id, "shutdown", &Value::Null);
        server.send(&json!({ "jsonrpc": "2.0", "method": "exit" }));
        assert!(server.child.wait().expect("server exits").success());
        reply
    })
}

fn busy_document_source(functions: usize) -> String {
    (0..functions).fold(String::new(), |mut text, i| {
        writeln!(text, "fn identity{i}(x: int) -> int = x").expect("write source");
        text
    })
}

/// Enough declarations that analysis is slower than reading the flood.
const FLOODED_FUNCTIONS: usize = 150;

/// Request ids of the flood: more than the analysis queue holds.
const FLOOD: std::ops::Range<i64> = 1000..1400;

/// More analysis requests than the queue holds, written at once. Edits no
/// longer wait behind analysis, so only requests can outrun it.
fn request_flood() -> String {
    FLOOD.fold(String::new(), |mut frames, id| {
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": "file:///tmp/osprey-overload.osp" } } })
        .to_string();
        write!(frames, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("frame requests");
        frames
    })
}

/// Floods requests at a busy server, then cancels them as an editor does when
/// the user moves on. Every request gets one reply: a result, the `-32800`
/// cancellation, or the `-32802` refusal a client retries.
fn flooded_session(server: &mut Server) -> Value {
    let source = format!(
        "{}\n//{}\n",
        busy_document_source(FLOODED_FUNCTIONS),
        "x".repeat(512 * 1024)
    );
    server.send(&json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": { "uri": "file:///tmp/osprey-overload.osp", "languageId": "osprey", "version": 1, "text": source }
    }}));
    server
        .stdin
        .write_all(request_flood().as_bytes())
        .expect("the server keeps reading");
    server.stdin.flush().expect("flush the flood");
    for id in FLOOD {
        server.send(
            &json!({ "jsonrpc": "2.0", "method": "$/cancelRequest", "params": { "id": id } }),
        );
    }
    let mut replies = std::collections::BTreeMap::new();
    while replies.len() < FLOOD.count() {
        let message = server.read();
        if let Some(id) = message.get("id").and_then(Value::as_i64) {
            let code = message.pointer("/error/code");
            let refused = code == Some(&json!(-32802));
            let answered = message.get("result").is_some() || code == Some(&json!(-32800));
            assert!(refused || answered, "{message}");
            assert!(replies.insert(id, refused).is_none(), "{id} answered twice");
        }
    }
    let shutdown = server.request(1, "shutdown", &Value::Null);
    server.send(&json!({ "jsonrpc": "2.0", "method": "exit" }));
    let status = server.child.wait().expect("server exits");
    json!({ "ids": replies.keys().collect::<Vec<_>>(), "refused": replies.values().filter(|r| **r).count(),
        "shutdown": shutdown, "clean": status.success() })
}

/// A full analysis queue refuses what does not fit with `ServerCancelled`, so
/// the editor retries; cancelled queued requests stop costing analysis; and the
/// session never ends (VS Code gives up on a
/// server that keeps restarting).
#[test]
fn analysis_backlog_overload_refuses_requests_and_keeps_the_session() {
    let mut server = Server::start_in(&std::env::temp_dir());
    let _ = server.request(1, "initialize", &json!({ "capabilities": {} }));
    let result = with_server_deadline(server, Duration::from_secs(10), flooded_session)
        .expect("a flooded server answers every request promptly");
    assert_eq!(
        result.get("ids"),
        Some(&json!(FLOOD.collect::<Vec<_>>())),
        "{result}"
    );
    assert!(
        result
            .get("refused")
            .and_then(Value::as_u64)
            .is_some_and(|n| n > 0),
        "{result}"
    );
    assert_eq!(
        result.pointer("/shutdown/result"),
        Some(&Value::Null),
        "{result}"
    );
    assert_eq!(result.get("clean"), Some(&json!(true)), "{result}");
}

#[test]
fn shutdown_remains_responsive_while_an_open_document_is_analyzed() {
    let mut server = Server::start_in(&std::env::temp_dir());
    let _ = server.request(1, "initialize", &json!({ "capabilities": {} }));
    let source = busy_document_source(1500);
    server.send(
        &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": { "uri": "file:///tmp/osprey-busy-shutdown.osp", "languageId": "osprey",
                "version": 1, "text": source }
        }}),
    );
    std::thread::sleep(Duration::from_millis(50));
    let reply = timely_shutdown(server, 2).expect("shutdown responds while diagnostics are busy");
    assert_eq!(reply.get("result"), Some(&Value::Null), "{reply}");
}

#[test]
fn the_language_server_lists_and_profiles_a_running_program() {
    lists_and_profiles(Flavor::Default, "lsp-cpu");
}

#[test]
fn the_language_server_lists_and_profiles_a_running_ml_program() {
    lists_and_profiles(Flavor::Ml, "lsp-cpu-ml");
}

/// `osprey lsp` finds the running `flavor` twin and attributes its hot
/// function to that twin's own source.
fn lists_and_profiles(flavor: Flavor, name: &str) {
    let fixture = Fixture::start(name, flavor);
    let socket = fixture.socket_dir.join(format!("{}.sock", fixture.pid));
    assert!(socket.exists(), "the program serves {}", socket.display());
    let mut server = Server::start(&fixture);
    let init = server.request(1, "initialize", &json!({ "capabilities": {} }));
    assert!(
        init.pointer("/result/capabilities/experimental/lspProfiler")
            .is_some(),
        "{init}"
    );
    let list = server.command(2, "processes", &json!({ "workspace_roots": [] }));
    let listed = list
        .get("processes")
        .and_then(Value::as_array)
        .and_then(|all| {
            all.iter()
                .find(|p| p.get("pid") == Some(&json!(fixture.pid)))
        })
        .unwrap_or_else(|| panic!("PID {} missing from {list}", fixture.pid));
    assert_eq!(listed.get("runtime"), Some(&json!("osprey")), "{listed}");
    let started = server.command(
        3,
        "cpu.start",
        &json!({ "pid": fixture.pid, "sample_rate": RATE_HZ }),
    );
    let session = json!({ "session_id": started.get("session_id") });
    server.await_samples(&session, 2 * RATE_HZ);
    let report = server.command(4, "cpu.stop", &session);
    let hot = report
        .get("hot_functions")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    assert!(
        hot.iter()
            .any(|f| f.get("name") == Some(&json!(HOT_FUNCTION))
                && f.get("file")
                    .and_then(Value::as_str)
                    .is_some_and(|file| file.ends_with(flavor.source()))),
        "{report}"
    );
    let lines = report
        .get("hot_lines")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    assert!(
        lines.iter().any(|l| l
            .get("file")
            .and_then(Value::as_str)
            .is_some_and(|file| file.ends_with(flavor.source()))
            && flavor
                .spin_lines()
                .contains(&l.get("line").and_then(Value::as_i64).unwrap_or(0))),
        "a hot line inside {HOT_FUNCTION}: {report}"
    );
    let shutdown = timely_shutdown(server, 5).expect("the server shuts down promptly");
    assert_eq!(shutdown.get("result"), Some(&Value::Null), "{shutdown}");
}

/// Send `method` with `params` and return the diagnostics then published for
/// `uri`, for `version` when one is named.
fn diagnostics_after(
    server: &mut Server,
    method: &str,
    params: &Value,
    uri: &str,
    version: Option<i64>,
) -> Vec<Value> {
    server.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    loop {
        let message = server.read();
        if message.get("method") == Some(&json!("textDocument/publishDiagnostics"))
            && message.pointer("/params/uri") == Some(&json!(uri))
            && version.is_none_or(|version| {
                message
                    .pointer("/params/version")
                    .is_none_or(|v| v == &json!(version))
            })
        {
            return message
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .cloned()
                .expect("diagnostics");
        }
    }
}

/// Open `text` as `uri` and return the diagnostics published for it.
fn published_diagnostics(server: &mut Server, uri: &str, text: &str) -> Vec<Value> {
    let params = json!({
        "textDocument": { "uri": uri, "languageId": "osprey", "version": 1, "text": text }
    });
    diagnostics_after(server, "textDocument/didOpen", &params, uri, None)
}

/// Exactly one diagnostic, the nesting limit, on the source's first line.
fn refused_for_nesting(diagnostics: &[Value]) -> bool {
    let [only] = diagnostics else { return false };
    only.pointer("/range/start/line") == Some(&json!(0))
        && only
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("nesting limit"))
}

/// [FAILURE-EXPLICIT]: the deepest source the syntax guard accepts fits the
/// editor's analysis worker, and one level deeper is a positioned diagnostic.
#[test]
fn editor_handles_depth_boundaries_without_aborting() {
    let mut server = Server::start_in(&std::env::temp_dir());
    let initialized = server.request(1, "initialize", &json!({}));
    assert!(initialized.get("result").is_some(), "{initialized}");
    for (shape, (extension, binding, open, close, deepest)) in DEPTH_BOUNDARIES.iter().enumerate() {
        for depth in [*deepest, deepest + 1] {
            let uri = format!("file:///depth_boundary_{shape}_{depth}.{extension}");
            let text = nested_source(binding, open, close, depth);
            let diagnostics = published_diagnostics(&mut server, &uri, &text);
            let refused = depth > *deepest;
            assert_eq!(
                refused_for_nesting(&diagnostics),
                refused,
                "{uri}: {diagnostics:?}"
            );
            assert_eq!(diagnostics.is_empty(), !refused, "{uri}: {diagnostics:?}");
        }
    }
    let shutdown = timely_shutdown(server, 2).expect("the server shuts down promptly");
    assert_eq!(shutdown.get("result"), Some(&Value::Null), "{shutdown}");
}

/// Send the whole new `text` of an open `uri` and return the diagnostics
/// published for that version.
fn edited_diagnostics(server: &mut Server, uri: &str, version: i64, text: &str) -> Vec<Value> {
    let params = json!({
        "textDocument": { "uri": uri, "version": version },
        "contentChanges": [{ "text": text }]
    });
    diagnostics_after(server, "textDocument/didChange", &params, uri, Some(version))
}

/// The codes and messages of every error (severity 1) in `diagnostics`.
fn errors_of(diagnostics: &[Value]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|item| item.get("severity") == Some(&json!(1)))
        .map(|item| format!("{}: {}", item["code"], item["message"]))
        .collect()
}

fn has_code(diagnostics: &[Value], code: &str) -> bool {
    diagnostics
        .iter()
        .any(|item| item.get("code") == Some(&json!(code)))
}

/// [TYPES-DISCIPLINE-SEAM] [PROOF-SOURCE-DISCIPLINE]: the editor checks a
/// disciplined module the way the CLI builds it. The checker erases its
/// theorems before assembly, so they are never reported as top-level
/// statements, and a broken proof is still an error the editor shows.
#[test]
fn editor_erases_disciplined_modules_like_the_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let disciplines = root.join("src/osprey/examples/disciplines");
    let mut server = Server::start_in(&std::env::temp_dir());
    let initialized = server.request(1, "initialize", &json!({}));
    assert!(initialized.get("result").is_some(), "{initialized}");
    let files = [
        "dependent_roundtrip/src/access.ospml",
        "dependent_roundtrip/src/main.ospml",
        "lean_roundtrip/src/tally.ospml",
        "lean_roundtrip/src/main.ospml",
    ];
    for file in files {
        let path = disciplines
            .join(file)
            .canonicalize()
            .expect("example exists");
        let text = std::fs::read_to_string(&path).expect("read example");
        let uri = format!("file://{}", path.display());
        let diagnostics = published_diagnostics(&mut server, &uri, &text);
        assert_eq!(errors_of(&diagnostics), Vec::<String>::new(), "{file}");
        assert!(
            !has_code(&diagnostics, "project-error"),
            "{file}: {diagnostics:?}"
        );
    }
    let access = disciplines
        .join("dependent_roundtrip/src/access.ospml")
        .canonicalize()
        .expect("access");
    let good = std::fs::read_to_string(&access).expect("read access");
    let uri = format!("file://{}", access.display());
    let opened = published_diagnostics(&mut server, &uri, &good);
    assert!(
        has_code(&opened, "unexampled-export"),
        "analysis completed: {opened:?}"
    );
    let unproved = good.replace(": Reader := read e", ": Reader := e");
    assert_ne!(unproved, good, "the mutation applies");
    let broken = edited_diagnostics(&mut server, &uri, 2, &unproved);
    let errors = errors_of(&broken);
    assert!(!errors.is_empty(), "a broken proof is an error: {broken:?}");
    assert!(
        errors.iter().all(|e| !e.contains("executable top-level")),
        "{errors:?}"
    );
    let restored = edited_diagnostics(&mut server, &uri, 3, &good);
    assert_eq!(errors_of(&restored), Vec::<String>::new());
    let shutdown = timely_shutdown(server, 2).expect("the server shuts down promptly");
    assert_eq!(shutdown.get("result"), Some(&Value::Null), "{shutdown}");
}
