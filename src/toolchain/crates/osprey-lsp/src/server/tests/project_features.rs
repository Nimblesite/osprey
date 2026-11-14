use super::*;

use crate::test_support::ProjectFixture;

/// Boxed error type shared by the project-feature tests.
type Failure = Box<dyn std::error::Error>;

/// Result type shared by the project-feature tests.
type Fallible = Result<(), Failure>;

/// Per-flavor `(extension, disk, live, main)` texts for the unsaved-sibling test.
const SIBLING_SOURCES: [(&str, &str, &str, &str); 2] = [
    (
        "osp",
        "fn saved(x) = x\n",
        "\nfn fresh(x) = x + \"!\"\n",
        "fn main() = print(fresh(\"x\"))\n",
    ),
    (
        "ospml",
        "saved x = x\n",
        "\nfresh x = x + \"!\"\n",
        "main () = print (fresh \"x\")\n",
    ),
];

/// Per-flavor `(extension, main, live)` texts for the live-effect test.
const EFFECT_SOURCES: [(&str, &str, &str); 2] = [
    (
        "osp",
        "fn read() = perform Reader.name()\nfn main() = provide(read)\n",
        "effect Reader { name: fn() -> string }\nfn provide(work) = {\n    handle Reader { name => \"live\" }\n    work()\n}\n",
    ),
    (
        "ospml",
        "read () = perform Reader.name ()\nmain () = provide read\n",
        "effect Reader\n    name : Unit => string\nprovide work =\n    handle Reader\n        name => \"live\"\n    work ()\n",
    ),
];

/// Per-flavor `(extension, main, helper)` texts for the watched-project test.
const WATCHED_SOURCES: [(&str, &str, &str); 2] = [
    (
        "osp",
        "fn main() = fresh(\"x\")\n",
        "fn fresh(x) = x + \"!\"\n",
    ),
    ("ospml", "main () = fresh \"x\"\n", "fresh x = x + \"!\"\n"),
];

/// The LSP URI of `path`.
fn path_uri(path: &std::path::Path) -> Result<String, Failure> {
    Ok(lspkit_server::uri::path_to_uri(path)?)
}

/// Prepend `flavor = "<flavor>"` to the fixture manifest and return its text.
fn set_flavor(fixture: &ProjectFixture, flavor: &str) -> Result<String, Failure> {
    let manifest = fixture.root.join("osprey.toml");
    let config = format!(
        "{}flavor = \"{flavor}\"\n",
        std::fs::read_to_string(&manifest)?
    );
    std::fs::write(&manifest, &config)?;
    Ok(config)
}

/// A new `extension` fixture with `main` written, as `(fixture, uri)`.
fn fixture_with_main(extension: &str, main: &str) -> Result<(ProjectFixture, String), Failure> {
    let fixture = ProjectFixture::new(extension)?;
    let uri = fixture.write("main", main)?;
    Ok((fixture, uri))
}

/// A harness with `main` open at `uri` and `text` open at `sibling`.
async fn start_with_sibling(uri: &str, main: &str, sibling: &str, text: &str) -> Harness {
    let mut h = Harness::start_with_progress().await;
    let _ = h.open_at(uri, main).await;
    let _ = h.open_at(sibling, text).await;
    h
}

/// A harness whose first report for `main` at `uri` is a type error.
async fn start_with_type_error(uri: &str, main: &str) -> Harness {
    let mut h = Harness::start_with_progress().await;
    let initial = h.open_at(uri, main).await;
    assert!(format!("{initial:?}").contains("type-error"), "{initial:?}");
    h
}

/// Close `sibling`, then request `feature` at line 0 / `column` of `uri`.
async fn closed_request(
    h: &mut Harness,
    sibling: &str,
    id: i64,
    feature: &str,
    uri: &str,
    column: u32,
) -> Message {
    h.notify("textDocument/didClose", text_doc(sibling)).await;
    h.request(
        id,
        &format!("textDocument/{feature}"),
        position_params(uri, 0, column),
    )
    .await
}

/// Send a watched-file change for `changed` and return the settled reports.
async fn watch_event(h: &mut Harness, changed: &str, kind: u8) -> Vec<Value> {
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":changed,"type":kind}]}),
    )
    .await;
    settled(h, 990).await
}

/// Watch `changed` and assert every `needles` entry appears in the reports.
async fn assert_watch_mentions(h: &mut Harness, changed: &str, kind: u8, needles: &[&str]) {
    let reports = watch_event(h, changed, kind).await;
    let text = format!("{reports:?}");
    for needle in needles {
        assert!(text.contains(needle), "missing {needle}: {reports:?}");
    }
}

// [LSP-WORKSPACE] Every feature must see the same unsaved project as diagnostics.
#[tokio::test]
async fn project_features_follow_unsaved_siblings_in_both_flavors() -> Fallible {
    for (extension, disk, live, main) in SIBLING_SOURCES {
        let (fixture, uri) = fixture_with_main(extension, main)?;
        let sibling = fixture.write("helper", disk)?;
        let mut h = start_with_sibling(&uri, main, &sibling, live).await;
        let parses = h.engine.project_cache.parses();
        assert_live_features(&mut h, &uri, &sibling, main).await;
        let reused = h.engine.project_cache.parses();
        assert_eq!(reused, parses, "requests must reuse unchanged syntax");
        let completion = closed_request(&mut h, &sibling, 906, "completion", &uri, 0).await;
        let items = format!("{:?}", completion.result);
        assert!(!items.contains("fresh"), "{completion:?}");
        h.shutdown_and_exit().await;
    }
    Ok(())
}

async fn assert_live_features(h: &mut Harness, uri: &str, sibling: &str, main: &str) {
    let column = crate::test_support::col_of(main, 0, "fresh");
    for (id, method) in [
        (900, "hover"),
        (901, "signatureHelp"),
        (902, "definition"),
        (903, "references"),
        (904, "completion"),
    ] {
        let params = json!({"textDocument":{"uri":uri},"position":{"line":0,"character":column},"context":{"includeDeclaration":true}});
        let reply = h
            .request(id, &format!("textDocument/{method}"), params)
            .await;
        let text = format!("{:?}", reply.result);
        let fresh = text.contains("fresh") && text.contains("string");
        match method {
            "definition" | "references" => assert!(text.contains(sibling), "{method}: {reply:?}"),
            _ => assert!(fresh, "{method}: {reply:?}"),
        }
        assert!(
            !text.contains("saved"),
            "stale disk symbol in {method}: {reply:?}"
        );
    }
}

#[tokio::test]
async fn live_effect_declarations_and_handler_locations_follow_both_flavors() -> Fallible {
    for (extension, main, live) in EFFECT_SOURCES {
        let (fixture, uri) = fixture_with_main(extension, main)?;
        let sibling = fixture.write("helper", "// saved file has no handler\n")?;
        let mut h = start_with_sibling(&uri, main, &sibling, live).await;
        assert_effect_features(&mut h, &uri, &sibling, main).await;
        h.shutdown_and_exit().await;
    }
    Ok(())
}

async fn assert_effect_features(h: &mut Harness, uri: &str, sibling: &str, main: &str) {
    let column = crate::test_support::col_of(main, 0, "name");
    let hover = h
        .request(910, "textDocument/hover", position_params(uri, 0, column))
        .await;
    let text = format!("{:?}", hover.result);
    let declared = text.contains("Reader.name") && text.contains("string");
    assert!(declared, "{hover:?}");
    let implementations = h
        .request(
            911,
            "textDocument/implementation",
            position_params(uri, 0, column),
        )
        .await;
    let locations = array_result(&implementations, "live handler locations");
    assert_eq!(locations.len(), 1, "{implementations:?}");
    let handler = locations.first().and_then(|location| location.get("uri"));
    assert_eq!(handler.and_then(Value::as_str), Some(sibling));
}

#[tokio::test]
async fn watched_sources_and_manifest_refresh_the_live_project() -> Fallible {
    for (extension, main, helper) in WATCHED_SOURCES {
        let (fixture, uri) = fixture_with_main(extension, main)?;
        let mut h = start_with_type_error(&uri, main).await;
        let sibling = fixture.write("helper", helper)?;
        assert_watched(&mut h, &uri, &sibling, 1, false).await;
        let parses = h.engine.project_cache.parses();
        let _ = h
            .request(916, "textDocument/completion", position_params(&uri, 0, 0))
            .await;
        assert_eq!(h.engine.project_cache.parses(), parses);
        exclude_helper(&fixture, &mut h, &uri, extension).await?;
        h.shutdown_and_exit().await;
    }
    Ok(())
}

async fn exclude_helper(
    fixture: &ProjectFixture,
    h: &mut Harness,
    uri: &str,
    extension: &str,
) -> Fallible {
    let manifest = fixture.root.join("osprey.toml");
    let original = std::fs::read_to_string(&manifest)?;
    std::fs::write(
        &manifest,
        original.replace("[\"src\"]", &format!("[\"src/main.{extension}\"]")),
    )?;
    let manifest_uri = path_uri(&manifest)?;
    assert_watched(h, uri, &manifest_uri, 2, true).await;
    std::fs::write(&manifest, &original)?;
    assert_watched(h, uri, &manifest_uri, 2, false).await;
    let helper = fixture.root.join(format!("src/helper.{extension}"));
    let helper_uri = path_uri(&helper)?;
    std::fs::remove_file(helper)?;
    assert_watched(h, uri, &helper_uri, 3, true).await;
    Ok(())
}

async fn assert_watched(h: &mut Harness, uri: &str, changed: &str, kind: u8, errors: bool) {
    let reports = watch_event(h, changed, kind).await;
    let report = reports
        .iter()
        .find(|report| report.get("uri").and_then(Value::as_str) == Some(uri));
    let published = report.is_some();
    assert!(
        published,
        "watch event failed to publish {uri}: {reports:?}"
    );
    let typed = format!("{report:?}").contains("type-error");
    assert_eq!(typed, errors, "{reports:?}");
}

#[tokio::test]
async fn unsaved_mixed_flavor_files_join_and_leave_the_project() -> Fallible {
    let main = "fn main() = print(fresh(\"x\"))\n";
    let (fixture, uri) = fixture_with_main("osp", main)?;
    let sibling = path_uri(&fixture.root.join("src/helper.ospml"))?;
    let mut h = start_with_sibling(&uri, main, &sibling, "\nfresh text = text + \"!\"\n").await;
    assert_live_features(&mut h, &uri, &sibling, main).await;
    let definition = closed_request(&mut h, &sibling, 930, "definition", &uri, 20).await;
    let closed = format!("closed unsaved definition: {definition:?}");
    assert_eq!(definition.result, Some(json!([])), "{closed}");
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn unsaved_repair_overrides_invalid_disk_and_actions_reuse_the_analysis() -> Fallible {
    let main = "decorate : string -> string\ndecorate text = suffix text\nmain () = print (decorate \"x\")\n";
    let (fixture, uri) = fixture_with_main("ospml", main)?;
    let sibling = fixture.write("helper", "suffix text = (\n")?;
    let mut h = start_with_sibling(&uri, main, &sibling, "suffix text = text + \"!\"\n").await;
    let _ = settled(&mut h, 990).await;
    let before = h.engine.project_cache.analyses();
    let actions = h.request(931, "textDocument/codeAction", json!({"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":20}},"context":{"only":["quickfix"]}})).await;
    let repaired = array_result(&actions, "repaired project actions").len();
    assert_eq!(repaired, 1, "{actions:?}");
    let analyses = h.engine.project_cache.analyses();
    assert_eq!(analyses, before, "actions repeated an identical proof");
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn watched_invalid_project_inputs_report_their_source() -> Fallible {
    let ready = "fn main() = print(\"ready\")\n";
    let (fixture, uri) = fixture_with_main("osp", ready)?;
    let mut h = Harness::start_with_progress().await;
    let _ = h.open_at(&uri, ready).await;
    let helper = fixture.write("helper", "fn helper(\n")?;
    assert_watch_mentions(&mut h, &helper, 2, &["project-error", "helper.osp"]).await;
    let manifest = fixture.root.join("osprey.toml");
    std::fs::write(&manifest, "[project]\nsource_roots = invalid\n")?;
    let manifest_uri = path_uri(&manifest)?;
    assert_watch_mentions(&mut h, &manifest_uri, 2, &["project-error", "osprey.toml"]).await;
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn manifest_flavor_controls_diagnostics_and_features_together() -> Fallible {
    let fixture = ProjectFixture::new("osp")?;
    let manifest = fixture.root.join("osprey.toml");
    let config = set_flavor(&fixture, "ml")?;
    let main = "main () = print (fresh \"x\")\n";
    let uri = fixture.write("main", main)?;
    let sibling = fixture.write("helper", "fresh x = x + \"!\"\n")?;
    let mut h = Harness::start_with_progress().await;
    let opened = h.open_at(&uri, main).await;
    assert_eq!(diagnostics(&opened), Some(&json!([])), "{opened:?}");
    assert_live_features(&mut h, &uri, &sibling, main).await;
    let formatted = h
        .request(939, "textDocument/formatting", text_doc(&uri))
        .await;
    assert_eq!(formatted.result, Some(json!([])), "{formatted:?}");
    let _ = h
        .open_at(&sibling, "fresh : string -> string\nfresh x = x + \"!\"\n")
        .await;
    let actions = h
        .request(
            940,
            "textDocument/codeAction",
            json!({"textDocument":{"uri":sibling},"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":0}},"context":{"diagnostics":[]}}),
        )
        .await;
    let fixes = array_result(&actions, "manifest-flavor annotation fixes").len();
    assert_eq!(fixes, 2);
    std::fs::write(&manifest, config.replace("\"ml\"", "\"default\""))?;
    let manifest_uri = path_uri(&manifest)?;
    assert_watch_mentions(&mut h, &manifest_uri, 2, &["syntax-error"]).await;
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn files_outside_source_roots_keep_standalone_flavor_and_symbols() -> Fallible {
    let (fixture, _) = fixture_with_main("osp", "main () = print \"project\"\n")?;
    let _ = fixture.write("helper", "projectOnly x = x\n")?;
    let _ = set_flavor(&fixture, "ml")?;
    let uri = path_uri(&fixture.root.join("scratch.osp"))?;
    let mut h = Harness::start_with_progress().await;
    let opened = h.open_at(&uri, "fn main() = print(\"standalone\")\n").await;
    assert_eq!(diagnostics(&opened), Some(&json!([])), "{opened:?}");
    let completion = h
        .request(950, "textDocument/completion", position_params(&uri, 0, 0))
        .await;
    let items = format!("{completion:?}");
    assert!(!items.contains("projectOnly"), "{completion:?}");
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn nested_project_edits_refresh_every_project_that_includes_the_source() -> Fallible {
    let main = "fn main() = print(fresh(\"x\"))\n";
    let (fixture, uri) = fixture_with_main("osp", main)?;
    let nested = fixture.root.join("src/nested");
    std::fs::create_dir_all(nested.join("src"))?;
    std::fs::write(nested.join("osprey.toml"), "[project]\nsource_roots = [\"src\"]\ndefault_namespace = \"nested\"\nentry = \"src/helper.osp\"\n")?;
    let helper = nested.join("src/helper.osp");
    std::fs::write(&helper, "fn saved(x) = x\n")?;
    let sibling = path_uri(&helper)?;
    let mut h = start_with_type_error(&uri, main).await;
    let _ = h.open_at(&sibling, "fn fresh(x) = x + \"!\"\n").await;
    let reports = settled(&mut h, 990).await;
    let drift = json!([{
        "code": "namespace-folder-drift", "severity": 2, "source": "osprey",
        "message": "namespace `live` spans 2 folders; source paths do not change its identity",
        "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 29}}
    }]);
    assert!(
        reports.iter().any(
            |report| report.get("uri").and_then(Value::as_str) == Some(&uri)
                && report.get("diagnostics") == Some(&drift)
        ),
        "{reports:?}"
    );
    h.notify("textDocument/didClose", text_doc(&sibling)).await;
    let reports = settled(&mut h, 990).await;
    assert!(format!("{reports:?}").contains("type-error"), "{reports:?}");
    h.shutdown_and_exit().await;
    Ok(())
}

// [LSP-ANALYSIS-SCOPE] Displayed types come from the whole project and its
// packages by default; `osprey.analysis.scope = "file"` checks each file alone,
// live, and `"project"` restores the default.
#[tokio::test]
async fn analysis_scope_setting_turns_project_inference_off_and_on() -> Fallible {
    let main = "main () = print \"${listLength (painting::Logo::steps ())}\"\n";
    let (fixture, _main) = fixture_with_main("ospml", main)?;
    let _ui = fixture.write("ui", "namespace ui\n\ntype PathStep =\n    LineTo\n        toX : float\n        toY : float\n    Close\n")?;
    let logo = "namespace painting\n\nmodule Logo\n    segment row = match row\n        [x, y, ..._rest] => ui::LineTo(toX = x, toY = y)\n        _ => ui::Close\n\n    export steps () = mapList [[1.0, 2.0]] segment\n";
    let uri = fixture.write("logo", logo)?;
    let mut h = Harness::start_with_progress().await;
    let _ = h.open_at(&uri, logo).await;
    let binder = crate::test_support::col_of(logo, 4, "x, y");
    let function = crate::test_support::col_of(logo, 3, "segment");
    for (id, scope, binder_type, signature) in [
        (910, None, "x : float", "List<float> -> ui::PathStep"),
        (920, Some("file"), "x : _", "_ -> _"),
        (
            930,
            Some("project"),
            "x : float",
            "List<float> -> ui::PathStep",
        ),
    ] {
        if let Some(scope) = scope {
            let settings = json!({"settings":{"osprey":{"analysis":{"scope":scope}}}});
            h.notify("workspace/didChangeConfiguration", settings).await;
        }
        for (offset, method, line, column, shown) in [
            (0, "hover", 4, binder, binder_type),
            (1, "hover", 3, function, signature),
            (2, "signatureHelp", 3, function, signature),
        ] {
            let params = position_params(&uri, line, column);
            let reply = h
                .request(id + offset, &format!("textDocument/{method}"), params)
                .await;
            let text = format!("{:?}", reply.result);
            assert!(
                text.contains(shown),
                "{scope:?} {method}: want {shown} in {text}"
            );
        }
    }
    h.shutdown_and_exit().await;
    Ok(())
}
