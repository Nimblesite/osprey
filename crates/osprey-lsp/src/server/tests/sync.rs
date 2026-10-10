use super::*;

#[tokio::test]
async fn initialize_and_shutdown_round_trip_with_capabilities() {
    // Pins the framed request path in [LSP-TRANSPORT], [LSP-LIFECYCLE],
    // [LSP-CAPABILITIES], and the fixed UTF-16 wire choice in
    // [LSP-ENCODING].
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
async fn did_open_publishes_warnings_then_errors_on_change() {
    // Document synchronization drives compiler diagnostics.
    // [LSP-LIFECYCLE], [LSP-DIAGNOSTICS]
    let mut h = Harness::start();
    let opened = h.open(SRC).await;
    assert_eq!(
        opened.method.as_deref(),
        Some("textDocument/publishDiagnostics")
    );
    let params = opened.params.expect("diagnostics params");
    assert_at(&params, "/uri", URI);
    let expected: Vec<_> = [
        "redundant type annotation on parameter `a` of `add`: inference derives `int` without it",
        "redundant type annotation on parameter `b` of `add`: inference derives `int` without it",
        "redundant return type annotation on `add`: inference derives `int` without it",
    ]
    .into_iter()
    .zip([(8, 13), (16, 21), (23, 29)])
    .map(|(message, (start, end))| {
        json!({
            "code": "redundant-annotation",
            "message": message,
            "range": {
                "start": { "line": 0, "character": start },
                "end": { "line": 0, "character": end }
            },
            "severity": 2,
            "source": "osprey"
        })
    })
    .collect();
    assert_eq!(
        params.pointer("/diagnostics").and_then(Value::as_array),
        Some(&expected),
        "redundant annotations publish precise warnings"
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
    // Incremental sync and close semantics from [LSP-LIFECYCLE].
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
