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

/// The fixed wire position encoding advertised and used throughout.
/// Implements [LSP-ENCODING].
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
/// Implements [LSP-TRANSPORT].
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
/// Implements [LSP-LIFECYCLE].
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
                let doc = DocumentUri::new(uri);
                engine.vfs().close(&doc);
                engine.project_cache.opened(doc.as_str(), false);
                publish_siblings(engine, bus, &doc).await;
            }
        }
        "workspace/didChangeWatchedFiles" => {
            for doc in engine.project_cache.documents() {
                publish(engine, bus, doc).await;
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
    engine.project_cache.opened(doc.as_str(), true);
    publish(engine, bus, doc.clone()).await;
    publish_siblings(engine, bus, &doc).await;
}

async fn did_change(engine: &OspreyEngine, bus: &lspkit_server::DiagnosticsBus, params: &Value) {
    let Some(uri) = wire::doc_uri(params) else {
        return;
    };
    let doc = DocumentUri::new(uri);
    apply_changes(engine.vfs(), &doc, params);
    engine.project_cache.opened(doc.as_str(), true);
    publish(engine, bus, doc.clone()).await;
    publish_siblings(engine, bus, &doc).await;
}

/// Live sibling changes can make a formerly redundant annotation necessary.
/// Refresh other open project files on open/change/close, so their diagnostics
/// agree with the current-program proof used by actions.
/// Implements [LSP-CODE-ACTIONS-ANNOTATIONS].
async fn publish_siblings(
    engine: &OspreyEngine,
    bus: &lspkit_server::DiagnosticsBus,
    changed: &DocumentUri,
) {
    for doc in engine.project_cache.documents() {
        if doc != *changed
            && engine
                .project_cache
                .affected(doc.as_str(), changed.as_str())
        {
            publish(engine, bus, doc).await;
        }
    }
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
/// Implements [LSP-DIAGNOSTICS].
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
/// Implements [LSP-CAPABILITIES].
fn build_dispatcher(engine: &OspreyEngine) -> Dispatcher {
    let dispatcher = Dispatcher::new();
    register(
        &dispatcher,
        engine,
        "textDocument/codeAction",
        |e, p, c| async move {
            let query = Query::CodeActions {
                uri: DocumentUri::new(wire::doc_uri(&p)?),
                range: wire::action_range(&p)?,
                only: wire::action_kinds(&p),
            };
            let actions = match answer(&e, query, c).await {
                Some(Report::CodeActions(actions)) => actions,
                _ => Vec::new(),
            };
            Some(result(wire::code_actions_result(&actions)))
        },
    );
    positional(
        &dispatcher,
        engine,
        "textDocument/hover",
        Query::Hover,
        hover_value,
    );
    positional(
        &dispatcher,
        engine,
        "textDocument/definition",
        Query::Definition,
        locations_value,
    );
    // Standard implementation-provider routing makes VS Code expose its native
    // context-menu action. [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS]
    positional(
        &dispatcher,
        engine,
        "textDocument/implementation",
        Query::Implementation,
        locations_value,
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
    positional(
        &dispatcher,
        engine,
        "textDocument/signatureHelp",
        Query::SignatureHelp,
        signature_value,
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
    positional(
        &dispatcher,
        engine,
        "textDocument/completion",
        Query::Completion,
        completion_value,
    );
    register(
        &dispatcher,
        engine,
        "textDocument/formatting",
        |e, p, _c| async move {
            let uri = wire::doc_uri(&p)?;
            let text = e.vfs().text(&DocumentUri::new(uri.clone()))?;
            let project = e.project_cache.view(&uri, e.vfs());
            let flavor = osprey_syntax::resolve_flavor(project.configured, &uri, &text).ok()?;
            let formatted = osprey_fmt::format_source(&text, flavor).ok()?;
            Some(result(wire::formatting_result(&formatted, &text, ENCODING)))
        },
    );
    dispatcher
}

/// Register a request whose whole handling is "read a position, ask the engine
/// one query, render the answer" — the shape every purely positional method
/// shares. The three things that differ are its arguments.
fn positional(
    dispatcher: &Dispatcher,
    engine: &OspreyEngine,
    method: &str,
    query: fn(At) -> Query,
    render: fn(Option<Report>) -> Value,
) {
    register(dispatcher, engine, method, move |e, p, c| async move {
        Some(result(render(answer(&e, query(at(&p)?), c).await)))
    });
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
mod tests;
