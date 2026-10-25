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

use live_fixture::{
    listed_samples, listed_session, Fixture, Flavor, HOT_FUNCTION, POLL, RATE_HZ,
    RECORDING_DEADLINE,
};

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

fn timely_shutdown(server: Server) -> Result<Value, std::sync::mpsc::RecvTimeoutError> {
    with_server_deadline(server, Duration::from_secs(3), |server| {
        let reply = server.request(2, "shutdown", &Value::Null);
        server.send(&json!({ "jsonrpc": "2.0", "method": "exit" }));
        assert!(server.child.wait().expect("server exits").success());
        reply
    })
}

fn busy_document_source() -> String {
    (0..1500).fold(String::new(), |mut text, i| {
        writeln!(text, "fn identity{i}(x: int) -> int = x").expect("write source");
        text
    })
}

fn change_flood(source: &str) -> String {
    (2..82).fold(String::new(), |mut frames, version| {
        let body = json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
            "textDocument": { "uri": "file:///tmp/osprey-overload.osp", "version": version },
            "contentChanges": [{ "text": source }]
        }})
        .to_string();
        write!(frames, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("frame changes");
        frames
    })
}

fn flooded_session(server: &mut Server) -> Value {
    let source = format!("{}\n//{}\n", busy_document_source(), "x".repeat(512 * 1024));
    server.send(&json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": { "uri": "file:///tmp/osprey-overload.osp", "languageId": "osprey", "version": 1, "text": source }
    }}));
    if let Err(error) = server.stdin.write_all(change_flood(&source).as_bytes()) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    let status = server.child.wait().expect("overloaded server exits");
    let mut stderr = String::new();
    let _ = server
        .child
        .stderr
        .as_mut()
        .expect("server stderr")
        .read_to_string(&mut stderr)
        .expect("read failure");
    json!({ "code": status.code(), "stderr": stderr })
}

#[test]
fn analysis_backlog_overload_is_explicit_and_bounded() {
    let mut server = Server::start_in(&std::env::temp_dir());
    let _ = server.request(1, "initialize", &json!({ "capabilities": {} }));
    let result = with_server_deadline(server, Duration::from_secs(10), flooded_session)
        .expect("the overload limit ends the session promptly");
    assert_eq!(result.get("code"), Some(&json!(1)), "{result}");
    assert!(
        result
            .get("stderr")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("analysis backlog limit")),
        "{result}"
    );
}

#[test]
fn shutdown_remains_responsive_while_an_open_document_is_analyzed() {
    let mut server = Server::start_in(&std::env::temp_dir());
    let _ = server.request(1, "initialize", &json!({ "capabilities": {} }));
    let source = busy_document_source();
    server.send(
        &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": { "uri": "file:///tmp/osprey-busy-shutdown.osp", "languageId": "osprey",
                "version": 1, "text": source }
        }}),
    );
    std::thread::sleep(Duration::from_millis(50));
    let reply = timely_shutdown(server).expect("shutdown responds while diagnostics are busy");
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
    let shutdown = server.request(5, "shutdown", &Value::Null);
    assert!(shutdown.get("error").is_none(), "{shutdown}");
    server.send(&json!({ "jsonrpc": "2.0", "method": "exit" }));
}
