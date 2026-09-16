//! Current-buffer actions, derived from the same safe set as diagnostics.
//! Implements [LSP-CODE-ACTIONS-ANNOTATIONS].

use lspkit_vfs::{DocumentUri, Vfs};

use crate::model::{CodeAction, Span};
use crate::warning_actions::Fix;

pub(crate) const FIX_ALL: &str = "source.fixAll.osprey";

pub(crate) fn actions(
    vfs: &Vfs,
    uri: &DocumentUri,
    range: Span,
    only: &[String],
) -> Vec<CodeAction> {
    let (Some(version), Some(source)) = (vfs.version(uri), vfs.text(uri)) else {
        return Vec::new();
    };
    let analysis =
        crate::diagnostics::analyze_live(&source, uri.as_str(), vfs.encoding(), Some(vfs));
    let mut actions = Vec::new();
    if includes(only, "quickfix") {
        actions.extend(
            analysis
                .fixes
                .iter()
                .filter(|fix| overlaps(range, fix.diagnostic.range))
                .map(|fix| action(uri, version.get(), &[fix], false)),
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

fn overlaps(selection: Span, diagnostic: Span) -> bool {
    let (start, end) = ((selection.0, selection.1), (selection.2, selection.3));
    let (low, high) = ((diagnostic.0, diagnostic.1), (diagnostic.2, diagnostic.3));
    if start == end {
        low <= start && start <= high
    } else {
        start < high && low < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lspkit_vfs::{DocumentVersion, Position, PositionEncoding, Range, TextEdit};

    const SOURCE: &str = "decorate : string -> string\ndecorate text = text + \"!\"\n";

    fn document(source: &str, flavor: &str) -> (Vfs, DocumentUri) {
        let vfs = Vfs::new(PositionEncoding::Utf16);
        let uri = DocumentUri::new(format!("file:///annotation-action.{flavor}"));
        vfs.open(uri.clone(), source, DocumentVersion::new(7));
        (vfs, uri)
    }

    fn apply(vfs: &Vfs, uri: &DocumentUri, action: &CodeAction) -> String {
        let mut changes = action.edits.clone();
        changes.sort_by_key(|edit| std::cmp::Reverse(edit.range));
        let edits: Vec<_> = changes
            .into_iter()
            .map(|edit| {
                let (sl, sc, el, ec) = edit.range;
                TextEdit::new(
                    Range::new(Position::new(sl, sc), Position::new(el, ec)),
                    edit.new_text,
                )
            })
            .collect();
        vfs.change(uri, &edits, DocumentVersion::new(action.version + 1))
            .expect("valid source edits");
        vfs.text(uri).expect("open source")
    }

    #[test]
    fn action_filters_selection_and_kind_without_trusting_client_diagnostics() {
        let (vfs, uri) = document(SOURCE, "ospml");
        let quick = actions(&vfs, &uri, (0, 2, 0, 2), &["quickfix".to_string()]);
        assert_eq!(quick.len(), 1);
        let quick = quick.first().expect("one quick fix");
        assert_eq!(quick.version, 7);
        assert_eq!(
            quick.diagnostics.first().expect("one diagnostic").range,
            (0, 0, 0, 27)
        );
        assert!(actions(&vfs, &uri, (1, 0, 1, 5), &["quickfix".to_string()]).is_empty());
        assert!(actions(&vfs, &uri, (0, 0, 0, 5), &["refactor".to_string()]).is_empty());
        let all = actions(&vfs, &uri, (1, 0, 1, 0), &["source.fixAll".to_string()]);
        assert_eq!(all.len(), 1);
        assert_eq!(all.first().expect("one fix-all").kind, FIX_ALL);
        assert_eq!(actions(&vfs, &uri, (0, 0, 0, 2), &[]).len(), 2);
        assert_eq!(actions(&vfs, &uri, (0, 0, 0, 2), &[String::new()]).len(), 2);
        assert_eq!(apply(&vfs, &uri, quick), "decorate text = text + \"!\"\n");
        assert!(actions(&vfs, &uri, (0, 0, 0, 2), &[]).is_empty());
        assert!(crate::diagnostics::compute(
            &vfs.text(&uri).expect("text"),
            uri.as_str(),
            vfs.encoding()
        )
        .is_empty());
    }

    #[test]
    fn fix_all_preserves_the_annotation_that_holds_a_comparison_at_int() {
        let source = "fn pick(a: int, b: int) -> int = if a < b { a } else { b }\n";
        let (vfs, uri) = document(source, "osp");
        let all = actions(&vfs, &uri, (0, 0, 0, 0), &[FIX_ALL.to_string()]);
        assert_eq!(all.len(), 1);
        assert_eq!(
            all.first().expect("one fix-all").diagnostics.len(),
            2,
            "the result annotation is necessary"
        );
        assert_eq!(
            apply(&vfs, &uri, all.first().expect("one fix-all")),
            "fn pick(a, b) -> int = if a < b { a } else { b }\n"
        );
        assert!(actions(&vfs, &uri, (0, 0, 0, 0), &[FIX_ALL.to_string()]).is_empty());
    }

    #[test]
    fn current_buffer_recheck_rejects_stale_or_unparseable_fixes() {
        let (vfs, uri) = document(SOURCE, "ospml");
        assert_eq!(
            actions(&vfs, &uri, (0, 0, 0, 0), &["quickfix".to_string()]).len(),
            1
        );
        for source in [
            "decorate : string -> string\ndecorate text = text\n",
            "decorate : string -> string\ndecorate text = missing\n",
            "decorate : ->\n",
        ] {
            vfs.open(uri.clone(), source, DocumentVersion::new(8));
            assert!(
                actions(&vfs, &uri, (0, 0, 0, 0), &[]).is_empty(),
                "{source}"
            );
        }
        vfs.close(&uri);
        assert!(actions(&vfs, &uri, (0, 0, 0, 0), &[]).is_empty());
    }

    #[test]
    fn byte_ranges_are_encoded_without_splitting_unicode() {
        let source = "// 😀\r\nfn callbackOf(_label, callback) = callback\r\nlet f = callbackOf(\"😀é\", fn(name: string) -> string => name + \"!\")\r\n";
        let utf16 = crate::diagnostics::analyze(source, "test.osp", PositionEncoding::Utf16);
        let utf8 = crate::diagnostics::analyze(source, "test.osp", PositionEncoding::Utf8);
        assert_eq!(utf16.fixes.len(), 2, "{utf16:?}");
        assert_eq!(utf8.fixes.len(), 2);
        assert_eq!(
            utf16.fixes.first().expect("UTF16 fix").diagnostic.range,
            (2, 33, 2, 41)
        );
        assert_eq!(
            utf8.fixes.first().expect("UTF8 fix").diagnostic.range,
            (2, 36, 2, 44)
        );
        assert_eq!(
            crate::warning_actions::byte_span("😀", &(1..4), PositionEncoding::Utf16),
            None
        );
        assert_eq!(
            crate::warning_actions::byte_span("😀", &(0..4), PositionEncoding::Utf16),
            Some((0, 0, 0, 2))
        );
        assert_eq!(
            crate::warning_actions::byte_span(
                "abc",
                &std::ops::Range { start: 2, end: 1 },
                PositionEncoding::Utf16
            ),
            None
        );
    }
}
