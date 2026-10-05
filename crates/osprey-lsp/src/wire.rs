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
const SYMBOL_MODULE: u8 = 2;
const SYMBOL_NAMESPACE: u8 = 3;
const SYMBOL_CLASS: u8 = 5;
const SYMBOL_INTERFACE: u8 = 11;
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
pub(crate) fn doc_uri(params: &Value) -> Option<String> {
    nested(params, "textDocument", "uri")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The `(line, character)` of a request's `position`, if present.
#[must_use]
pub(crate) fn position(params: &Value) -> Option<(u32, u32)> {
    let pos = params.get("position")?;
    Some((field_u32(pos, "line"), field_u32(pos, "character")))
}

/// Whether a references request asks to include the declaration.
#[must_use]
pub(crate) fn include_declaration(params: &Value) -> bool {
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
pub(crate) fn open_text(params: &Value) -> Option<String> {
    nested(params, "textDocument", "text")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The document version of a `didOpen`/`didChange`, defaulting to 0.
#[must_use]
pub(crate) fn version(params: &Value) -> i32 {
    nested(params, "textDocument", "version")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0)
}

/// A `didChange` content change: either an incremental edit (`Ok`) or a
/// whole-document replacement (`Err(full_text)`).
#[must_use]
pub(crate) fn content_changes(params: &Value) -> Vec<Result<TextEdit, String>> {
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

pub(crate) fn action_range(params: &Value) -> Option<Span> {
    let range = params.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;
    let number = |value: &Value, name| u32::try_from(value.get(name)?.as_u64()?).ok();
    let result = (
        number(start, "line")?,
        number(start, "character")?,
        number(end, "line")?,
        number(end, "character")?,
    );
    ((result.0, result.1) <= (result.2, result.3)).then_some(result)
}

pub(crate) fn action_kinds(params: &Value) -> Vec<String> {
    nested(params, "context", "only")
        .and_then(Value::as_array)
        .map(|kinds| {
            kinds
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn code_actions_result(actions: &[crate::model::CodeAction]) -> Value {
    Value::Array(actions.iter().map(|action| json!({
        "title": action.title, "kind": action.kind, "isPreferred": true,
        "data": { "uri": action.uri, "version": action.version },
        "diagnostics": action.diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
        "edit": { "documentChanges": [{
            "textDocument": { "uri": action.uri, "version": action.version },
            "edits": action.edits.iter().map(|edit| json!({
                "range": range_json(edit.range), "newText": edit.new_text
            })).collect::<Vec<_>>()
        }] }
    })).collect())
}

/// The `initialize` result advertising the server's capabilities.
/// Implements [LSP-CAPABILITIES] and [LSP-ENCODING].
#[must_use]
pub(crate) fn initialize_result(encoding: &str) -> Value {
    json!({
        "capabilities": {
            "positionEncoding": encoding,
            "textDocumentSync": 2,
            "hoverProvider": true,
            "definitionProvider": true,
            "implementationProvider": true,
            "referencesProvider": true,
            "documentSymbolProvider": true,
            "documentFormattingProvider": true,
            "codeActionProvider": { "codeActionKinds": ["quickfix", crate::code_actions::FIX_ALL] },
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
pub(crate) fn hover_result(markdown: Option<String>) -> Value {
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
pub(crate) fn locations_result(locations: &[Location]) -> Value {
    Value::Array(locations.iter().map(location_json).collect())
}

/// `textDocument/formatting` result: a single whole-document `TextEdit` when the
/// formatter changed anything, or an empty array when the buffer is already
/// formatted (so the editor records no change).
#[must_use]
pub(crate) fn formatting_result(
    formatted: &str,
    original: &str,
    encoding: PositionEncoding,
) -> Value {
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
pub(crate) fn symbols_result(
    symbols: &[SymbolInfo],
    text: &str,
    encoding: PositionEncoding,
) -> Value {
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
        crate::analysis::SymbolKind::Namespace => SYMBOL_NAMESPACE,
        crate::analysis::SymbolKind::Module => SYMBOL_MODULE,
        crate::analysis::SymbolKind::Signature => SYMBOL_INTERFACE,
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
    occurrences(text, &s.source_name, encoding)
        .into_iter()
        .find(|o| o.line == line)
        .map_or_else(
            || {
                let col = s.position.map_or(0, |p| p.column);
                (
                    line,
                    col,
                    line,
                    col.saturating_add(measure(&s.source_name, encoding)),
                )
            },
            |o| (o.line, o.start, o.line, o.end),
        )
}

/// `textDocument/signatureHelp` result, or JSON `null`.
#[must_use]
pub(crate) fn signature_result(info: Option<SignatureInfo>) -> Value {
    info.map_or(Value::Null, |s| {
        let params: Vec<Value> = s.parameters.iter().map(|p| json!({ "label": p })).collect();
        let mut signature = json!({ "label": s.label, "parameters": params });
        insert_opt(
            &mut signature,
            "documentation",
            markdown(s.documentation.as_deref()),
        );
        json!({
            "signatures": [signature],
            "activeSignature": 0,
            "activeParameter": s.active_parameter
        })
    })
}

/// `textDocument/completion` result: an array of completion items.
#[must_use]
pub(crate) fn completion_result(items: &[CompletionItem]) -> Value {
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
    insert_opt(
        &mut obj,
        "documentation",
        markdown(item.documentation.as_deref()),
    );
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

fn markdown(text: Option<&str>) -> Option<Value> {
    text.map(|value| json!({ "kind": "markdown", "value": value }))
}

fn insert_opt(obj: &mut Value, key: &str, value: Option<Value>) {
    if let (Some(map), Some(value)) = (obj.as_object_mut(), value) {
        let _ = map.insert(key.to_owned(), value);
    }
}

/// `textDocument/publishDiagnostics` params for `uri`.
/// Implements [LSP-DIAGNOSTICS].
#[must_use]
pub(crate) fn publish_diagnostics(uri: &str, diagnostics: &[Diagnostic]) -> Value {
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
    if d.code
        .as_deref()
        .is_some_and(|code| code.starts_with("unused-"))
    {
        insert_opt(&mut obj, "tags", Some(json!([1])));
    }
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
mod tests;
