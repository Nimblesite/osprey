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
    ]
    .into_iter()
    .zip([(8, 13), (16, 21)])
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
    // An ordinary request stays behind document updates; shutdown has its own
    // responsive transport path and can bypass busy diagnostics.
    let barrier = h.request(901, "initialize", Value::Null).await;
    assert!(
        barrier.error.is_none(),
        "ordered request succeeds: {barrier:?}"
    );
    let synced = h.request(1, "shutdown", Value::Null).await;
    assert!(is_null_result(&synced));
    assert!(
        h.engine.vfs().text(&DocumentUri::new(URI)).is_none(),
        "closed document is dropped from the vfs"
    );
    h.shutdown_and_exit().await;
}

/// Configured roots, warnings and unsaved buffers use the real framed server.
/// Implements [LSP-DIAGNOSTICS-SCOPE].
#[tokio::test]
async fn diagnostic_scope_initial_roots_unsaved_python_and_workspace_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("osprey scope roots {}", std::process::id()));
    let a = root.join("a");
    let b = root.join("b");
    for folder in [&a, &b] {
        std::fs::create_dir_all(folder)?;
    }
    let warning = a.join("warning.ospml");
    let python = b.join("bad.py");
    std::fs::write(&warning, "double : int -> int\ndouble x = x * 2\n")?;
    std::fs::write(
        &python,
        "# osprey: flavor=python\ndef broken():\n    return missing\n",
    )?;
    std::fs::write(a.join("tool.py"), "import sys\n")?;
    let warning_uri = crate::workspace::uri_of(&warning).ok_or("warning URI")?;
    let python_uri = crate::workspace::uri_of(&python).ok_or("python URI")?;
    let a_uri = crate::workspace::uri_of(&a).ok_or("a URI")?;
    let b_uri = crate::workspace::uri_of(&b).ok_or("b URI")?;
    let mut h = Harness::start();
    let _ = h.request(500, "initialize", with_progress(json!({"rootUri":a_uri,"initializationOptions":{"diagnostics":{"scope":"roots","roots":[a,b_uri]}}}))).await;
    h.notify("initialized", json!({})).await;
    let reports = settled(&mut h, 501).await;
    assert!(
        reports.iter().any(|p| p["uri"] == warning_uri
            && p.pointer("/diagnostics/0/code") == Some(&json!("redundant-annotation"))
            && p.pointer("/diagnostics/0/severity") == Some(&json!(2))),
        "unopened warning: {reports:?}"
    );
    assert!(
        reports.iter().any(|p| p["uri"] == python_uri
            && p.pointer("/diagnostics/0/code") == Some(&json!("type-error"))),
        "Python flavor: {reports:?}"
    );
    assert!(
        !reports.iter().any(|p| p["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with("tool.py"))),
        "CPython excluded"
    );
    let report = h.open_at(&warning_uri, "double x = x * 2\n").await;
    assert_eq!(
        diagnostics(&report),
        Some(&json!([])),
        "unsaved source wins"
    );
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":warning_uri,"type":2}]}),
    )
    .await;
    let reports = settled(&mut h, 502).await;
    assert!(
        !reports
            .iter()
            .any(|p| p["uri"] == warning_uri && p["diagnostics"] != json!([])),
        "disk cannot override dirty buffer"
    );
    h.notify("textDocument/didClose", text_doc(&warning_uri))
        .await;
    let reports = settled(&mut h, 503).await;
    assert!(
        reports.iter().any(|p| p["uri"] == warning_uri
            && p.pointer("/diagnostics/0/code") == Some(&json!("redundant-annotation"))),
        "close restores disk warning: {reports:?}"
    );
    std::fs::remove_file(&python)?;
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":python_uri,"type":3}]}),
    )
    .await;
    settles_cleared(&mut h, 504, &python_uri, "deleted finding clears").await;
    scope_disable_and_remove(&mut h, &warning_uri, &a_uri, &b_uri).await;
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

async fn scope_disable_and_remove(h: &mut Harness, warning_uri: &str, a_uri: &str, b_uri: &str) {
    h.notify(
        "workspace/didChangeConfiguration",
        json!({"settings":{"osprey":{"diagnostics":{"enabled":false}}}}),
    )
    .await;
    settles_cleared(h, 505, warning_uri, "disable clears findings").await;
    h.notify(
        "workspace/didChangeConfiguration",
        json!({"settings":{"osprey":{"diagnostics":{"scope":"repository"}}}}),
    )
    .await;
    let reports = settled(h, 506).await;
    assert!(
        reports
            .iter()
            .any(|p| p["uri"] == warning_uri
                && p.pointer("/diagnostics/0/severity") == Some(&json!(2))),
        "reenable restores warning"
    );
    h.notify(
        "workspace/didChangeWorkspaceFolders",
        json!({"event":{"removed":[{"uri":a_uri}],"added":[{"uri":b_uri}]}}),
    )
    .await;
    settles_cleared(h, 507, warning_uri, "removed workspace clears").await;
}

#[tokio::test]
async fn unreadable_closed_source_reports_error_instead_of_clean_file() {
    let root = std::env::temp_dir().join(format!("osprey unreadable {}.osp", std::process::id()));
    std::fs::create_dir_all(&root).expect("directory cannot be read as source");
    let engine = OspreyEngine::new(Vfs::new(ENCODING));
    let uri = DocumentUri::new(crate::workspace::uri_of(&root).expect("URI"));
    let report = engine
        .report(Query::Diagnostics(uri), CancellationToken::new())
        .await
        .expect("report");
    let Report::Diagnostics(diagnostics) = report.data else {
        panic!("expected diagnostics")
    };
    assert_eq!(
        diagnostics
            .first()
            .and_then(|diagnostic| diagnostic.code.as_deref()),
        Some("source-read-error")
    );
    std::fs::remove_dir(root).expect("cleanup");
}

/// Opening a repository subfolder must still check the whole repository.
#[tokio::test]
async fn repository_scope_expands_git_parent_and_initial_open_only_does_not_scan() {
    let root = std::env::temp_dir().join(format!("osprey git scope {}", std::process::id()));
    std::fs::create_dir_all(root.join(".git")).expect("repository marker");
    std::fs::create_dir_all(root.join("nested")).expect("workspace folder");
    let file = root.join("outside.osp");
    std::fs::write(&file, "fn outside() { missing }\n").expect("closed source");
    let file_uri = crate::workspace::uri_of(&file).expect("file URI");
    for (id, options, expect_error) in [
        (600, json!({}), true),
        (610, json!({"diagnostics":{"scope":"openFiles"}}), false),
    ] {
        let mut h = Harness::start();
        let _ = h.request(id, "initialize", with_progress(json!({"rootUri":crate::workspace::uri_of(&root.join("nested")), "initializationOptions":options}))).await;
        h.notify("initialized", json!({})).await;
        let reports = settled(&mut h, id + 1).await;
        assert_eq!(
            reports.iter().any(|p| p["uri"] == file_uri
                && p.pointer("/diagnostics/0/code") == Some(&json!("type-error"))),
            expect_error,
            "scope: {reports:?}"
        );
        h.shutdown_and_exit().await;
    }
    std::fs::remove_dir_all(root).expect("fixture cleanup");
}

const SWEPT_FILES: usize = 120;

/// A hover is answered while the repository sweep is still checking files, so
/// a large workspace never leaves the editor's hover on "Loading…"; the sweep
/// still publishes every file and reports its progress [LSP-DIAGNOSTICS-SCOPE].
#[tokio::test]
async fn requests_are_answered_before_the_workspace_sweep_finishes(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("osprey sweep {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    for n in 0..SWEPT_FILES {
        std::fs::write(root.join(format!("f{n}.osp")), SRC)?;
    }
    let opened = crate::workspace::uri_of(&root.join("f0.osp")).ok_or("opened URI")?;
    let folder = crate::workspace::uri_of(&root).ok_or("folder URI")?;
    let mut h = Harness::start();
    let _ = h
        .request(
            700,
            "initialize",
            with_progress(json!({"workspaceFolders":[{"uri":folder,"name":"repo"}]})),
        )
        .await;
    h.notify("initialized", json!({})).await;
    h.notify(
        "textDocument/didOpen",
        json!({"textDocument":{"uri":opened,"version":1,"text":SRC}}),
    )
    .await;
    h.send(&Message::request(
        RequestId::Number(701),
        "textDocument/hover",
        position_params(&opened, 0, 3),
    ))
    .await;
    let mut published = std::collections::BTreeSet::new();
    let (mut began, mut hover_at) = (false, None);
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(60), h.read_message())
            .await
            .map_err(|elapsed| format!("the sweep reports its end: {elapsed}"))?;
        began |= sweep_progress(&message, "begin");
        if message.id == Some(RequestId::Number(701)) {
            assert_at(
                message.result.as_ref().ok_or("hover result")?,
                "/contents/kind",
                "markdown",
            );
            hover_at = Some(published.len());
        }
        if let Some(uri) = message.params.as_ref().and_then(|p| p["uri"].as_str()) {
            let _ = published.insert(uri.to_string());
        }
        if sweep_progress(&message, "end") {
            break;
        }
    }
    assert!(began, "the sweep reports when it begins");
    let hover_at = hover_at.ok_or("hover answered before the sweep ended")?;
    assert!(
        hover_at < SWEPT_FILES,
        "hover waited for {hover_at} of {SWEPT_FILES} files"
    );
    assert_eq!(
        published.len(),
        SWEPT_FILES,
        "every workspace file is published"
    );
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}
