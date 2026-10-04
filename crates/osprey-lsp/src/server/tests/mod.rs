use super::*;
use crate::test_support::ADD_SRC as SRC;
use crate::wire::assert_at;
use lspkit_server::jsonrpc::read_message;
use serde_json::json;
use tokio::io::{duplex, DuplexStream};

const URI: &str = "file:///a.osp";
const ML_URI: &str = "file:///a.ospml";

/// An in-process LSP client driving [`serve`] over two duplex pipes: one for
/// requests (client -> server) and one for responses (server -> client).
struct Harness {
    to_server: DuplexStream,
    from_server: BufReader<DuplexStream>,
    engine: OspreyEngine,
    join: tokio::task::JoinHandle<Result<(), ServerError>>,
}

impl Harness {
    fn start() -> Self {
        let (client_writes, server_reads) = duplex(1 << 16);
        let (server_writes, client_reads) = duplex(1 << 16);
        let engine = OspreyEngine::new(Vfs::new(ENCODING));
        let writer: SharedWriter = Arc::new(MessageWriter::new(Box::new(server_writes)));
        let bus = lspkit_server::DiagnosticsBus::new();
        let attached = bus.attach(Arc::new(LspSink {
            writer: Arc::clone(&writer),
        }));
        assert_eq!(attached, 1, "exactly one diagnostics sink attached");
        let dispatcher = build_dispatcher(&engine);
        let engine_for_loop = engine.clone();
        let join = tokio::spawn(async move {
            let reader: BoxedReader = Box::new(server_reads);
            let mut reader = BufReader::new(reader);
            serve(&mut reader, &writer, &engine_for_loop, &bus, &dispatcher).await
        });
        Self {
            to_server: client_writes,
            from_server: BufReader::new(client_reads),
            engine,
            join,
        }
    }

    async fn send(&mut self, message: &Message) {
        let body = serde_json::to_vec(message).expect("serialize");
        self.send_raw(&body).await;
    }

    /// Frame and send arbitrary JSON bytes (for malformed/edge-case messages).
    async fn send_raw(&mut self, body: &[u8]) {
        use tokio::io::AsyncWriteExt as _;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.to_server
            .write_all(header.as_bytes())
            .await
            .expect("write header");
        self.to_server.write_all(body).await.expect("write body");
        self.to_server.flush().await.expect("flush");
    }

    async fn request(&mut self, id: i64, method: &str, params: Value) -> Message {
        self.send(&Message::request(RequestId::Number(id), method, params))
            .await;
        self.read_response_for(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) {
        self.send(&Message::notification(method, params)).await;
    }

    /// Open a `version: 1` document at [`URI`] with `text` and return the
    /// first published message (the diagnostics for the freshly-opened doc).
    async fn open(&mut self, text: &str) -> Message {
        self.open_at(URI, text).await
    }

    async fn open_at(&mut self, uri: &str, text: &str) -> Message {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "version": 1, "text": text } }),
        )
        .await;
        self.read_message().await
    }

    /// Send `method` with `params` and assert the reply is a null result
    /// (neither `result` nor `error`) — the empty-result short-circuit path.
    async fn assert_null_request(&mut self, id: i64, method: &str, params: Value) {
        let reply = self.request(id, method, params).await;
        assert!(is_null_result(&reply), "{method}: {reply:?}");
    }

    /// Prove the read/route loop is still serving after a benign edge-case
    /// input: an `initialize` must come back with a result. `why` annotates
    /// the survival the test is asserting.
    async fn assert_still_serving(&mut self, id: i64, why: &str) {
        let init = self.request(id, "initialize", json!({})).await;
        assert!(init.result.is_some(), "{why}");
    }

    /// Read messages until the response carrying `id` arrives, returning it.
    /// Notifications (e.g. `publishDiagnostics`) encountered first are dropped.
    async fn read_response_for(&mut self, id: i64) -> Message {
        loop {
            let message = read_message(&mut self.from_server).await.expect("read");
            if message.id == Some(RequestId::Number(id)) {
                return message;
            }
        }
    }

    async fn read_message(&mut self) -> Message {
        read_message(&mut self.from_server).await.expect("read")
    }

    async fn shutdown_and_exit(mut self) {
        self.notify("exit", Value::Null).await;
        let outcome = self.join.await.expect("join");
        assert!(outcome.is_ok(), "serve exited cleanly: {outcome:?}");
    }
}

mod project_features;
mod project_tests;

async fn assert_refreshed(h: &mut Harness, uri: &str, expected: &Value) {
    let notification = tokio::time::timeout(std::time::Duration::from_secs(3), h.read_message())
        .await
        .expect("dependent diagnostics must be republished");
    let params = notification.params.expect("diagnostics notification");
    assert_eq!(
        notification.method.as_deref(),
        Some("textDocument/publishDiagnostics")
    );
    assert_at(&params, "/uri", uri);
    assert_eq!(params.get("diagnostics"), Some(expected));
}

fn text_doc(uri: &str) -> Value {
    json!({ "textDocument": { "uri": uri } })
}

fn position_params(uri: &str, line: u32, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    })
}

/// A JSON-RPC `result: null` round-trips as an absent `result` field (serde
/// maps JSON `null` to `Option::None`), so a "null result" reply is one that
/// carries neither a `result` nor an `error`.
fn is_null_result(message: &Message) -> bool {
    message.result.is_none() && message.error.is_none()
}

/// The array carried by a request's `result`, cloned out for inspection.
fn array_result(message: &Message, what: &str) -> Vec<Value> {
    message
        .result
        .as_ref()
        .unwrap_or_else(|| panic!("{what} result"))
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("{what} array"))
}

/// The string at `pointer` within `value`, owned for downstream `contains`
/// checks.
fn str_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("string at {pointer}"))
        .to_owned()
}

/// The string at `pointer` collected across every element of `items` (e.g.
/// every symbol `/name` or completion `/label`).
fn field_values<'a>(items: &'a [Value], pointer: &str) -> Vec<&'a str> {
    items
        .iter()
        .filter_map(|item| item.pointer(pointer).and_then(Value::as_str))
        .collect()
}

/// The first element of `items` whose `pointer` field equals `expected`.
fn find_by<'a>(items: &'a [Value], pointer: &str, expected: &str) -> &'a Value {
    items
        .iter()
        .find(|item| item.pointer(pointer) == Some(&Value::from(expected)))
        .unwrap_or_else(|| panic!("{expected} at {pointer}"))
}

mod features;
mod sync;
mod transport;

#[tokio::test]
async fn signature_quick_fix_is_versioned_and_removes_only_the_header() {
    let mut h = Harness::start();
    let source =
        "decorate : string -> string\ndecorate text = text + \"!\"\nprint (decorate \"a\")\n";
    let published = h.open_at(ML_URI, source).await;
    let params = published.params.expect("diagnostic notification");
    let diagnostics = params
        .get("diagnostics")
        .and_then(Value::as_array)
        .expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "one whole signature: {params}");
    assert_eq!(
        diagnostics.first().expect("one warning")["code"],
        "redundant-annotation"
    );
    let response = h
        .request(
            80,
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": ML_URI },
                "range": {"start":{"line":0,"character":4},"end":{"line":0,"character":4}},
                "context": {"diagnostics":diagnostics,"only":["quickfix"]}
            }),
        )
        .await;
    assert!(response.error.is_none(), "quickfix route: {response:?}");
    let actions = array_result(&response, "signature quickfix");
    assert_eq!(actions.len(), 1, "one deletion action: {actions:?}");
    let action = actions.first().expect("one action");
    assert_at(action, "/title", "Remove redundant type signature");
    assert_at(action, "/kind", "quickfix");
    assert_at(action, "/isPreferred", true);
    assert_at(action, "/edit/documentChanges/0/textDocument/version", 1);
    assert_at(action, "/edit/documentChanges/0/textDocument/uri", ML_URI);
    assert_at(action, "/edit/documentChanges/0/edits/0/newText", "");
    assert_at(
        action,
        "/edit/documentChanges/0/edits/0/range/start/line",
        0,
    );
    assert_at(action, "/edit/documentChanges/0/edits/0/range/end/line", 1);
    assert_eq!(action.get("diagnostics"), params.get("diagnostics"));
    h.shutdown_and_exit().await;
}
