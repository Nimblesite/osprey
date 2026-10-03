use super::*;

// [LSP-WORKSPACE] Every feature must see the same unsaved project as diagnostics.
#[tokio::test]
async fn project_features_follow_unsaved_siblings_in_both_flavors(
) -> Result<(), Box<dyn std::error::Error>> {
    for extension in ["osp", "ospml"] {
        live_project_features(extension).await?;
    }
    Ok(())
}

async fn live_project_features(extension: &str) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new(extension)?;
    let disk = if extension == "osp" {
        "fn saved(x) = x\n"
    } else {
        "saved x = x\n"
    };
    let live = if extension == "osp" {
        "\nfn fresh(x) = x + \"!\"\n"
    } else {
        "\nfresh x = x + \"!\"\n"
    };
    let main = if extension == "osp" {
        "fn main() = print(fresh(\"x\"))\n"
    } else {
        "main () = print (fresh \"x\")\n"
    };
    let uri = fixture.write("main", main)?;
    let sibling = fixture.write("helper", disk)?;
    let mut h = Harness::start();
    let _ = h.open_at(&uri, main).await;
    let _ = h.open_at(&sibling, live).await;
    let parses = h.engine.project_cache.parses();
    assert_live_features(&mut h, &uri, &sibling, main).await;
    assert_eq!(
        h.engine.project_cache.parses(),
        parses,
        "requests must reuse unchanged syntax"
    );
    h.notify("textDocument/didClose", text_doc(&sibling)).await;
    let completion = h
        .request(906, "textDocument/completion", position_params(&uri, 0, 0))
        .await;
    assert!(
        !format!("{:?}", completion.result).contains("fresh"),
        "{completion:?}"
    );
    h.shutdown_and_exit().await;
    Ok(())
}

async fn assert_live_features(h: &mut Harness, uri: &str, sibling: &str, main: &str) {
    let column = crate::test_support::col_of(main, 0, "fresh");
    for (index, method) in [
        "hover",
        "signatureHelp",
        "definition",
        "references",
        "completion",
    ]
    .iter()
    .enumerate()
    {
        let params = json!({"textDocument":{"uri":uri},"position":{"line":0,"character":column},"context":{"includeDeclaration":true}});
        let reply = h
            .request(
                i64::try_from(index).map_or(900, |index| 900 + index),
                &format!("textDocument/{method}"),
                params,
            )
            .await;
        let text = format!("{:?}", reply.result);
        match *method {
            "definition" | "references" => assert!(text.contains(sibling), "{method}: {reply:?}"),
            _ => assert!(
                text.contains("fresh") && text.contains("string"),
                "{method}: {reply:?}"
            ),
        }
        assert!(
            !text.contains("saved"),
            "stale disk symbol in {method}: {reply:?}"
        );
    }
}

#[tokio::test]
async fn live_effect_declarations_and_handler_locations_follow_both_flavors(
) -> Result<(), Box<dyn std::error::Error>> {
    for extension in ["osp", "ospml"] {
        live_effect_features(extension).await?;
    }
    Ok(())
}

async fn live_effect_features(extension: &str) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new(extension)?;
    let main = if extension == "osp" {
        "fn read() = perform Reader.name()\nfn main() = provide(read)\n"
    } else {
        "read () = perform Reader.name ()\nmain () = provide read\n"
    };
    let live = if extension == "osp" {
        "effect Reader { name: fn() -> string }\nfn provide(work) = {\n    handle Reader { name => \"live\" }\n    work()\n}\n"
    } else {
        "effect Reader\n    name : Unit => string\nprovide work =\n    handle Reader\n        name => \"live\"\n    work ()\n"
    };
    let uri = fixture.write("main", main)?;
    let sibling = fixture.write("helper", "// saved file has no handler\n")?;
    let mut h = Harness::start();
    let _ = h.open_at(&uri, main).await;
    let _ = h.open_at(&sibling, live).await;
    assert_effect_features(&mut h, &uri, &sibling, main).await;
    h.shutdown_and_exit().await;
    Ok(())
}

async fn assert_effect_features(h: &mut Harness, uri: &str, sibling: &str, main: &str) {
    let column = crate::test_support::col_of(main, 0, "name");
    let hover = h
        .request(910, "textDocument/hover", position_params(uri, 0, column))
        .await;
    let text = format!("{:?}", hover.result);
    assert!(
        text.contains("Reader.name") && text.contains("string"),
        "{hover:?}"
    );
    let implementations = h
        .request(
            911,
            "textDocument/implementation",
            position_params(uri, 0, column),
        )
        .await;
    let locations = array_result(&implementations, "live handler locations");
    assert_eq!(locations.len(), 1, "{implementations:?}");
    assert_eq!(
        locations
            .first()
            .and_then(|location| location.get("uri"))
            .and_then(Value::as_str),
        Some(sibling)
    );
}

#[tokio::test]
async fn watched_sources_and_manifest_refresh_the_live_project(
) -> Result<(), Box<dyn std::error::Error>> {
    for extension in ["osp", "ospml"] {
        watched_project(extension).await?;
    }
    Ok(())
}

async fn watched_project(extension: &str) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new(extension)?;
    let main = if extension == "osp" {
        "fn main() = fresh(\"x\")\n"
    } else {
        "main () = fresh \"x\"\n"
    };
    let helper = if extension == "osp" {
        "fn fresh(x) = x + \"!\"\n"
    } else {
        "fresh x = x + \"!\"\n"
    };
    let uri = fixture.write("main", main)?;
    let mut h = Harness::start();
    let initial = h.open_at(&uri, main).await;
    assert!(format!("{initial:?}").contains("type-error"), "{initial:?}");
    let sibling = fixture.write("helper", helper)?;
    assert_watched(&mut h, &uri, &sibling, 1, false).await;
    let parses = h.engine.project_cache.parses();
    let _ = h
        .request(916, "textDocument/completion", position_params(&uri, 0, 0))
        .await;
    assert_eq!(h.engine.project_cache.parses(), parses);
    exclude_helper(&fixture, &mut h, &uri, extension).await?;
    h.shutdown_and_exit().await;
    Ok(())
}

async fn exclude_helper(
    fixture: &crate::test_support::ProjectFixture,
    h: &mut Harness,
    uri: &str,
    extension: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = fixture.root.join("osprey.toml");
    let original = std::fs::read_to_string(&manifest)?;
    std::fs::write(
        &manifest,
        original.replace("[\"src\"]", &format!("[\"src/main.{extension}\"]")),
    )?;
    let manifest_uri = lspkit_server::uri::path_to_uri(&manifest)?;
    assert_watched(h, uri, &manifest_uri, 2, true).await;
    std::fs::write(&manifest, &original)?;
    assert_watched(h, uri, &manifest_uri, 2, false).await;
    let helper = fixture.root.join(format!("src/helper.{extension}"));
    let helper_uri = lspkit_server::uri::path_to_uri(&helper)?;
    std::fs::remove_file(helper)?;
    assert_watched(h, uri, &helper_uri, 3, true).await;
    Ok(())
}

async fn assert_watched(h: &mut Harness, uri: &str, changed: &str, kind: u8, errors: bool) {
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":changed,"type":kind}]}),
    )
    .await;
    let reports = drain_reports(h).await;
    let report = reports
        .iter()
        .find(|report| report.get("uri").and_then(Value::as_str) == Some(uri));
    assert!(
        report.is_some(),
        "watch event failed to publish {uri}: {reports:?}"
    );
    assert_eq!(
        format!("{report:?}").contains("type-error"),
        errors,
        "{reports:?}"
    );
}

async fn drain_reports(h: &mut Harness) -> Vec<Value> {
    h.send(&Message::request(
        RequestId::Number(990),
        "initialize",
        json!({}),
    ))
    .await;
    let mut reports = Vec::new();
    loop {
        let message = h.read_message().await;
        if message.id == Some(RequestId::Number(990)) {
            return reports;
        }
        if let Some(params) = message.params {
            reports.push(params);
        }
    }
}

#[tokio::test]
async fn unsaved_mixed_flavor_files_join_and_leave_the_project(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let main = "fn main() = print(fresh(\"x\"))\n";
    let uri = fixture.write("main", main)?;
    let sibling = lspkit_server::uri::path_to_uri(&fixture.root.join("src/helper.ospml"))?;
    let mut h = Harness::start();
    let _ = h.open_at(&uri, main).await;
    let _ = h.open_at(&sibling, "\nfresh text = text + \"!\"\n").await;
    assert_live_features(&mut h, &uri, &sibling, main).await;
    h.notify("textDocument/didClose", text_doc(&sibling)).await;
    let definition = h
        .request(930, "textDocument/definition", position_params(&uri, 0, 20))
        .await;
    assert_eq!(
        definition.result,
        Some(json!([])),
        "closed unsaved definition: {definition:?}"
    );
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn unsaved_repair_overrides_invalid_disk_and_actions_reuse_the_analysis(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("ospml")?;
    let main = "decorate : string -> string\ndecorate text = suffix text\nmain () = print (decorate \"x\")\n";
    let uri = fixture.write("main", main)?;
    let sibling = fixture.write("helper", "suffix text = (\n")?;
    let mut h = Harness::start();
    let _ = h.open_at(&uri, main).await;
    let _ = h.open_at(&sibling, "suffix text = text + \"!\"\n").await;
    let _ = drain_reports(&mut h).await;
    let before = h.engine.project_cache.analyses();
    let actions = h.request(931, "textDocument/codeAction", json!({"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":20}},"context":{"only":["quickfix"]}})).await;
    assert_eq!(
        array_result(&actions, "repaired project actions").len(),
        1,
        "{actions:?}"
    );
    assert_eq!(
        h.engine.project_cache.analyses(),
        before,
        "actions repeated an identical proof"
    );
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn watched_invalid_project_inputs_report_their_source(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let uri = fixture.write("main", "fn main() = print(\"ready\")\n")?;
    let mut h = Harness::start();
    let _ = h.open_at(&uri, "fn main() = print(\"ready\")\n").await;
    let helper = fixture.write("helper", "fn helper(\n")?;
    assert_load_error(&mut h, &helper, "helper.osp").await;
    let manifest = fixture.root.join("osprey.toml");
    std::fs::write(&manifest, "[project]\nsource_roots = invalid\n")?;
    assert_load_error(
        &mut h,
        &lspkit_server::uri::path_to_uri(&manifest)?,
        "osprey.toml",
    )
    .await;
    h.shutdown_and_exit().await;
    Ok(())
}

async fn assert_load_error(h: &mut Harness, changed: &str, path: &str) {
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":changed,"type":2}]}),
    )
    .await;
    let reports = drain_reports(h).await;
    let text = format!("{reports:?}");
    assert!(
        text.contains("project-error") && text.contains(path),
        "{reports:?}"
    );
}

#[tokio::test]
async fn manifest_flavor_controls_diagnostics_and_features_together(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let manifest = fixture.root.join("osprey.toml");
    let config = format!("{}flavor = \"ml\"\n", std::fs::read_to_string(&manifest)?);
    std::fs::write(&manifest, &config)?;
    let main = "main () = print (fresh \"x\")\n";
    let uri = fixture.write("main", main)?;
    let sibling = fixture.write("helper", "fresh x = x + \"!\"\n")?;
    let mut h = Harness::start();
    let opened = h.open_at(&uri, main).await;
    assert_eq!(
        opened
            .params
            .as_ref()
            .and_then(|params| params.get("diagnostics")),
        Some(&json!([])),
        "{opened:?}"
    );
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
    assert_eq!(
        array_result(&actions, "manifest-flavor annotation fixes").len(),
        2
    );
    std::fs::write(&manifest, config.replace("\"ml\"", "\"default\""))?;
    h.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":lspkit_server::uri::path_to_uri(&manifest)?,"type":2}]}),
    )
    .await;
    let reports = drain_reports(&mut h).await;
    assert!(
        format!("{reports:?}").contains("syntax-error"),
        "{reports:?}"
    );
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn files_outside_source_roots_keep_standalone_flavor_and_symbols(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let _ = fixture.write("main", "main () = print \"project\"\n")?;
    let _ = fixture.write("helper", "projectOnly x = x\n")?;
    let manifest = fixture.root.join("osprey.toml");
    std::fs::write(
        &manifest,
        format!("{}flavor = \"ml\"\n", std::fs::read_to_string(&manifest)?),
    )?;
    let uri = lspkit_server::uri::path_to_uri(&fixture.root.join("scratch.osp"))?;
    let mut h = Harness::start();
    let opened = h.open_at(&uri, "fn main() = print(\"standalone\")\n").await;
    assert_eq!(
        opened
            .params
            .as_ref()
            .and_then(|params| params.get("diagnostics")),
        Some(&json!([])),
        "{opened:?}"
    );
    let completion = h
        .request(950, "textDocument/completion", position_params(&uri, 0, 0))
        .await;
    assert!(
        !format!("{completion:?}").contains("projectOnly"),
        "{completion:?}"
    );
    h.shutdown_and_exit().await;
    Ok(())
}

#[tokio::test]
async fn nested_project_edits_refresh_every_project_that_includes_the_source(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = crate::test_support::ProjectFixture::new("osp")?;
    let main = "fn main() = print(fresh(\"x\"))\n";
    let uri = fixture.write("main", main)?;
    let nested = fixture.root.join("src/nested");
    std::fs::create_dir_all(nested.join("src"))?;
    std::fs::write(nested.join("osprey.toml"), "[project]\nsource_roots = [\"src\"]\ndefault_namespace = \"nested\"\nentry = \"src/helper.osp\"\n")?;
    let helper = nested.join("src/helper.osp");
    std::fs::write(&helper, "fn saved(x) = x\n")?;
    let sibling = lspkit_server::uri::path_to_uri(&helper)?;
    let mut h = Harness::start();
    let initial = h.open_at(&uri, main).await;
    assert!(format!("{initial:?}").contains("type-error"), "{initial:?}");
    let _ = h.open_at(&sibling, "fn fresh(x) = x + \"!\"\n").await;
    let reports = drain_reports(&mut h).await;
    assert!(
        reports.iter().any(
            |report| report.get("uri").and_then(Value::as_str) == Some(&uri)
                && report.get("diagnostics") == Some(&json!([]))
        ),
        "{reports:?}"
    );
    h.notify("textDocument/didClose", text_doc(&sibling)).await;
    let reports = drain_reports(&mut h).await;
    assert!(format!("{reports:?}").contains("type-error"), "{reports:?}");
    h.shutdown_and_exit().await;
    Ok(())
}
