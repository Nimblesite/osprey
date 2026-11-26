//! Current-buffer actions, derived from the same safe set as diagnostics.
//! Implements [LSP-CODE-ACTIONS-ANNOTATIONS].

use lspkit::vfs::{DocumentUri, PositionEncoding, Vfs};
use lspkit_server::Diagnostic;

use crate::model::{CodeAction, Span, TextChange};
use crate::text::measure;
use crate::warning_actions::Fix;

pub(crate) const FIX_ALL: &str = "source.fixAll.osprey";

/// The project warning whose import [`import_removal`] deletes.
const UNUSED_IMPORT: &str = "unused-import";

pub(crate) fn actions(
    vfs: &Vfs,
    uri: &DocumentUri,
    range: Span,
    only: &[String],
    cache: &crate::project_cache::ProjectCache,
) -> Vec<CodeAction> {
    let (Some(version), Some(source)) = (vfs.version(uri), vfs.text(uri)) else {
        return Vec::new();
    };
    let analysis = crate::diagnostics::analyze_cached(
        &source,
        uri.as_str(),
        vfs.encoding(),
        Some(vfs),
        cache,
        true,
    );
    let mut actions = Vec::new();
    if includes(only, "quickfix") {
        actions.extend(crate::module_actions::actions(
            vfs,
            uri,
            range,
            cache,
            &analysis.diagnostics,
        ));
        actions.extend(
            analysis
                .fixes
                .iter()
                .filter(|fix| overlaps(range, fix.diagnostic.range))
                .map(|fix| action(uri, version.get(), &[fix], false)),
        );
        actions.extend(
            analysis
                .diagnostics
                .iter()
                .filter(|diagnostic| overlaps(range, diagnostic.range))
                .filter_map(|diagnostic| {
                    import_removal(uri, version.get(), &source, vfs.encoding(), diagnostic)
                }),
        );
    }
    if includes(only, FIX_ALL) && !analysis.fixes.is_empty() {
        let fixes: Vec<_> = analysis.fixes.iter().collect();
        actions.push(action(uri, version.get(), &fixes, true));
    }
    actions
}

fn action(uri: &DocumentUri, version: i32, fixes: &[&Fix], all: bool) -> CodeAction {
    let title = if all {
        "Remove all redundant type annotations"
    } else if fixes.first().is_some_and(|fix| fix.signature) {
        "Remove redundant type signature"
    } else {
        "Remove redundant type annotation"
    };
    CodeAction {
        title,
        kind: if all { FIX_ALL } else { "quickfix" },
        uri: uri.as_str().to_string(),
        version,
        diagnostics: fixes.iter().map(|fix| fix.diagnostic.clone()).collect(),
        edits: fixes.iter().flat_map(|fix| fix.edits.clone()).collect(),
    }
}

/// The quick fix for an `unused-import` warning: delete the lines the import
/// is written on. Offered only when nothing else is written on them, so the
/// edit removes the import and nothing more ([LSP-CODE-ACTIONS-IMPORTS]).
fn import_removal(
    uri: &DocumentUri,
    version: i32,
    source: &str,
    encoding: PositionEncoding,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    if diagnostic.code.as_deref() != Some(UNUSED_IMPORT) {
        return None;
    }
    let (start, column, end, end_column) = diagnostic.range;
    let lines: Vec<&str> = source.lines().collect();
    let first = lines.get(usize::try_from(start).ok()?)?;
    let last = lines.get(usize::try_from(end).ok()?)?;
    let alone = column == measure(first, encoding) - measure(first.trim_start(), encoding)
        && first.trim_start().starts_with("import")
        && end_column == measure(last.trim_end(), encoding);
    alone.then(|| CodeAction {
        title: "Remove unused import",
        kind: "quickfix",
        uri: uri.as_str().to_string(),
        version,
        diagnostics: vec![diagnostic.clone()],
        edits: vec![TextChange {
            range: (start, 0, end + 1, 0),
            new_text: String::new(),
        }],
    })
}

fn includes(only: &[String], kind: &str) -> bool {
    only.is_empty()
        || only.iter().any(|parent| {
            parent.is_empty()
                || parent == kind
                || kind
                    .strip_prefix(parent)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        })
}

pub(crate) fn overlaps(selection: Span, diagnostic: Span) -> bool {
    let (start, end) = ((selection.0, selection.1), (selection.2, selection.3));
    let (low, high) = ((diagnostic.0, diagnostic.1), (diagnostic.2, diagnostic.3));
    if start == end {
        low <= start && start <= high
    } else {
        start < high && low < end
    }
}

#[cfg(test)]
mod tests;
