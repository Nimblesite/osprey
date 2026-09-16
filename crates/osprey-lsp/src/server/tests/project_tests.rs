//! Project diagnostics through the real server transport.

use super::*;

#[tokio::test]
async fn sibling_syntax_errors_preserve_existing_type_errors() {
    let root = std::env::temp_dir().join(format!("osprey sibling error {}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("fixture directory");
    std::fs::write(root.join("osprey.toml"), "[project]\nname = \"errors\"\nsource_roots = [\"src\"]\ndefault_namespace = \"review\"\nentry = \"src/main.ospml\"\n").expect("manifest");
    let main = "print missingName\n";
    let helper = "suffix text = text + \"!\"\n";
    std::fs::write(root.join("src/main.ospml"), main).expect("entry");
    std::fs::write(root.join("src/helper.ospml"), helper).expect("helper");
    let uri = lspkit_server::uri::path_to_uri(&root.join("src/main.ospml")).expect("URI");
    let sibling = lspkit_server::uri::path_to_uri(&root.join("src/helper.ospml")).expect("URI");
    let mut h = Harness::start();
    let initial = h
        .open_at(&uri, main)
        .await
        .params
        .expect("initial diagnostics");
    assert_at(&initial, "/diagnostics/0/code", "type-error");
    let expected = initial.get("diagnostics").expect("type error").clone();
    let _ = h.open_at(&sibling, helper).await;
    assert_refreshed(&mut h, &uri, &expected).await;
    h.notify("textDocument/didChange", json!({"textDocument":{"uri":sibling,"version":2},"contentChanges":[{"text":"suffix text = ("}]})).await;
    let changed = h.read_message().await.params.expect("sibling diagnostics");
    assert_at(&changed, "/diagnostics/0/code", "syntax-error");
    assert_refreshed(&mut h, &uri, &expected).await;
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root).expect("remove fixture");
}

#[tokio::test]
async fn sibling_open_change_and_close_refresh_signature_warnings() {
    let root = std::env::temp_dir().join(format!("osprey live diagnostics {}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("fixture directory");
    std::fs::write(root.join("osprey.toml"), "[project]\nname = \"warnings\"\nsource_roots = [\"src\"]\ndefault_namespace = \"review\"\nentry = \"src/main.ospml\"\n").expect("manifest");
    let main = "decorate : string -> string\ndecorate text = suffix text\nprint (decorate \"x\")\n";
    let helper = "suffix text = text + \"!\"\n";
    std::fs::write(root.join("src/main.ospml"), main).expect("entry");
    std::fs::write(root.join("src/helper.ospml"), helper).expect("helper");
    let uri = lspkit_server::uri::path_to_uri(&root.join("src/main.ospml")).expect("URI");
    let sibling = lspkit_server::uri::path_to_uri(&root.join("src/helper.ospml")).expect("URI");
    let mut h = Harness::start();
    let initial = h
        .open_at(&uri, main)
        .await
        .params
        .expect("initial diagnostics");
    let expected = initial.get("diagnostics").expect("warning").clone();
    assert_eq!(expected.as_array().expect("warnings").len(), 1);
    assert_at(&initial, "/diagnostics/0/code", "redundant-annotation");
    let opened = h
        .open_at(&sibling, "suffix text = text\n")
        .await
        .params
        .expect("sibling diagnostics");
    assert_at(&opened, "/uri", sibling.clone());
    assert_eq!(opened.get("diagnostics"), Some(&json!([])));
    assert_refreshed(&mut h, &uri, &json!([])).await;
    h.notify(
        "textDocument/didChange",
        json!({"textDocument":{"uri":sibling,"version":2},"contentChanges":[{"text":helper}]}),
    )
    .await;
    let changed = h.read_message().await.params.expect("sibling diagnostics");
    assert_at(&changed, "/uri", sibling.clone());
    assert_refreshed(&mut h, &uri, &expected).await;
    h.notify("textDocument/didChange", json!({"textDocument":{"uri":sibling,"version":3},"contentChanges":[{"text":"suffix text = text\n"}]})).await;
    let changed = h.read_message().await.params.expect("sibling diagnostics");
    assert_at(&changed, "/uri", sibling.clone());
    assert_eq!(changed.get("diagnostics"), Some(&json!([])));
    assert_refreshed(&mut h, &uri, &json!([])).await;
    h.notify("textDocument/didClose", text_doc(&sibling)).await;
    assert_refreshed(&mut h, &uri, &expected).await;
    assert!(
        h.engine.project_cache.analyses() <= 5,
        "five document events must cost at most five project analyses, including sibling refreshes"
    );
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root).expect("remove fixture");
}

/// The manifest decides the namespace a declaration is named by, so an edit to
/// `osprey.toml` changes what an unchanged buffer is told — the reader saves the
/// manifest and types on. One project analysis is shared between the open files
/// answering an edit, and it must be keyed on the manifest too: keying on the
/// buffers alone republished the previous namespace.
#[tokio::test]
async fn manifest_changes_refresh_unchanged_buffer_diagnostics() {
    let root = std::env::temp_dir().join(format!("osprey manifest cache {}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("fixture directory");
    let manifest = root.join("osprey.toml");
    let config = "[project]\nname = \"cache\"\nsource_roots = [\"src\"]\ndefault_namespace = \"before\"\nentry = \"src/main.ospml\"\n";
    std::fs::write(&manifest, config).expect("manifest");
    let source = "identity : int -> int\nidentity x = x + 1 ?: 0\nprint (identity 1)\n";
    let file = root.join("src/main.ospml");
    std::fs::write(&file, source).expect("entry");
    let uri = lspkit_server::uri::path_to_uri(&file).expect("URI");
    let mut h = Harness::start();
    let initial = h
        .open_at(&uri, source)
        .await
        .params
        .expect("initial diagnostics");
    assert!(
        initial.to_string().contains("before::identity"),
        "{initial}"
    );
    std::fs::write(&manifest, config.replace("before", "after")).expect("updated manifest");
    h.notify(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":source}]}),
    )
    .await;
    // A request is a barrier even if the diagnostics bus coalesces an unchanged
    // (stale) report. Do not wait forever for a notification in that case.
    h.send(&Message::request(
        RequestId::Number(901),
        "initialize",
        json!({}),
    ))
    .await;
    let mut reports = Vec::new();
    loop {
        let message = h.read_message().await;
        if message.id == Some(RequestId::Number(901)) {
            break;
        }
        reports.push(message.params.expect("diagnostics notification"));
    }
    assert!(
        reports
            .iter()
            .any(|report| report.to_string().contains("after::identity")),
        "{reports:?}"
    );
    assert!(
        !reports
            .iter()
            .any(|report| report.to_string().contains("before::identity")),
        "{reports:?}"
    );
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root).expect("remove fixture");
}
