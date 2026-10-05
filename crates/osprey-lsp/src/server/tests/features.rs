use super::*;

/// [LSP-EFFECT-REQUIREMENTS] All wire views follow the current unsaved body.
#[tokio::test]
async fn live_effect_requirement_views_refresh_in_both_flavors() -> Result<(), String> {
    for (uri, source, performed, line, column) in [
        (URI, "effect Read { get: fn() -> int }\nfn leaf() = perform Read.get()\nfn relay() = leaf()\n", "perform Read.get()", 2, 4),
        (ML_URI, "effect Read\n    get : Unit => int\nleaf () = perform Read.get ()\nrelay () = leaf ()\n", "perform Read.get ()", 3, 1),
    ] {
        let mut h = Harness::start();
        let _ = h.open_at(uri, source).await;
        assert_requirement_views(&mut h, uri, line, column, Some("Requires on full application: `Read.get`.")).await?;
        h.notify("textDocument/didChange", json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":source.replace(performed, "42")}]})).await;
        let _ = h.read_message().await;
        assert_requirement_views(&mut h, uri, line, column, None).await?;
        h.shutdown_and_exit().await;
    }
    Ok(())
}

async fn assert_requirement_views(
    h: &mut Harness,
    uri: &str,
    line: u32,
    column: u32,
    expected: Option<&str>,
) -> Result<(), String> {
    let hover = h
        .request(
            701,
            "textDocument/hover",
            position_params(uri, line, column),
        )
        .await
        .result
        .ok_or("hover")?;
    let text = hover
        .pointer("/contents/value")
        .and_then(Value::as_str)
        .ok_or("hover text")?;
    assert_eq!(
        text.contains("Requires on full application:"),
        expected.is_some(),
        "{text}"
    );
    if let Some(expected) = expected {
        assert!(text.contains(expected), "{text}");
    }
    let signature = h
        .request(
            702,
            "textDocument/signatureHelp",
            position_params(uri, line, column),
        )
        .await
        .result
        .ok_or("signature")?;
    assert_eq!(
        signature
            .pointer("/signatures/0/documentation/value")
            .and_then(Value::as_str),
        expected
    );
    let completion = h
        .request(
            703,
            "textDocument/completion",
            position_params(uri, line, column),
        )
        .await;
    let items = array_result(&completion, "completion");
    let relay = find_by(&items, "/label", "relay");
    assert_eq!(
        relay
            .pointer("/documentation/value")
            .and_then(Value::as_str),
        expected
    );
    Ok(())
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
async fn effect_operation_implementation_request_returns_matching_handler_arms() {
    let src = concat!(
        "// osprey: flavor=ml\n",
        "effect Trace\n",
        "    mark : string => Unit\n",
        "effect Other\n",
        "    mark : string => Unit\n",
        "traced () = perform Trace.mark \"one\"\n",
        "first =\n",
        "    handle Trace\n",
        "        mark label => print label\n",
        "    in traced ()\n",
        "wrong =\n",
        "    handle Other\n",
        "        mark label => print label\n",
        "    in perform Other.mark \"other\"\n",
        "second =\n",
        "    handle Trace\n",
        "        mark label => print label\n",
        "    in traced ()\n",
    );
    let mut h = Harness::start();
    let _diags = h.open_at(ML_URI, src).await;

    let response = h
        .request(
            13,
            "textDocument/implementation",
            position_params(ML_URI, 5, 27),
        )
        .await;
    assert!(
        response.error.is_none(),
        "implementation request must be registered: {response:?}"
    );
    let locations = array_result(&response, "implementation");
    let lines: Vec<u64> = locations
        .iter()
        .filter_map(|location| location.pointer("/range/start/line")?.as_u64())
        .collect();
    assert_eq!(lines, [8, 16], "only Trace.mark handlers: {locations:?}");
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
