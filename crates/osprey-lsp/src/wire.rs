//! JSON-RPC payload conversion.
//!
//! Incoming LSP params are read straight off `serde_json::Value` (panic-free
//! `.get`/`.as_*` accessors); outgoing results are built with `json!`. Keeping
//! the wire shape here lets the rest of the server speak the neutral
//! [`crate::model`] vocabulary.

use lspkit_server::{Diagnostic, Severity};
use lspkit_vfs::{Position, PositionEncoding, Range, TextEdit};
use serde_json::{json, Value};

use crate::analysis::SymbolInfo;
use crate::model::{CompletionItem, CompletionKind, Location, SignatureInfo, Span};
use crate::text::{measure, occurrences};

// LSP `SymbolKind` numeric codes.
const SYMBOL_CLASS: u8 = 5;
const SYMBOL_FUNCTION: u8 = 12;
const SYMBOL_VARIABLE: u8 = 13;
// LSP `CompletionItemKind` numeric codes.
const COMPLETION_FUNCTION: u8 = 3;
const COMPLETION_VARIABLE: u8 = 6;
const COMPLETION_CLASS: u8 = 7;
const COMPLETION_KEYWORD: u8 = 14;
// LSP `InsertTextFormat`: snippet.
const INSERT_SNIPPET: u8 = 2;

/// The value at `params[outer][inner]`, if both levels are present — the shared
/// two-level lookup behind every request-field accessor below.
fn nested<'a>(params: &'a Value, outer: &str, inner: &str) -> Option<&'a Value> {
    params.get(outer).and_then(|o| o.get(inner))
}

/// The document URI of a request's `textDocument`, if present.
#[must_use]
pub fn doc_uri(params: &Value) -> Option<String> {
    nested(params, "textDocument", "uri")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The `(line, character)` of a request's `position`, if present.
#[must_use]
pub fn position(params: &Value) -> Option<(u32, u32)> {
    let pos = params.get("position")?;
    Some((field_u32(pos, "line"), field_u32(pos, "character")))
}

/// Whether a references request asks to include the declaration.
#[must_use]
pub fn include_declaration(params: &Value) -> bool {
    nested(params, "context", "includeDeclaration")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn field_u32(value: &Value, key: &str) -> u32 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

/// The full text of a `textDocument/didOpen`.
#[must_use]
pub fn open_text(params: &Value) -> Option<String> {
    nested(params, "textDocument", "text")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The document version of a `didOpen`/`didChange`, defaulting to 0.
#[must_use]
pub fn version(params: &Value) -> i32 {
    nested(params, "textDocument", "version")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
}

/// A `didChange` content change: either an incremental edit (`Ok`) or a
/// whole-document replacement (`Err(full_text)`).
#[must_use]
pub fn content_changes(params: &Value) -> Vec<Result<TextEdit, String>> {
    params
        .get("contentChanges")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(change_event).collect())
        .unwrap_or_default()
}

fn change_event(change: &Value) -> Result<TextEdit, String> {
    let text = change
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match change.get("range").and_then(range_of) {
        Some(range) => Ok(TextEdit::new(range, text)),
        None => Err(text),
    }
}

fn range_of(range: &Value) -> Option<Range> {
    let start = range.get("start")?;
    let end = range.get("end")?;
    Some(Range::new(
        Position::new(field_u32(start, "line"), field_u32(start, "character")),
        Position::new(field_u32(end, "line"), field_u32(end, "character")),
    ))
}

/// The `initialize` result advertising the server's capabilities.
#[must_use]
pub fn initialize_result(encoding: &str) -> Value {
    json!({
        "capabilities": {
            "positionEncoding": encoding,
            "textDocumentSync": 2,
            "hoverProvider": true,
            "definitionProvider": true,
            "referencesProvider": true,
            "documentSymbolProvider": true,
            "documentFormattingProvider": true,
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", ":", "$", "(", "|"]
            },
            "signatureHelpProvider": { "triggerCharacters": ["(", ","] }
        },
        "serverInfo": { "name": "osprey-lsp" }
    })
}

/// `textDocument/hover` result, or JSON `null`.
#[must_use]
pub fn hover_result(markdown: Option<String>) -> Value {
    markdown.map_or(
        Value::Null,
        |value| json!({ "contents": { "kind": "markdown", "value": value } }),
    )
}

fn range_json(span: Span) -> Value {
    let (sl, sc, el, ec) = span;
    json!({ "start": { "line": sl, "character": sc }, "end": { "line": el, "character": ec } })
}

fn location_json(loc: &Location) -> Value {
    json!({ "uri": loc.uri, "range": range_json(loc.span) })
}

/// `textDocument/definition` / `references` result: an array of locations.
#[must_use]
pub fn locations_result(locations: &[Location]) -> Value {
    Value::Array(locations.iter().map(location_json).collect())
}

/// `textDocument/formatting` result: a single whole-document `TextEdit` when the
/// formatter changed anything, or an empty array when the buffer is already
/// formatted (so the editor records no change).
#[must_use]
pub fn formatting_result(formatted: &str, original: &str, encoding: PositionEncoding) -> Value {
    if formatted == original {
        return Value::Array(Vec::new());
    }
    json!([{ "range": full_range(original, encoding), "newText": formatted }])
}

/// The range spanning the whole document, from `(0, 0)` to the end of the last
/// line measured in `encoding`.
fn full_range(text: &str, encoding: PositionEncoding) -> Value {
    let last_line = text.rsplit('\n').next().unwrap_or("");
    let end_line = u32::try_from(text.matches('\n').count()).unwrap_or(u32::MAX);
    json!({
        "start": { "line": 0, "character": 0 },
        "end": { "line": end_line, "character": measure(last_line, encoding) }
    })
}

/// `textDocument/documentSymbol` result: a flat list of `DocumentSymbol`s.
#[must_use]
pub fn symbols_result(symbols: &[SymbolInfo], text: &str, encoding: PositionEncoding) -> Value {
    Value::Array(
        symbols
            .iter()
            .map(|s| symbol_json(s, text, encoding))
            .collect(),
    )
}

fn symbol_json(s: &SymbolInfo, text: &str, encoding: PositionEncoding) -> Value {
    let span = identifier_span(s, text, encoding);
    let kind = match s.kind {
        crate::analysis::SymbolKind::Function => SYMBOL_FUNCTION,
        crate::analysis::SymbolKind::Variable => SYMBOL_VARIABLE,
        crate::analysis::SymbolKind::Type => SYMBOL_CLASS,
    };
    json!({
        "name": s.name,
        "detail": s.ty,
        "kind": kind,
        "range": range_json(span),
        "selectionRange": range_json(span)
    })
}

/// The span of a symbol's NAME. The parser records a declaration's position at
/// its keyword (`fn`/`let`/`type`), so scan the declaration line for the first
/// whole-word occurrence of the name; fall back to the keyword column.
fn identifier_span(s: &SymbolInfo, text: &str, encoding: PositionEncoding) -> Span {
    let line = s.position.map_or(0, |p| p.line.saturating_sub(1));
    occurrences(text, &s.name, encoding)
        .into_iter()
        .find(|o| o.line == line)
        .map_or_else(
            || {
                let col = s.position.map_or(0, |p| p.column);
                (
                    line,
                    col,
                    line,
                    col.saturating_add(measure(&s.name, encoding)),
                )
            },
            |o| (o.line, o.start, o.line, o.end),
        )
}

/// `textDocument/signatureHelp` result, or JSON `null`.
#[must_use]
pub fn signature_result(info: Option<SignatureInfo>) -> Value {
    info.map_or(Value::Null, |s| {
        let params: Vec<Value> = s.parameters.iter().map(|p| json!({ "label": p })).collect();
        json!({
            "signatures": [{ "label": s.label, "parameters": params }],
            "activeSignature": 0,
            "activeParameter": s.active_parameter
        })
    })
}

/// `textDocument/completion` result: an array of completion items.
#[must_use]
pub fn completion_result(items: &[CompletionItem]) -> Value {
    Value::Array(items.iter().map(completion_json).collect())
}

fn completion_json(item: &CompletionItem) -> Value {
    let kind = match item.kind {
        CompletionKind::Keyword => COMPLETION_KEYWORD,
        CompletionKind::Function => COMPLETION_FUNCTION,
        CompletionKind::Variable => COMPLETION_VARIABLE,
        CompletionKind::Type => COMPLETION_CLASS,
    };
    let mut obj = json!({ "label": item.label, "kind": kind });
    insert_opt(&mut obj, "detail", item.detail.as_deref().map(Value::from));
    if let Some(text) = &item.insert_text {
        insert_opt(&mut obj, "insertText", Some(Value::from(text.clone())));
        insert_opt(
            &mut obj,
            "insertTextFormat",
            Some(Value::from(INSERT_SNIPPET)),
        );
    }
    obj
}

fn insert_opt(obj: &mut Value, key: &str, value: Option<Value>) {
    if let (Some(map), Some(value)) = (obj.as_object_mut(), value) {
        let _ = map.insert(key.to_owned(), value);
    }
}

/// `textDocument/publishDiagnostics` params for `uri`.
#[must_use]
pub fn publish_diagnostics(uri: &str, diagnostics: &[Diagnostic]) -> Value {
    json!({
        "uri": uri,
        "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>()
    })
}

fn diagnostic_json(d: &Diagnostic) -> Value {
    let severity = match d.severity {
        Severity::Warning => 2,
        Severity::Information => 3,
        Severity::Hint => 4,
        // Error and any future (`#[non_exhaustive]`) severity map to error.
        _ => 1,
    };
    let mut obj = json!({
        "range": range_json(d.range),
        "severity": severity,
        "message": d.message
    });
    insert_opt(&mut obj, "source", d.source.as_deref().map(Value::from));
    insert_opt(&mut obj, "code", d.code.as_deref().map(Value::from));
    obj
}

#[cfg(test)]
/// Assert that `value` carries `expected` at the JSON pointer `pointer`.
pub(crate) fn assert_at(value: &Value, pointer: &str, expected: impl Into<Value>) {
    assert_eq!(value.pointer(pointer), Some(&expected.into()), "{pointer}");
}

#[cfg(test)]
mod tests {
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
        // A type, a function, and a `let` exercise all three `SymbolKind` arms.
        let value = symbols_value("type Shade = Light | Dark\nfn f() -> Unit = 1\nlet x = 2\n");
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
        assert_eq!(by_name("Shade"), Some(Value::from(SYMBOL_CLASS)));
        assert_eq!(by_name("f"), Some(Value::from(SYMBOL_FUNCTION)));
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
            kind: SymbolKind::Variable,
            ty: "int".to_owned(),
            position: Some(Position { line: 1, column: 4 }),
            signature: None,
            parameters: Vec::new(),
            return_type: None,
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
                insert_text: Some("match ${1:x} {}".to_owned()),
            },
            CompletionItem {
                label: "Shade".to_owned(),
                kind: CompletionKind::Type,
                detail: None,
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
        let open =
            json!({ "textDocument": { "uri": "file:///a.osp", "version": 7, "text": "hi" } });
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
    fn initialize_result_advertises_the_full_capability_set() {
        let value = initialize_result("utf-16");
        assert_at(&value, "/capabilities/positionEncoding", "utf-16");
        assert_at(&value, "/capabilities/textDocumentSync", 2);
        assert_at(&value, "/capabilities/referencesProvider", true);
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
}
