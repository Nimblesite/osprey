//! The stdio LSP server loop.
//!
//! Built on `lspkit-server`: [`read_message`]/[`MessageWriter`] for framing, a
//! [`Dispatcher`] for request routing + cancellation, and a [`DiagnosticsBus`]
//! that fans diagnostics out to an LSP sink. Notifications mutate the shared
//! [`Vfs`]; every analysis goes through the [`OspreyEngine`].

use std::sync::Arc;

use async_trait::async_trait;
use lspkit::EngineApi;
use lspkit_server::diagnostics::{DiagnosticsBatch, DiagnosticsSink};
use lspkit_server::jsonrpc::{read_message, FramingError, MessageWriter};
use lspkit_server::{Dispatcher, HandlerResult, JsonRpcError, Message, RequestId};
use lspkit_vfs::{DocumentUri, DocumentVersion, PositionEncoding, Vfs};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio_util::sync::CancellationToken;

use crate::engine::OspreyEngine;
use crate::model::{At, Query, Report};
use crate::wire;

/// The position encoding the server negotiates and uses throughout.
const ENCODING: PositionEncoding = PositionEncoding::Utf16;
/// JSON-RPC "method not found".
const METHOD_NOT_FOUND: i32 = -32601;

/// A type-erased async output sink. Type erasure (rather than a generic writer)
/// keeps a single [`serve`] instantiation: the stdio server boxes `stdout`, and
/// tests box an in-memory pipe, so the same loop body is exercised by both.
type BoxedWriter = Box<dyn AsyncWrite + Unpin + Send>;

/// A shared, thread-safe message writer over the type-erased output.
type SharedWriter = Arc<MessageWriter<BoxedWriter>>;

/// A type-erased async input source, mirroring [`BoxedWriter`].
type BoxedReader = Box<dyn AsyncRead + Unpin + Send>;

/// Errors from running the server.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// A fatal transport failure.
    #[error(transparent)]
    Framing(#[from] FramingError),
}

/// Run the language server over stdin/stdout until the client disconnects or
/// sends `exit`.
///
/// # Errors
/// Returns [`ServerError::Framing`] on an unrecoverable transport failure.
pub async fn run_stdio() -> Result<(), ServerError> {
    let writer: SharedWriter = Arc::new(MessageWriter::new(Box::new(tokio::io::stdout())));
    let reader: BoxedReader = Box::new(tokio::io::stdin());
    serve_on(reader, &writer).await
}

/// Wire an engine, diagnostics bus and dispatcher onto `reader`/`writer`, then
/// run the read/route loop. Shared by [`run_stdio`] and the in-process tests.
async fn serve_on(reader: BoxedReader, writer: &SharedWriter) -> Result<(), ServerError> {
    let engine = OspreyEngine::new(Vfs::new(ENCODING));
    let bus = lspkit_server::DiagnosticsBus::new();
    let _ = bus.attach(Arc::new(LspSink {
        writer: Arc::clone(writer),
    }));
    let dispatcher = build_dispatcher(&engine);
    let mut reader = BufReader::new(reader);
    serve(&mut reader, writer, &engine, &bus, &dispatcher).await
}

/// The main read/route loop. Returns `Ok` on a clean disconnect or `exit`.
async fn serve(
    reader: &mut BufReader<BoxedReader>,
    writer: &SharedWriter,
    engine: &OspreyEngine,
    bus: &lspkit_server::DiagnosticsBus,
    dispatcher: &Dispatcher,
) -> Result<(), ServerError> {
    loop {
        let message = match read_message(reader).await {
            Ok(message) => message,
            Err(FramingError::Closed) => return Ok(()),
            Err(other) => return Err(other.into()),
        };
        let params = message.params.unwrap_or(Value::Null);
        match (message.id, message.method) {
            (Some(id), Some(method)) => {
                let reply = request(dispatcher, &id, &method, params).await;
                let _ = writer.write_message(&reply).await;
            }
            (None, Some(method)) => {
                if method == "exit" {
                    return Ok(());
                }
                notify(engine, bus, dispatcher, &method, params).await;
            }
            _ => {}
        }
    }
}

/// Answer a request: lifecycle methods inline, everything else via the
/// dispatcher (so cancellation tokens are tracked).
async fn request(dispatcher: &Dispatcher, id: &RequestId, method: &str, params: Value) -> Message {
    match method {
        "initialize" => Message::response(id.clone(), wire::initialize_result(ENCODING.as_str())),
        "shutdown" => Message::response(id.clone(), Value::Null),
        _ => match dispatcher.dispatch(id, method, params).await {
            Ok(HandlerResult::Ok(value)) => Message::response(id.clone(), value),
            Ok(HandlerResult::Err(error)) => Message::response_error(id.clone(), error),
            Ok(_) => Message::response(id.clone(), Value::Null),
            Err(_) => Message::response_error(
                id.clone(),
                JsonRpcError::new(METHOD_NOT_FOUND, format!("method not found: {method}")),
            ),
        },
    }
}

/// Handle a notification: document sync drives diagnostics; cancellation trips
/// the matching in-flight token.
async fn notify(
    engine: &OspreyEngine,
    bus: &lspkit_server::DiagnosticsBus,
    dispatcher: &Dispatcher,
    method: &str,
    params: Value,
) {
    match method {
        "textDocument/didOpen" => did_open(engine, bus, &params).await,
        "textDocument/didChange" => did_change(engine, bus, &params).await,
        "textDocument/didClose" => {
            if let Some(uri) = wire::doc_uri(&params) {
                engine.vfs().close(&DocumentUri::new(uri));
            }
        }
        "$/cancelRequest" => {
            if let Some(id) = cancel_id(&params) {
                dispatcher.cancel(&id);
            }
        }
        _ => {}
    }
}

async fn did_open(engine: &OspreyEngine, bus: &lspkit_server::DiagnosticsBus, params: &Value) {
    let (Some(uri), Some(text)) = (wire::doc_uri(params), wire::open_text(params)) else {
        return;
    };
    let doc = DocumentUri::new(uri);
    engine.vfs().open(
        doc.clone(),
        &text,
        DocumentVersion::new(wire::version(params)),
    );
    publish(engine, bus, doc).await;
}

async fn did_change(engine: &OspreyEngine, bus: &lspkit_server::DiagnosticsBus, params: &Value) {
    let Some(uri) = wire::doc_uri(params) else {
        return;
    };
    let doc = DocumentUri::new(uri);
    apply_changes(engine.vfs(), &doc, params);
    publish(engine, bus, doc).await;
}

/// Apply a `didChange`. A conformant client sends either a single full-document
/// replacement (no range) or a batch of incremental edits — never both. Handling
/// them as separate paths avoids the version collision that would arise from an
/// `open` (which stamps the version) followed by a `change` (which then rejects
/// the same version as stale and silently drops the edits).
fn apply_changes(vfs: &Vfs, doc: &DocumentUri, params: &Value) {
    let version = DocumentVersion::new(wire::version(params));
    let changes = wire::content_changes(params);
    // A full replacement takes precedence; use the last one the batch carries.
    if let Some(full) = changes.iter().rev().find_map(|c| c.as_ref().err()) {
        vfs.open(doc.clone(), full, version);
        return;
    }
    let edits: Vec<_> = changes.into_iter().filter_map(Result::ok).collect();
    if !edits.is_empty() {
        if let Err(error) = vfs.change(doc, &edits, version) {
            // Lost edits would desync diagnostics from the buffer; surface it.
            eprintln!("osprey lsp: didChange edit rejected: {error}");
        }
    }
}

/// Compute and broadcast diagnostics for `doc`.
async fn publish(engine: &OspreyEngine, bus: &lspkit_server::DiagnosticsBus, doc: DocumentUri) {
    let snapshot = engine
        .report(Query::Diagnostics(doc.clone()), CancellationToken::new())
        .await;
    if let Ok(snap) = snapshot {
        if let Report::Diagnostics(diagnostics) = snap.data {
            bus.publish(DiagnosticsBatch::new(doc, snap.generation, diagnostics))
                .await;
        }
    }
}

/// Register the feature request handlers, each capturing a clone of the engine.
fn build_dispatcher(engine: &OspreyEngine) -> Dispatcher {
    let dispatcher = Dispatcher::new();
    register(
        &dispatcher,
        engine,
        "textDocument/hover",
        |e, p, c| async move {
            Some(result(hover_value(
                answer(&e, Query::Hover(at(&p)?), c).await,
            )))
        },
    );
    register(
        &dispatcher,
        engine,
        "textDocument/definition",
        |e, p, c| async move {
            Some(result(locations_value(
                answer(&e, Query::Definition(at(&p)?), c).await,
            )))
        },
    );
    register(
        &dispatcher,
        engine,
        "textDocument/references",
        |e, p, c| async move {
            let query = Query::References {
                at: at(&p)?,
                include_declaration: wire::include_declaration(&p),
            };
            Some(result(locations_value(answer(&e, query, c).await)))
        },
    );
    register(
        &dispatcher,
        engine,
        "textDocument/signatureHelp",
        |e, p, c| async move {
            Some(result(signature_value(
                answer(&e, Query::SignatureHelp(at(&p)?), c).await,
            )))
        },
    );
    register(
        &dispatcher,
        engine,
        "textDocument/documentSymbol",
        |e, p, c| async move {
            let uri = DocumentUri::new(wire::doc_uri(&p)?);
            let text = e.vfs().text(&uri).unwrap_or_default();
            Some(result(symbols_value(
                answer(&e, Query::Symbols(uri), c).await,
                &text,
            )))
        },
    );
    register(
        &dispatcher,
        engine,
        "textDocument/completion",
        |e, p, c| async move {
            Some(result(completion_value(
                answer(&e, Query::Completion(at(&p)?), c).await,
            )))
        },
    );
    register(
        &dispatcher,
        engine,
        "textDocument/formatting",
        |e, p, _c| async move {
            let uri = wire::doc_uri(&p)?;
            let text = e.vfs().text(&DocumentUri::new(uri.clone()))?;
            let formatted = osprey_fmt::format_for_path(&uri, &text).ok()?;
            Some(result(wire::formatting_result(&formatted, &text, ENCODING)))
        },
    );
    dispatcher
}

/// Register one handler. The closure receives a cloned engine, the params, and
/// a cancellation token, and returns the JSON result.
fn register<F, Fut>(dispatcher: &Dispatcher, engine: &OspreyEngine, method: &str, handler: F)
where
    F: Fn(OspreyEngine, Value, CancellationToken) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = Option<HandlerResult>> + Send + 'static,
{
    let engine = engine.clone();
    dispatcher.register(method, move |params, cancel| {
        let engine = engine.clone();
        let handler = handler.clone();
        async move {
            handler(engine, params, cancel)
                .await
                .unwrap_or_else(empty_ok)
        }
    });
}

/// The `(uri, line, character)` target of a positional request.
fn at(params: &Value) -> Option<At> {
    let uri = wire::doc_uri(params)?;
    let (line, character) = wire::position(params)?;
    Some(At {
        uri: DocumentUri::new(uri),
        line,
        character,
    })
}

/// Run a query, returning the report or `None` if the engine is unavailable.
async fn answer(engine: &OspreyEngine, query: Query, cancel: CancellationToken) -> Option<Report> {
    engine.report(query, cancel).await.ok().map(|s| s.data)
}

fn result(value: Value) -> HandlerResult {
    HandlerResult::Ok(value)
}

fn empty_ok() -> HandlerResult {
    HandlerResult::Ok(Value::Null)
}

fn hover_value(report: Option<Report>) -> Value {
    match report {
        Some(Report::Hover(markdown)) => wire::hover_result(markdown),
        _ => Value::Null,
    }
}

fn locations_value(report: Option<Report>) -> Value {
    match report {
        Some(Report::Locations(locations)) => wire::locations_result(&locations),
        _ => Value::Array(Vec::new()),
    }
}

fn signature_value(report: Option<Report>) -> Value {
    match report {
        Some(Report::Signature(info)) => wire::signature_result(info),
        _ => Value::Null,
    }
}

fn symbols_value(report: Option<Report>, text: &str) -> Value {
    match report {
        Some(Report::Symbols(symbols)) => wire::symbols_result(&symbols, text, ENCODING),
        _ => Value::Array(Vec::new()),
    }
}

fn completion_value(report: Option<Report>) -> Value {
    match report {
        Some(Report::Completion(items)) => wire::completion_result(&items),
        _ => Value::Array(Vec::new()),
    }
}

/// The id targeted by a `$/cancelRequest`.
fn cancel_id(params: &Value) -> Option<RequestId> {
    let id = params.get("id")?;
    id.as_i64()
        .map(RequestId::Number)
        .or_else(|| id.as_str().map(|s| RequestId::String(s.to_owned())))
}

/// The LSP diagnostics sink: each batch becomes a `publishDiagnostics`
/// notification on the shared writer.
struct LspSink {
    writer: SharedWriter,
}

#[async_trait]
impl DiagnosticsSink for LspSink {
    async fn publish(&self, batch: DiagnosticsBatch) {
        let params = wire::publish_diagnostics(batch.uri.as_str(), &batch.diagnostics);
        let message = Message::notification("textDocument/publishDiagnostics", params);
        let _ = self.writer.write_message(&message).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::assert_at;
    use lspkit_server::jsonrpc::read_message;
    use serde_json::json;
    use tokio::io::{duplex, DuplexStream};

    const URI: &str = "file:///a.osp";

    /// A full source program exercising functions, a `let`, a type, and calls.
    const SRC: &str = "fn add(a: int, b: int) -> int = a + b\n\
                       let total = add(1, 2)\n";

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
            self.notify(
                "textDocument/didOpen",
                json!({ "textDocument": { "uri": URI, "version": 1, "text": text } }),
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

    #[tokio::test]
    async fn initialize_and_shutdown_round_trip_with_capabilities() {
        let mut h = Harness::start();
        let init = h.request(1, "initialize", json!({})).await;
        assert_eq!(init.id, Some(RequestId::Number(1)));
        let result = init.result.expect("initialize result");
        assert_at(&result, "/capabilities/positionEncoding", "utf-16");
        assert_at(&result, "/capabilities/hoverProvider", true);
        assert_at(&result, "/capabilities/definitionProvider", true);
        assert_at(&result, "/serverInfo/name", "osprey-lsp");
        let shutdown = h.request(2, "shutdown", Value::Null).await;
        assert_eq!(shutdown.id, Some(RequestId::Number(2)));
        assert!(is_null_result(&shutdown), "shutdown -> null: {shutdown:?}");
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn did_open_publishes_clean_diagnostics_then_errors_on_change() {
        let mut h = Harness::start();
        let clean = h.open(SRC).await;
        assert_eq!(
            clean.method.as_deref(),
            Some("textDocument/publishDiagnostics")
        );
        let params = clean.params.expect("diagnostics params");
        assert_at(&params, "/uri", URI);
        assert_eq!(
            params.pointer("/diagnostics").and_then(Value::as_array),
            Some(&Vec::new()),
            "clean program has no diagnostics"
        );
        // The engine now has the open document text.
        assert_eq!(
            h.engine.vfs().text(&DocumentUri::new(URI)).as_deref(),
            Some(SRC)
        );

        // A full-document replacement that no longer parses must publish an error.
        h.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": URI, "version": 2 },
                "contentChanges": [ { "text": "fn main( = \n" } ]
            }),
        )
        .await;
        let broken = h.read_message().await;
        let diags = broken
            .params
            .expect("params")
            .pointer("/diagnostics")
            .and_then(Value::as_array)
            .cloned()
            .expect("diagnostics array");
        let first = diags.first().expect("at least one diagnostic");
        assert_at(first, "/severity", 1);
        assert_at(first, "/source", "osprey");
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn incremental_change_and_close_update_the_vfs() {
        let mut h = Harness::start();
        let _open = h.open("let x = 1\n").await;

        // Incremental edit: insert a digit so the line becomes `let x = 12`.
        h.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": URI, "version": 2 },
                "contentChanges": [ {
                    "range": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 9 }
                    },
                    "text": "2"
                } ]
            }),
        )
        .await;
        let _changed = h.read_message().await;
        assert_eq!(
            h.engine.vfs().text(&DocumentUri::new(URI)).as_deref(),
            Some("let x = 12\n"),
            "incremental edit applied to the buffer"
        );

        h.notify("textDocument/didClose", text_doc(URI)).await;
        // The read/route loop is sequential: a reply to a following request proves
        // the preceding `didClose` notification has already been processed.
        let synced = h.request(1, "shutdown", Value::Null).await;
        assert!(is_null_result(&synced));
        assert!(
            h.engine.vfs().text(&DocumentUri::new(URI)).is_none(),
            "closed document is dropped from the vfs"
        );
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn hover_definition_references_over_open_document() {
        let mut h = Harness::start();
        let _diags = h.open(SRC).await;

        // Hover over `add` in its call on line 1.
        let hover = h
            .request(10, "textDocument/hover", position_params(URI, 1, 13))
            .await;
        let markdown = str_at(&hover.result.expect("hover result"), "/contents/value");
        assert!(
            markdown.contains("fn add(a: int, b: int) -> int"),
            "{markdown}"
        );

        // Definition of `add` from its use site lands on the declaration line 0.
        let def = h
            .request(11, "textDocument/definition", position_params(URI, 1, 13))
            .await;
        let locations = array_result(&def, "definition");
        assert_eq!(locations.len(), 1, "{locations:?}");
        let location = locations.first().expect("one location");
        assert_at(location, "/uri", URI);
        assert_at(location, "/range/start/line", 0);

        // References to `add` including its declaration: two occurrences.
        let refs = h
            .request(
                12,
                "textDocument/references",
                json!({
                    "textDocument": { "uri": URI },
                    "position": { "line": 0, "character": 3 },
                    "context": { "includeDeclaration": true }
                }),
            )
            .await;
        let ref_locs = array_result(&refs, "references");
        assert_eq!(ref_locs.len(), 2, "{ref_locs:?}");
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn symbols_completion_and_signature_help() {
        let mut h = Harness::start();
        let _diags = h.open(SRC).await;

        let symbols = h
            .request(20, "textDocument/documentSymbol", text_doc(URI))
            .await;
        let syms = array_result(&symbols, "symbols");
        let names = field_values(&syms, "/name");
        assert!(names.contains(&"add"), "{names:?}");
        assert!(names.contains(&"total"), "{names:?}");
        // `add` is a function (LSP SymbolKind 12) and its selection lands on the name.
        let add = find_by(&syms, "/name", "add");
        assert_at(add, "/kind", 12);
        assert_at(add, "/selectionRange/start/character", 3);

        // Completion is positional: line 2 is the empty line after both
        // declarations, i.e. declaration position, where every keyword is
        // legal. Implements [LSP-COMPLETION-CONTEXT].
        let completion = h
            .request(21, "textDocument/completion", position_params(URI, 2, 0))
            .await;
        let items = array_result(&completion, "completion");
        let labels = field_values(&items, "/label");
        assert!(labels.contains(&"fn"), "keyword completion present");
        assert!(labels.contains(&"add"), "declaration completion present");
        // The `fn` keyword carries a snippet insert text.
        let fn_item = find_by(&items, "/label", "fn");
        assert_at(fn_item, "/insertTextFormat", 2);

        // Signature help over the second argument of `add(1, 2)` on line 1.
        let sig = h
            .request(
                22,
                "textDocument/signatureHelp",
                position_params(URI, 1, 19),
            )
            .await;
        let sig_result = sig.result.expect("signature result");
        assert_at(&sig_result, "/activeParameter", 1);
        assert_at(&sig_result, "/signatures/0/parameters/0/label", "a: int");
        // ...and the same server, over a multi-declaration document, withholds
        // the declaration keywords mid-expression. This pins the wire path, not
        // just `context::classify`: the VSCode suite's equivalent assertion has
        // failed only when run after its siblings, which points at the client
        // rather than here. [LSP-COMPLETION-CONTEXT]
        let rich = "type Shape = Circle | Square\n\
                    fn area(r) = r * r\n\
                    fn perimeter(r) = r + r + r + r\n\
                    let radius = 5\n\
                    let m = print(radius)\n";
        h.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": URI, "version": 2 },
                "contentChanges": [{ "text": rich }]
            }),
        )
        .await;
        let _changed = h.read_message().await;
        // Line 4 is `let m = print(radius)`; character 8 sits just after `= `.
        let mid = h
            .request(23, "textDocument/completion", position_params(URI, 4, 8))
            .await;
        let mid_items = array_result(&mid, "completion");
        let mid_labels = field_values(&mid_items, "/label");
        for keyword in ["fn", "let", "type", "namespace"] {
            assert!(
                !mid_labels.contains(&keyword),
                "`{keyword}` offered in a value position: {mid_labels:?}"
            );
        }
        assert!(mid_labels.contains(&"match"), "{mid_labels:?}");
        assert!(mid_labels.contains(&"perimeter"), "{mid_labels:?}");
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn formatting_reindents_the_open_document() {
        let mut h = Harness::start();
        // A messy-but-valid Default buffer reindents to four-space blocks.
        let _diags = h.open("fn main() = {\nprint(1)\n}\n").await;
        let resp = h
            .request(90, "textDocument/formatting", text_doc(URI))
            .await;
        let edits = array_result(&resp, "formatting");
        let first = edits.first().expect("one formatting edit");
        assert_at(first, "/newText", "fn main() = {\n    print(1)\n}\n");

        // Once formatted, a second request reports no edits.
        let _again = h.open("fn main() = {\n    print(1)\n}\n").await;
        let resp2 = h
            .request(91, "textDocument/formatting", text_doc(URI))
            .await;
        assert_eq!(resp2.result, Some(Value::Array(Vec::new())));
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn positional_requests_return_empty_when_no_symbol_present() {
        let mut h = Harness::start();
        let _diags = h.open(SRC).await;

        // Hover over the `=` sign on line 1 (`let total = add(1, 2)`) -> null.
        h.assert_null_request(30, "textDocument/hover", position_params(URI, 1, 10))
            .await;

        // Definition over the same non-identifier position -> empty array.
        let def = h
            .request(31, "textDocument/definition", position_params(URI, 1, 10))
            .await;
        assert_eq!(def.result, Some(Value::Array(Vec::new())));

        // Signature help with the cursor before any open call -> null.
        h.assert_null_request(32, "textDocument/signatureHelp", position_params(URI, 0, 0))
            .await;
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn unknown_method_yields_method_not_found_error() {
        let mut h = Harness::start();
        let reply = h.request(40, "textDocument/foobar", json!({})).await;
        assert!(reply.result.is_none());
        let error = reply.error.expect("error response");
        assert_eq!(error.code, METHOD_NOT_FOUND);
        assert!(error.message.contains("textDocument/foobar"), "{error:?}");
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn malformed_positional_params_resolve_to_empty_results() {
        let mut h = Harness::start();
        // With no `textDocument`/`position`, every positional handler hits the
        // `at(&p)?` None path and short-circuits to empty_ok (null): `hover`,
        // `documentSymbol`, and `completion` short-circuit on the missing
        // document; `definition`, `references`, and `signatureHelp` likewise.
        h.assert_null_request(50, "textDocument/hover", json!({}))
            .await;
        h.assert_null_request(51, "textDocument/documentSymbol", json!({}))
            .await;
        h.assert_null_request(52, "textDocument/completion", json!({}))
            .await;
        h.assert_null_request(53, "textDocument/definition", json!({}))
            .await;
        h.assert_null_request(54, "textDocument/references", json!({}))
            .await;
        h.assert_null_request(55, "textDocument/signatureHelp", json!({}))
            .await;
        // A request that carries `textDocument` but no `position` also fails
        // `at()` (no position) and resolves to the empty result.
        h.assert_null_request(56, "textDocument/hover", text_doc(URI))
            .await;
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn cancel_request_is_accepted_without_a_reply() {
        let mut h = Harness::start();
        // A cancel for an unknown id is a harmless no-op; the loop keeps serving.
        h.notify("$/cancelRequest", json!({ "id": 999 })).await;
        h.notify("$/cancelRequest", json!({ "id": "abc" })).await;
        // The server still answers subsequent requests.
        h.assert_still_serving(60, "loop survives unknown cancels")
            .await;
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn cancel_id_parses_numeric_and_string_forms() {
        assert_eq!(cancel_id(&json!({ "id": 7 })), Some(RequestId::Number(7)));
        assert_eq!(
            cancel_id(&json!({ "id": "tok" })),
            Some(RequestId::String("tok".to_owned()))
        );
        assert_eq!(cancel_id(&json!({})), None);
        assert_eq!(cancel_id(&json!({ "id": 1.5 })), None);
    }

    #[tokio::test]
    async fn unparented_notification_methods_are_ignored() {
        let mut h = Harness::start();
        // An unknown notification must not crash the loop or produce output.
        h.notify("workspace/didChangeConfiguration", json!({}))
            .await;
        h.notify("textDocument/didOpen", json!({})).await; // missing uri/text -> ignored
        h.assert_still_serving(70, "loop survives ignored notifications")
            .await;
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn response_shaped_messages_are_silently_ignored() {
        let mut h = Harness::start();
        // A message carrying an `id` but no `method` is a response, not a request:
        // the loop's catch-all arm drops it without replying.
        h.send(&Message::response(
            RequestId::Number(1),
            json!({ "echo": true }),
        ))
        .await;
        // A raw frame with neither id nor method is likewise dropped.
        h.send_raw(b"{\"jsonrpc\":\"2.0\"}").await;
        // The loop keeps serving afterwards.
        h.assert_still_serving(80, "loop survives stray response frames")
            .await;
        h.shutdown_and_exit().await;
    }

    #[tokio::test]
    async fn closed_reader_ends_the_loop_cleanly_via_serve_on() {
        let (client_writes, server_reads) = duplex(64);
        let (server_writes, _client_reads) = duplex(64);
        let writer: SharedWriter = Arc::new(MessageWriter::new(Box::new(server_writes)));
        // Drop the client's write half immediately: the reader sees EOF, so the
        // full `serve_on` wiring runs and the loop returns cleanly at once.
        drop(client_writes);
        let reader: BoxedReader = Box::new(server_reads);
        let outcome = serve_on(reader, &writer).await;
        assert!(outcome.is_ok(), "EOF is a clean disconnect: {outcome:?}");
    }

    #[tokio::test]
    async fn apply_changes_prefers_full_replacement_over_edits() {
        let vfs = Vfs::new(ENCODING);
        let doc = DocumentUri::new(URI);
        vfs.open(doc.clone(), "old\n", DocumentVersion::new(1));
        // A batch carrying both an edit and a full replacement: full wins.
        let params = json!({
            "textDocument": { "uri": URI, "version": 2 },
            "contentChanges": [
                { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } }, "text": "X" },
                { "text": "brand new\n" }
            ]
        });
        apply_changes(&vfs, &doc, &params);
        assert_eq!(vfs.text(&doc).as_deref(), Some("brand new\n"));
    }

    #[test]
    fn value_builders_map_matching_reports_and_fall_back_otherwise() {
        use crate::model::{CompletionItem, CompletionKind, Location, Report, SignatureInfo};

        // Hover: a Hover report renders markdown; anything else (or None) is null.
        assert_eq!(
            hover_value(Some(Report::Hover(Some("**x**".to_owned())))).pointer("/contents/kind"),
            Some(&Value::from("markdown"))
        );
        assert_eq!(hover_value(Some(Report::Hover(None))), Value::Null);
        assert_eq!(hover_value(None), Value::Null);
        assert_eq!(
            hover_value(Some(Report::Completion(Vec::new()))),
            Value::Null
        );

        // Locations: a Locations report renders an array; otherwise an empty array.
        let locs = Report::Locations(vec![Location {
            uri: "file:///a.osp".to_owned(),
            span: (0, 0, 0, 1),
        }]);
        assert_eq!(
            locations_value(Some(locs)).as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(locations_value(None), Value::Array(Vec::new()));

        // Signature: a Signature(Some) report renders; None / wrong variant -> null.
        let sig = Report::Signature(Some(SignatureInfo {
            label: "fn f()".to_owned(),
            parameters: Vec::new(),
            active_parameter: 0,
        }));
        assert_eq!(
            signature_value(Some(sig)).pointer("/activeSignature"),
            Some(&Value::from(0))
        );
        assert_eq!(signature_value(Some(Report::Signature(None))), Value::Null);
        assert_eq!(signature_value(None), Value::Null);

        // Symbols: a Symbols report renders against the document text; else empty.
        let symbols = crate::analysis::collect_symbols(
            &osprey_syntax::parse_program("fn g() -> int = 1\n").program,
        );
        let rendered = symbols_value(Some(Report::Symbols(symbols)), "fn g() -> int = 1\n");
        assert_eq!(rendered.pointer("/0/name"), Some(&Value::from("g")));
        assert_eq!(symbols_value(None, ""), Value::Array(Vec::new()));

        // Completion: a Completion report renders items; else an empty array.
        let items = Report::Completion(vec![CompletionItem {
            label: "fn".to_owned(),
            kind: CompletionKind::Keyword,
            detail: None,
            insert_text: None,
        }]);
        assert_eq!(
            completion_value(Some(items)).pointer("/0/label"),
            Some(&Value::from("fn"))
        );
        assert_eq!(completion_value(None), Value::Array(Vec::new()));
    }

    #[tokio::test]
    async fn request_maps_handler_outcomes_to_responses() {
        let dispatcher = Dispatcher::new();
        dispatcher.register("ok/method", |_p, _c| async {
            HandlerResult::Ok(json!({ "ok": true }))
        });
        dispatcher.register("err/method", |_p, _c| async {
            HandlerResult::Err(JsonRpcError::new(-32000, "boom"))
        });
        let id = RequestId::Number(1);

        // The success arm carries the handler's value through.
        let ok = request(&dispatcher, &id, "ok/method", Value::Null).await;
        assert_eq!(ok.result, Some(json!({ "ok": true })));
        assert!(ok.error.is_none());

        // The error arm surfaces the handler's JSON-RPC error.
        let err = request(&dispatcher, &id, "err/method", Value::Null).await;
        assert!(err.result.is_none());
        let error = err.error.expect("error");
        assert_eq!(error.code, -32000);
        assert_eq!(error.message, "boom");

        // An unregistered method yields method-not-found.
        let missing = request(&dispatcher, &id, "no/such", Value::Null).await;
        assert_eq!(missing.error.map(|e| e.code), Some(METHOD_NOT_FOUND));

        // The lifecycle methods are answered inline without the dispatcher.
        let init = request(&dispatcher, &id, "initialize", Value::Null).await;
        assert!(init.result.is_some());
        let shutdown = request(&dispatcher, &id, "shutdown", Value::Null).await;
        assert_eq!(shutdown.result, Some(Value::Null));
    }

    #[tokio::test]
    async fn apply_changes_rejects_a_stale_incremental_edit() {
        let vfs = Vfs::new(ENCODING);
        let doc = DocumentUri::new(URI);
        vfs.open(doc.clone(), "abc\n", DocumentVersion::new(5));
        // An incremental edit stamped with a stale version (<= stored) is dropped
        // by the vfs; `apply_changes` surfaces the rejection and the buffer holds.
        let stale = json!({
            "textDocument": { "uri": URI, "version": 2 },
            "contentChanges": [ {
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
                "text": "X"
            } ]
        });
        apply_changes(&vfs, &doc, &stale);
        assert_eq!(
            vfs.text(&doc).as_deref(),
            Some("abc\n"),
            "stale edit must not mutate the buffer"
        );
        // An empty change batch is a no-op (neither full replacement nor edits).
        let empty = json!({ "textDocument": { "uri": URI, "version": 6 }, "contentChanges": [] });
        apply_changes(&vfs, &doc, &empty);
        assert_eq!(vfs.text(&doc).as_deref(), Some("abc\n"));
    }
}
