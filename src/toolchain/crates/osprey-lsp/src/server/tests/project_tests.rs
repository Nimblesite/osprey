//! Project diagnostics through the real server transport.

use super::*;

#[tokio::test]
async fn state_advice_tracks_unsaved_edits_and_never_survives_errors(
) -> Result<(), Box<dyn std::error::Error>> {
    for (extension, state, plain, invalid) in [
        (
            "osp",
            "/// Counts.\nstate module Store { /// Zero.\n export fn zero() = 0 }\n",
            "/// Counts.\nmodule Store { /// Zero.\n export fn zero() = 0 }\n",
            "/// Counts.\nstate module Store { /// Zero.\n export fn zero() = missing }\n",
        ),
        (
            "ospml",
            "(** Counts. *)\nstate Store\n    (** Zero. *)\n    export zero () = 0\n",
            "(** Counts. *)\nmodule Store\n    (** Zero. *)\n    export zero () = 0\n",
            "(** Counts. *)\nstate Store\n    (** Zero. *)\n    export zero () = missing\n",
        ),
    ] {
        for removed_from_disk in [false, true] {
            state_advice_changes(extension, [state, plain, invalid], removed_from_disk).await?;
        }
    }
    Ok(())
}

async fn state_advice_changes(
    extension: &str,
    sources: [&str; 3],
    removed_from_disk: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new(extension)?;
    let uri = fixture.write("main", sources[0])?;
    let (mut h, report) = started_open(&uri, sources[0], "diagnostics").await?;
    assert_at(&report, "/diagnostics/0/code", "state-boundary");
    if removed_from_disk {
        // The live buffer keeps working after its on-disk entry disappears.
        std::fs::remove_file(fixture.root.join(format!("src/main.{extension}")))?;
    }
    for (version, text, code) in [
        (2, sources[1], None),
        (3, sources[2], Some("type-error")),
        (4, sources[0], Some("state-boundary")),
        (5, "(", Some("syntax-error")),
    ] {
        assert_changed_advice(&mut h, &uri, version, text, code).await?;
    }
    h.shutdown_and_exit().await;
    Ok(())
}

async fn assert_changed_advice(
    h: &mut Harness,
    uri: &str,
    version: i32,
    source: &str,
    code: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    h.notify(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":source}]}),
    )
    .await;
    let report = h.read_message().await.params.ok_or("changed diagnostics")?;
    assert_at(&report, "/uri", uri);
    if let Some(code) = code {
        assert_at(&report, "/diagnostics/0/code", code);
        if code != "state-boundary" {
            assert!(!report.to_string().contains("state-boundary"));
        }
    } else {
        assert_eq!(report.get("diagnostics"), Some(&json!([])));
    }
    Ok(())
}

#[tokio::test]
async fn library_manifest_changes_invalidate_style_advice() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let source = "namespace \"com.example.app\";\nstate module Store {}\n";
    let uri = fixture.write("main", source)?;
    let (mut h, initial) = started_open(&uri, source, "initial diagnostics").await?;
    assert_at(&initial, "/diagnostics/0/code", "namespace-reverse-domain");
    assert_at(&initial, "/diagnostics/1/code", "state-boundary");
    let manifest = fixture.root.join("osprey.toml");
    let config = std::fs::read_to_string(&manifest)?;
    std::fs::write(
        &manifest,
        format!("{config}\n[modules]\npublished_library = true\n"),
    )?;
    assert_changed_advice(&mut h, &uri, 2, source, Some("state-boundary")).await?;
    h.shutdown_and_exit().await;
    Ok(())
}

// [MODULES-STATE-INVENTORY], [LSP-MODULE-ADVICE]: exact ownership locations.
#[tokio::test]
async fn state_boundaries_are_visible_in_both_flavors() -> Result<(), Box<dyn std::error::Error>> {
    for (extension, source) in [
        (
            "osp",
            "namespace vault;\nstate module Store { export fn zero() = 0 }\n",
        ),
        (
            "ospml",
            "namespace vault\nstate Store\n    export zero () = 0\n",
        ),
    ] {
        let fixture = crate::test_support::ProjectFixture::new(extension)?;
        let uri = fixture.write("main", source)?;
        assert_opened_first_warning(
            &uri,
            source,
            "state-boundary",
            1,
            "state boundary `vault::Store`: 0 private cells; exported effects: none",
        )
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn project_layout_advice_is_warning_only() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    std::fs::create_dir_all(fixture.root.join("src/nested"))?;
    let source = "namespace app;\nfn main() = 0\n";
    let uri = fixture.write("main", source)?;
    let _ = fixture.write("nested/helper", "namespace app;\nfn identity(x) = x\n")?;
    assert_opened_first_warning(
        &uri,
        source,
        "namespace-folder-drift",
        0,
        "namespace `app` spans 2 folders; source paths do not change its identity",
    )
    .await?;
    Ok(())
}

/// A two-module project on disk — `src/main.ospml` holding `main` and its
/// sibling `src/helper.ospml` holding `helper` — plus both URIs.
fn sibling_project(
    tag: &str,
    name: &str,
    main: &str,
    helper: &str,
) -> (std::path::PathBuf, String, String) {
    let root = std::env::temp_dir().join(format!("osprey {tag} {}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("fixture directory");
    let manifest = format!(
        "[project]\nname = \"{name}\"\nsource_roots = [\"src\"]\ndefault_namespace = \"review\"\nentry = \"src/main.ospml\"\n"
    );
    std::fs::write(root.join("osprey.toml"), manifest).expect("manifest");
    std::fs::write(root.join("src/main.ospml"), main).expect("entry");
    std::fs::write(root.join("src/helper.ospml"), helper).expect("helper");
    let uri = lspkit_server::uri::path_to_uri(&root.join("src/main.ospml")).expect("URI");
    let sibling = lspkit_server::uri::path_to_uri(&root.join("src/helper.ospml")).expect("URI");
    (root, uri, sibling)
}

/// A sibling project whose `main` is open, with its first report in hand.
async fn sibling_server(
    tag: &str,
    name: &str,
    main: &str,
    helper: &str,
) -> (std::path::PathBuf, String, String, Harness, Value) {
    let (root, uri, sibling) = sibling_project(tag, name, main, helper);
    let mut h = Harness::start();
    let initial = h
        .open_at(&uri, main)
        .await
        .params
        .expect("initial diagnostics");
    (root, uri, sibling, h, initial)
}

/// The sibling's report comes back clean, and `uri` refreshes clean with it.
async fn assert_sibling_and_main_clean(h: &mut Harness, uri: &str, sibling: &str, report: Value) {
    assert_at(&report, "/uri", sibling);
    assert_eq!(report.get("diagnostics"), Some(&json!([])));
    assert_refreshed(h, uri, &json!([])).await;
}

#[tokio::test]
async fn sibling_syntax_errors_preserve_existing_type_errors() {
    let main = "print missingName\n";
    let helper = "suffix text = text + \"!\"\n";
    let (root, uri, sibling, mut h, initial) =
        sibling_server("sibling error", "errors", main, helper).await;
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
    let main = "decorate : string -> string\ndecorate text = suffix text\nprint (decorate \"x\")\n";
    let helper = "suffix text = text + \"!\"\n";
    let (root, uri, sibling, mut h, initial) =
        sibling_server("live diagnostics", "warnings", main, helper).await;
    let expected = initial.get("diagnostics").expect("warning").clone();
    assert_eq!(expected.as_array().expect("warnings").len(), 1);
    assert_at(&initial, "/diagnostics/0/code", "redundant-annotation");
    let opened = h
        .open_at(&sibling, "suffix text = text\n")
        .await
        .params
        .expect("sibling diagnostics");
    assert_sibling_and_main_clean(&mut h, &uri, &sibling, opened).await;
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
    assert_sibling_and_main_clean(&mut h, &uri, &sibling, changed).await;
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
    let source = "identity : int -> int\nidentity x = wrapAdd x 1\nprint (identity 1)\n";
    let file = root.join("src/main.ospml");
    std::fs::write(&file, source).expect("entry");
    let uri = lspkit_server::uri::path_to_uri(&file).expect("URI");
    let mut h = Harness::start_with_progress().await;
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
    // Diagnostics are published beside requests: wait until every queued file
    // is published. The bus may coalesce an unchanged (stale) report.
    let reports = settled(&mut h, 901).await;
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

// [LSP-WORKSPACE] A source root two projects share (`source_roots = ["../lib",
// "."]`) has no manifest of its own, and the manifest that does enclose it
// does not list it. References from the shared file must still reach every
// project that includes it; they used to come back from the open buffer alone.
// The projects spell the file `app/../lib/shared.osp`, the editor
// `lib/shared.osp`: one file, so its declarations must not clash.
#[tokio::test]
async fn references_from_a_shared_source_root_reach_every_project_that_includes_it(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let shared_source = "fn shared(x) = x\n";
    let shared = fixture.root.join("lib/shared.osp");
    std::fs::create_dir_all(fixture.root.join("lib"))?;
    std::fs::write(&shared, shared_source)?;
    let mut mains = Vec::new();
    for name in ["app", "tool"] {
        let dir = fixture.root.join(name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("osprey.toml"),
            format!("[project]\nsource_roots = [\"../lib\", \".\"]\ndefault_namespace = \"{name}\"\nentry = \"main.osp\"\n"),
        )?;
        let main = dir.join("main.osp");
        std::fs::write(&main, "fn main() = print(shared(\"x\"))\n")?;
        mains.push(lspkit_server::uri::path_to_uri(&main)?);
    }
    let uri = lspkit_server::uri::path_to_uri(&shared)?;
    let mut h = Harness::start();
    let opened = h.open_at(&uri, shared_source).await;
    let errors = diagnostics(&opened)
        .and_then(Value::as_array)
        .map(|all| all.iter().filter(|d| d["severity"] == 1).count());
    assert_eq!(errors, Some(0), "{opened:?}");
    let column = crate::test_support::col_of(shared_source, 0, "shared");
    let reply = h
        .request(
            960,
            "textDocument/references",
            json!({"textDocument":{"uri":uri},"position":{"line":0,"character":column},"context":{"includeDeclaration":true}}),
        )
        .await;
    let text = format!("{:?}", reply.result);
    for main in &mains {
        assert!(text.contains(main.as_str()), "{main}: {reply:?}");
    }
    assert!(text.contains(uri.as_str()), "{reply:?}");
    h.shutdown_and_exit().await;
    Ok(())
}

/// Whether some settled report publishes `code` for `uri`.
fn reports_code(reports: &[Value], uri: &str, code: &str) -> bool {
    reports
        .iter()
        .any(|p| p["uri"] == uri && p.pointer("/diagnostics/0/code") == Some(&json!(code)))
}

/// The repository's glob keeps failscompilation/ quiet, open or not; replacing the exclude
/// globs reports it and clears whatever the new globs match.
/// Implements [LSP-DIAGNOSTICS-EXCLUDE].
async fn excluded_until_overridden(
    h: &mut Harness,
    reports: &[Value],
    uri: &impl Fn(&str) -> String,
) {
    assert!(
        !reports.iter().any(|p| p["uri"] == uri("failscompilation")),
        "failscompilation excluded by its glob: {reports:?}"
    );
    let rejected = json!({"textDocument":{"uri":uri("failscompilation"),"languageId":"osprey","version":1,"text":"fn broken() { missing }\n"}});
    h.notify("textDocument/didOpen", rejected).await;
    let reports = settled(h, 406).await;
    assert!(
        !reports.iter().any(|p| p["uri"] == uri("failscompilation")),
        "an open excluded file stays quiet: {reports:?}"
    );
    h.notify(
        "workspace/didChangeConfiguration",
        json!({"settings":{"osprey":{"diagnostics":{"exclude":["oth?r/*.osp", "/**/tw*/bad.osp"]}}}}),
    )
    .await;
    let reports = settled(h, 407).await;
    assert!(
        reports_code(&reports, &uri("failscompilation"), "type-error"),
        "replacing the glob reports failscompilation: {reports:?}"
    );
    for folder in ["other", "two"] {
        assert!(
            reports
                .iter()
                .any(|p| p["uri"] == uri(folder) && p["diagnostics"] == json!([])),
            "{folder} cleared by its glob: {reports:?}"
        );
    }
    assert!(
        !reports
            .iter()
            .any(|p| p["uri"] == uri("one") && p["diagnostics"] == json!([])),
        "one stays reported: {reports:?}"
    );
}

/// Unopened files must be checked on startup, then removed when out of scope;
/// failscompilation/ stays quiet, open or not, until the exclude globs change.
/// Implements [LSP-DIAGNOSTICS-SCOPE], [LSP-DIAGNOSTICS-EXCLUDE].
#[tokio::test]
async fn repository_diagnostics_default_and_live_scope_roots(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("osprey scopes {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for folder in ["one", "two", "other", "target", "failscompilation"] {
        std::fs::create_dir_all(root.join(folder))?;
        std::fs::write(
            root.join(folder).join("bad.osp"),
            "fn broken() { missing }\n",
        )?;
    }
    let uri =
        |folder: &str| crate::workspace::uri_of(&root.join(folder).join("bad.osp")).expect("uri");
    let mut h = Harness::start();
    let _ = h
        .request(
            400,
            "initialize",
            with_progress(json!({"workspaceFolders": [{"uri": crate::workspace::uri_of(&root), "name": "repo"}], "initializationOptions": {"diagnostics": {"exclude": ["**/failscompilation/**"]}}})),
        )
        .await;
    h.notify("initialized", json!({})).await;
    let reports = settled(&mut h, 401).await;
    for folder in ["one", "two", "other"] {
        assert!(
            reports_code(&reports, &uri(folder), "type-error"),
            "unopened {folder}: {reports:?}"
        );
    }
    assert!(
        !reports.iter().any(|p| p["uri"] == uri("target")),
        "build outputs excluded"
    );
    excluded_until_overridden(&mut h, &reports, &uri).await;
    h.notify(
        "workspace/didChangeConfiguration",
        json!({"settings":{"osprey":{"diagnostics":{"scope":"roots", "roots":["one", "two"], "exclude":["**/failscompilation/**"]}}}}),
    )
    .await;
    let reports = settled(&mut h, 402).await;
    assert!(
        reports
            .iter()
            .any(|p| p["uri"] == uri("failscompilation") && p["diagnostics"] == json!([])),
        "the glob returns and clears the open file: {reports:?}"
    );
    assert!(
        reports_code(&reports, &uri("two"), "type-error"),
        "two is back in scope: {reports:?}"
    );
    std::fs::write(root.join("two/bad.osp"), "fn broken() { 1 }\n")?;
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":uri("two"),"type":2}]}),
    )
    .await;
    settles_cleared(&mut h, 403, &uri("two"), "refresh unopened repair").await;
    h.notify(
        "workspace/didChangeConfiguration",
        json!({"settings":{"osprey":{"diagnostics":{"scope":"openFiles"}}}}),
    )
    .await;
    settles_cleared(&mut h, 404, &uri("one"), "clear closed files").await;
    let report = h.open_at(&uri("one"), "fn broken() { missing }\n").await;
    assert_at(
        &report.params.ok_or("open report")?,
        "/diagnostics/0/code",
        "type-error",
    );
    h.notify(
        "textDocument/didClose",
        json!({"textDocument":{"uri":uri("one")}}),
    )
    .await;
    settles_cleared(&mut h, 405, &uri("one"), "close clears").await;
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}
