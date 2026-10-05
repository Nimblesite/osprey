use super::*;

/// Assert that the JSON pointer `pointer` resolves to nothing on `value`.
fn assert_absent(value: &Value, pointer: &str) {
    assert_eq!(value.pointer(pointer), None, "{pointer}");
}

/// Parse `src`, collect its symbols, and render the `documentSymbol` JSON —
/// the shared setup for every symbol-shaping test.
fn symbols_value(src: &str) -> Value {
    let parsed = osprey_syntax::parse_program(src);
    let syms = crate::analysis::collect_symbols(&parsed.program);
    symbols_result(&syms, src, PositionEncoding::Utf16)
}

/// The `publishDiagnostics` JSON for a single diagnostic at the test URI.
fn published(diag: Diagnostic) -> Value {
    publish_diagnostics("file:///a.osp", &[diag])
}

#[test]
fn position_and_uri_parse_from_params() {
    let params = json!({
        "textDocument": { "uri": "file:///a.osp" },
        "position": { "line": 3, "character": 7 }
    });
    assert_eq!(doc_uri(&params).as_deref(), Some("file:///a.osp"));
    assert_eq!(position(&params), Some((3, 7)));
}

#[test]
fn content_changes_split_incremental_and_full() {
    let params = json!({ "contentChanges": [
        { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } }, "text": "x" },
        { "text": "whole file" }
    ] });
    let changes = content_changes(&params);
    assert!(matches!(changes.first(), Some(Ok(_))));
    assert!(matches!(changes.get(1), Some(Err(t)) if t == "whole file"));
}

#[test]
fn hover_and_diagnostics_render_expected_shape() {
    // The push payload shape is part of [LSP-DIAGNOSTICS].
    assert_eq!(hover_result(None), Value::Null);
    let hov = hover_result(Some("**x**".to_owned()));
    assert_at(&hov, "/contents/kind", "markdown");
    let diag = Diagnostic::new(Severity::Error, "boom", (1, 2, 1, 5)).with_source("osprey");
    let value = published(diag);
    assert_at(&value, "/diagnostics/0/severity", 1);
    assert_at(&value, "/diagnostics/0/source", "osprey");
}

#[test]
fn document_symbol_range_lands_on_the_identifier_not_the_keyword() {
    let value = symbols_value("fn add(a: int) -> int = a\n");
    // `add` is at column 3; the `fn` keyword is at column 0.
    assert_at(&value, "/0/name", "add");
    assert_at(&value, "/0/range/start/character", 3);
    assert_at(&value, "/0/range/end/character", 6);
}

#[test]
fn symbols_result_maps_every_symbol_kind_to_its_lsp_code() {
    // [MODULES-NAMESPACE] Container kinds use their dedicated LSP codes.
    let value = symbols_value(
        "namespace app { module M { export fn f() -> Unit = 1 } }\n\
         signature Api { fn f() -> Unit }\n\
         type Shade = Light | Dark\nlet x = 2\n",
    );
    let by_name = |name: &str| {
        value
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|s| s.pointer("/name") == Some(&Value::from(name)))
            })
            .and_then(|s| s.pointer("/kind"))
            .cloned()
    };
    assert_eq!(by_name("app"), Some(Value::from(SYMBOL_NAMESPACE)));
    assert_eq!(by_name("app::M"), Some(Value::from(SYMBOL_MODULE)));
    assert_eq!(by_name("Api"), Some(Value::from(SYMBOL_INTERFACE)));
    assert_eq!(by_name("Shade"), Some(Value::from(SYMBOL_CLASS)));
    assert_eq!(by_name("app::M::f"), Some(Value::from(SYMBOL_FUNCTION)));
    assert_eq!(by_name("x"), Some(Value::from(SYMBOL_VARIABLE)));
}

#[test]
fn identifier_span_falls_back_to_the_keyword_column_when_name_is_absent() {
    use crate::analysis::{SymbolInfo, SymbolKind};
    use osprey_ast::Position;
    // The declaration's recorded line text never contains the name, so the
    // occurrence scan misses and the keyword-column fallback is used.
    let sym = SymbolInfo {
        name: "ghost".to_owned(),
        source_name: "ghost".to_owned(),
        kind: SymbolKind::Variable,
        ty: "int".to_owned(),
        position: Some(Position { line: 1, column: 4 }),
        signature: None,
        binder: String::new(),
        parameters: Vec::new(),
        return_type: None,
        declared_effect_row: None,
        effect_requirements: None,
        doc: None,
    };
    let value = symbols_result(&[sym], "let other = 1\n", PositionEncoding::Utf16);
    // Fallback span starts at the keyword column (4) and spans the name width (5).
    assert_at(&value, "/0/range/start/character", 4);
    assert_at(&value, "/0/range/end/character", 9);
}

#[test]
fn signature_result_handles_none_and_renders_parameters() {
    use crate::model::SignatureInfo;
    assert_eq!(signature_result(None), Value::Null);
    let info = SignatureInfo {
        label: "fn add(a: int, b: int) -> int".to_owned(),
        parameters: vec!["a: int".to_owned(), "b: int".to_owned()],
        documentation: None,
        active_parameter: 1,
    };
    let value = signature_result(Some(info));
    assert_at(&value, "/activeSignature", 0);
    assert_at(&value, "/activeParameter", 1);
    assert_at(
        &value,
        "/signatures/0/label",
        "fn add(a: int, b: int) -> int",
    );
    assert_at(&value, "/signatures/0/parameters/1/label", "b: int");
}

#[test]
fn completion_result_maps_kinds_and_carries_snippet_format() {
    use crate::model::{CompletionItem, CompletionKind};
    let items = vec![
        CompletionItem {
            label: "match".to_owned(),
            kind: CompletionKind::Keyword,
            detail: Some("Pattern matching".to_owned()),
            documentation: None,
            insert_text: Some("match ${1:x} {}".to_owned()),
        },
        CompletionItem {
            label: "Shade".to_owned(),
            kind: CompletionKind::Type,
            detail: None,
            documentation: None,
            insert_text: None,
        },
    ];
    let value = completion_result(&items);
    assert_at(&value, "/0/kind", COMPLETION_KEYWORD);
    assert_at(&value, "/0/detail", "Pattern matching");
    // A snippet insert text also carries the snippet `insertTextFormat`.
    assert_at(&value, "/0/insertTextFormat", INSERT_SNIPPET);
    // A type item maps to the class completion kind and omits detail/insert.
    assert_at(&value, "/1/kind", COMPLETION_CLASS);
    assert_absent(&value, "/1/detail");
    assert_absent(&value, "/1/insertText");
}

#[test]
fn formatting_result_spans_the_whole_document_or_is_empty() {
    // An unchanged buffer yields no edits.
    let same = formatting_result("a\n", "a\n", PositionEncoding::Utf16);
    assert_eq!(same.as_array().map(Vec::len), Some(0));
    // A changed buffer yields one edit spanning (0,0)..(end_line, end_char).
    let edit = formatting_result("x\ny\n", "x\n  y\n", PositionEncoding::Utf16);
    assert_at(&edit, "/0/newText", "x\ny\n");
    assert_at(&edit, "/0/range/start/line", 0);
    assert_at(&edit, "/0/range/start/character", 0);
    // The original has two newlines, so the end is line 2, character 0.
    assert_at(&edit, "/0/range/end/line", 2);
    assert_at(&edit, "/0/range/end/character", 0);
}

#[test]
fn locations_result_renders_each_location_range() {
    use crate::model::Location;
    let locs = vec![
        Location {
            uri: "file:///a.osp".to_owned(),
            span: (1, 2, 1, 5),
        },
        Location {
            uri: "file:///b.osp".to_owned(),
            span: (0, 0, 0, 3),
        },
    ];
    let value = locations_result(&locs);
    assert_eq!(value.as_array().map(Vec::len), Some(2));
    assert_at(&value, "/0/uri", "file:///a.osp");
    assert_at(&value, "/0/range/start/line", 1);
    assert_at(&value, "/0/range/end/character", 5);
    assert_at(&value, "/1/range/start/character", 0);
}

#[test]
fn diagnostic_json_maps_each_severity_and_optional_fields() {
    // Warning, Information, and Hint each get their distinct LSP code; an
    // error with a code carries it through.
    let cases = [
        (Severity::Warning, 2),
        (Severity::Information, 3),
        (Severity::Hint, 4),
        (Severity::Error, 1),
    ];
    for (severity, code) in cases {
        let diag = Diagnostic::new(severity, "msg", (0, 0, 0, 1));
        let value = published(diag);
        assert_eq!(
            value.pointer("/diagnostics/0/severity"),
            Some(&Value::from(code)),
            "{severity:?}"
        );
        // Without a code, the field is omitted.
        assert_absent(&value, "/diagnostics/0/code");
    }
    let coded = Diagnostic::new(Severity::Error, "boom", (0, 0, 0, 1)).with_code("type-error");
    let value = published(coded);
    assert_at(&value, "/diagnostics/0/code", "type-error");
}

#[test]
fn parsing_helpers_default_and_round_trip_request_fields() {
    // `open_text` and `version` read the didOpen payload.
    let open = json!({ "textDocument": { "uri": "file:///a.osp", "version": 7, "text": "hi" } });
    assert_eq!(open_text(&open).as_deref(), Some("hi"));
    assert_eq!(version(&open), 7);
    // Missing version/text default cleanly.
    let bare = json!({ "textDocument": { "uri": "file:///a.osp" } });
    assert_eq!(open_text(&bare), None);
    assert_eq!(version(&bare), 0);
    // `include_declaration` reads the references context, defaulting to false.
    let with_ctx = json!({ "context": { "includeDeclaration": true } });
    assert!(include_declaration(&with_ctx));
    assert!(!include_declaration(&json!({})));
}

#[test]
fn action_ranges_reject_missing_negative_overflow_and_reversed_positions() {
    let valid = json!({"range":{"start":{"line":2,"character":3},"end":{"line":2,"character":5}},"context":{"only":["quickfix",7,"source.fixAll"]}});
    assert_eq!(action_range(&valid), Some((2, 3, 2, 5)));
    assert_eq!(action_kinds(&valid), ["quickfix", "source.fixAll"]);
    assert_eq!(action_kinds(&json!({})), Vec::<String>::new());
    assert_eq!(action_range(&json!({})), None);
    for (path, replacement) in [
        ("/range/start/line", Value::Null),
        ("/range/start/character", json!(-1)),
        ("/range/end/line", json!(u64::from(u32::MAX) + 1)),
        ("/range/end/line", json!(1)),
        ("/range/end/character", json!(2)),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(path).expect("fixture field") = replacement;
        assert_eq!(action_range(&invalid), None, "{invalid}");
    }
}

#[test]
fn initialize_result_advertises_the_full_capability_set() {
    // [LSP-CAPABILITIES], [LSP-ENCODING]
    let value = initialize_result("utf-16");
    assert_at(&value, "/capabilities/positionEncoding", "utf-16");
    assert_at(&value, "/capabilities/textDocumentSync", 2);
    assert_eq!(
        value.pointer("/capabilities/codeActionProvider/codeActionKinds"),
        Some(&json!(["quickfix", "source.fixAll.osprey"]))
    );
    assert_at(&value, "/capabilities/referencesProvider", true);
    assert_at(&value, "/capabilities/implementationProvider", true);
    assert_at(&value, "/capabilities/documentSymbolProvider", true);
    assert_at(&value, "/capabilities/documentFormattingProvider", true);
    assert_at(
        &value,
        "/capabilities/completionProvider/resolveProvider",
        false,
    );
    // The completion trigger characters include the pipe and dollar.
    let triggers = value
        .pointer("/capabilities/completionProvider/triggerCharacters")
        .and_then(Value::as_array)
        .expect("trigger characters");
    assert!(triggers.contains(&Value::from(".")));
    assert!(triggers.contains(&Value::from("$")));
    assert_at(
        &value,
        "/capabilities/signatureHelpProvider/triggerCharacters/0",
        "(",
    );
}
