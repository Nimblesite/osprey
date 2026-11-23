use super::*;

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

/// Implements [LSP-LIFECYCLE]: unknown methods never leave a
/// cancellation entry behind in the server's real dispatcher.
#[tokio::test]
async fn unknown_methods_leave_no_request_in_flight() {
    let engine = OspreyEngine::new(Vfs::new(ENCODING));
    let dispatcher = build_dispatcher(&engine);
    let idle = format!("{dispatcher:?}");
    assert!(idle.contains("in_flight: 0"), "{idle}");
    for n in 0..32_i64 {
        let id = if n % 2 == 0 {
            RequestId::Number(n)
        } else {
            RequestId::String(format!("u{n}"))
        };
        let method = format!("osprey/unknown{n}");
        let reply = request(&dispatcher, &id, &method, json!({ "n": n })).await;
        assert!(reply.result.is_none(), "{method}");
        let error = reply.error.expect("method-not-found error");
        assert_eq!(error.code, METHOD_NOT_FOUND, "{method}");
        assert!(error.message.contains(&method), "{error:?}");
        // A late $/cancelRequest for the finished id must not insert an entry.
        dispatcher.cancel(&id);
        let now = format!("{dispatcher:?}");
        assert!(now.contains("in_flight: 0"), "after {method}: {now}");
    }
    // A known request still answers, and it too leaves nothing in flight.
    let hover = request(
        &dispatcher,
        &RequestId::Number(99),
        "textDocument/hover",
        json!({}),
    )
    .await;
    assert_eq!(hover.result, Some(Value::Null));
    let done = format!("{dispatcher:?}");
    assert!(done.contains("in_flight: 0"), "{done}");
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
    // Cancellation notification handling from [LSP-LIFECYCLE].
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

/// An open document at `URI` holding `text` at `version`.
fn opened_doc(text: &str, version: i32) -> (Vfs, DocumentUri) {
    let vfs = Vfs::new(ENCODING);
    let doc = DocumentUri::new(URI);
    vfs.open(doc.clone(), text, DocumentVersion::new(version));
    (vfs, doc)
}

/// One incremental `contentChanges` entry editing line 0, `start..end`.
fn edit_event(start: u32, end: u32, text: &str) -> Value {
    json!({
        "range": { "start": { "line": 0, "character": start }, "end": { "line": 0, "character": end } },
        "text": text
    })
}

#[tokio::test]
async fn apply_changes_prefers_full_replacement_over_edits() {
    let (vfs, doc) = opened_doc("old\n", 1);
    // A batch carrying both an edit and a full replacement: full wins.
    let params = json!({
        "textDocument": { "uri": URI, "version": 2 },
        "contentChanges": [edit_event(0, 1, "X"), { "text": "brand new\n" }]
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
        documentation: None,
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
        documentation: None,
        insert_text: None,
    }]);
    assert_eq!(
        completion_value(Some(items)).pointer("/items/0/label"),
        Some(&Value::from("fn"))
    );
    assert_eq!(
        completion_value(None).pointer("/items"),
        Some(&Value::Array(Vec::new()))
    );
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
    let (vfs, doc) = opened_doc("abc\n", 5);
    // An incremental edit stamped with a stale version (<= stored) is dropped
    // by the vfs; `apply_changes` surfaces the rejection and the buffer holds.
    let stale = json!({
        "textDocument": { "uri": URI, "version": 2 },
        "contentChanges": [edit_event(0, 1, "X")]
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

/// LSP `ServerCancelled`: the server declined the request; the client retries.
const SERVER_CANCELLED: i32 = -32802;

/// Hovers past the queue (256) and past what the reply pipe holds while the
/// client is still busy sending. A full queue used to end the session
/// ("analysis backlog limit reached"); VS Code then restarted the server,
/// and gave up after repeated failures. Every request must get exactly one
/// answer, a hover or `ServerCancelled`, and the session must stay up.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_request_flood_is_answered_and_never_ends_the_session() {
    const FLOOD: i64 = 2000;
    let mut h = Harness::start();
    let _diags = h.open(SRC).await;
    let ids = 100..100 + FLOOD;
    let to_server = &mut h.to_server;
    let from_server = &mut h.from_server;
    let writes = async {
        use tokio::io::AsyncWriteExt as _;
        for id in ids.clone() {
            let hover = Message::request(
                RequestId::Number(id),
                "textDocument/hover",
                position_params(URI, 0, 3),
            );
            let body = serde_json::to_vec(&hover).expect("serialize");
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            to_server
                .write_all(header.as_bytes())
                .await
                .expect("header");
            to_server.write_all(&body).await.expect("body");
        }
        to_server.flush().await.expect("flush");
    };
    let reads = async {
        // The client reads nothing until the server has had time to back up.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let mut answered = std::collections::BTreeMap::new();
        while answered.len() < ids.clone().count() {
            let message = read_message(&mut *from_server)
                .await
                .expect("the session ended under load");
            if let Some(RequestId::Number(id)) = message.id {
                let previous = answered.insert(id, message);
                assert!(previous.is_none(), "request {id} answered twice");
            }
        }
        answered
    };
    let ((), answered) = tokio::join!(writes, reads);
    let mut hovers = 0;
    for (id, reply) in &answered {
        assert!(ids.contains(id), "reply for unknown request {id}");
        match (&reply.result, &reply.error) {
            (Some(result), None) => {
                hovers += 1;
                assert!(result.get("contents").is_some(), "{id}: {result}");
            }
            (None, Some(error)) => assert_eq!(error.code, SERVER_CANCELLED, "{id}: {error:?}"),
            other => panic!("request {id} got neither a hover nor a refusal: {other:?}"),
        }
    }
    assert!(hovers > 0, "every hover was refused");
    h.assert_still_serving(9000, "the flood ended the session")
        .await;
    let hover = h
        .request(9001, "textDocument/hover", position_params(URI, 0, 3))
        .await;
    assert!(hover.result.is_some(), "hover after the flood: {hover:?}");
    h.shutdown_and_exit().await;
}
