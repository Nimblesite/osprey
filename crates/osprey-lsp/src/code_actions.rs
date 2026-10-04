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
    cache: &crate::project_cache::ProjectCache,
) -> Vec<CodeAction> {
    let (Some(version), Some(source)) = (vfs.version(uri), vfs.text(uri)) else {
        return Vec::new();
    };
    let analysis =
        crate::diagnostics::analyze_cached(&source, uri.as_str(), vfs.encoding(), Some(vfs), cache);
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
mod tests {
    fn actions(vfs: &Vfs, uri: &DocumentUri, range: Span, only: &[String]) -> Vec<CodeAction> {
        super::actions(
            vfs,
            uri,
            range,
            only,
            &crate::project_cache::ProjectCache::default(),
        )
    }
    use super::*;

    // [LSP-CODE-ACTIONS-MODULES]: edits must repair the actual compiler contract.
    #[test]
    fn module_paths_and_quoted_import_aliases_have_proven_fixes() -> Result<(), String> {
        for (extension, source, expected) in [
            (
                "osp",
                "module Store { export fn read() = 42 }\nlet answer = Store.read()\n",
                "Store::read",
            ),
            (
                "ospml",
                "module Store\n    export read () = 42\nanswer = Store.read ()\n",
                "Store::read",
            ),
            (
                "osp",
                "namespace \"billing/api\" { fn read() = 42 }\nimport \"billing/api\"\n",
                " as BillingApi",
            ),
            (
                "ospml",
                "namespace \"billing/api\"\n    read () = 42\nimport \"billing/api\"\n",
                " as BillingApi",
            ),
        ] {
            assert_module_fix(extension, source, expected)?;
        }
        Ok(())
    }

    fn assert_module_fix(extension: &str, source: &str, expected: &str) -> Result<(), String> {
        let (vfs, uri) = document(source, extension);
        let result = actions(&vfs, &uri, (0, 0, 99, 0), &["quickfix".into()]);
        assert_eq!(result.len(), 1, "{extension}: {result:?}");
        let fix = result.first().ok_or("missing action")?;
        assert_eq!(fix.version, 7);
        assert_eq!(fix.kind, "quickfix");
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(
            fix.edits.first().map(|edit| edit.new_text.as_str()),
            Some(expected)
        );
        assert!(!fix.diagnostics.is_empty());
        assert!(actions(&vfs, &uri, (0, 0, 99, 0), &[FIX_ALL.into()]).is_empty());
        let repaired = apply(&vfs, &uri, fix);
        assert!(repaired.contains(expected), "{repaired}");
        assert!(actions(&vfs, &uri, (0, 0, 99, 0), &["quickfix".into()]).is_empty());
        assert!(
            crate::diagnostics::compute(&repaired, uri.as_str(), PositionEncoding::Utf16)
                .iter()
                .all(|finding| finding.severity != lspkit_server::Severity::Error)
        );
        Ok(())
    }

    #[test]
    fn module_repairs_respect_privacy_alias_collisions_and_remaining_errors() {
        for source in [
            "module Store { fn read() = 42 }\nlet answer = Store.read()\n",
            "module Store { export fn read() = 42 }\nlet answer = Store.read()\nlet bad = missing\n",
            "let store = { read: 42 }\nlet answer = store.read\n",
            "// Store.read\nlet bad = missing\n",
            "let label = \"Store.read\"\nlet bad = missing\n",
            "namespace \"billing/api\" { fn read() = 42 }\nnamespace lib { fn read() = 1 }\nimport lib as BillingApi\nimport \"billing/api\"\n",
            "import \"missing/library\"\n",
            "module Store { export fn read() = 42 }\nlet answer = Store.read(\n",
        ] {
            let (vfs, uri) = document(source, "osp");
            assert!(actions(&vfs, &uri, (0, 0, 99, 0), &["quickfix".into()]).is_empty(), "{source}");
        }
    }

    #[test]
    fn module_repairs_select_imports_at_the_keyword_and_preserve_comments() -> Result<(), String> {
        let source = "namespace \"billing/api\" { fn read() = 42 }\r\nimport \"billing/api\" // preserve this\r\n";
        let (vfs, uri) = document(source, "osp");
        assert!(actions(&vfs, &uri, (0, 0, 0, 1), &["quickfix".into()]).is_empty());
        let result = actions(&vfs, &uri, (1, 0, 1, 0), &["quickfix".into()]);
        assert_eq!(result.len(), 1);
        let fixed = apply(&vfs, &uri, result.first().ok_or("missing alias fix")?);
        assert_eq!(
            fixed,
            source.replace(
                "import \"billing/api\"",
                "import \"billing/api\" as BillingApi"
            )
        );
        Ok(())
    }

    #[test]
    fn quoted_namespace_aliases_handle_digits_unicode_and_escaped_quotes() -> Result<(), String> {
        for (label, alias) in [
            ("123/foo", "Imported123Foo"),
            ("☃", "Imported"),
            ("a\\\"b", "AB"),
        ] {
            let source =
                format!("namespace \"{label}\" {{ fn read() = 42 }}\nimport \"{label}\"\n");
            assert_module_fix("osp", &source, &format!(" as {alias}"))?;
            let ml = format!("namespace \"{label}\"\n    read () = 42\nimport \"{label}\"\n");
            assert_module_fix("ospml", &ml, &format!(" as {alias}"))?;
        }
        Ok(())
    }

    #[test]
    fn module_repairs_preserve_unicode_and_newlines() -> Result<(), String> {
        let source = "module Store { export fn read() = 42 }\r\nfn pair(_label, value) = value\r\nlet answer = pair(\"😀\", Store.read())\r\n";
        for encoding in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            let vfs = Vfs::new(encoding);
            let uri = DocumentUri::new("file:///module-unicode.osp");
            vfs.open(uri.clone(), source, DocumentVersion::new(7));
            let result = actions(&vfs, &uri, (2, 0, 2, 99), &["quickfix".into()]);
            assert_eq!(result.len(), 1);
            assert_eq!(
                apply(&vfs, &uri, result.first().ok_or("missing path fix")?),
                source.replace("Store.read", "Store::read")
            );
        }
        Ok(())
    }

    #[test]
    fn module_repairs_follow_unsaved_siblings_in_both_flavors(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (extension, main, library) in [
            (
                "osp",
                "fn main() = Store.read()\n",
                "module Store { export fn read() = 42 }\n",
            ),
            (
                "ospml",
                "main () = Store.read ()\n",
                "module Store\n    export read () = 42\n",
            ),
        ] {
            assert_live_module_repairs(extension, main, library)?;
        }
        Ok(())
    }

    fn assert_live_module_repairs(
        extension: &str,
        main: &str,
        library: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let fixture = crate::test_support::ProjectFixture::new(extension)?;
        let uri = DocumentUri::new(fixture.write("main", main)?);
        let sibling = DocumentUri::new(fixture.write("library", &library.replace("export ", ""))?);
        let vfs = Vfs::new(PositionEncoding::Utf16);
        vfs.open(uri.clone(), main, DocumentVersion::new(7));
        let cache = crate::project_cache::ProjectCache::default();
        let repairs = || super::actions(&vfs, &uri, (0, 0, 0, 99), &["quickfix".into()], &cache);
        assert!(repairs().is_empty(), "private disk implementation");
        vfs.open(sibling.clone(), library, DocumentVersion::new(1));
        assert_eq!(repairs().len(), 1, "public live implementation");
        vfs.open(sibling.clone(), "(", DocumentVersion::new(2));
        assert!(repairs().is_empty(), "incomplete sibling");
        vfs.close(&sibling);
        assert!(
            repairs().is_empty(),
            "closing restores private disk implementation"
        );
        std::fs::write(
            fixture.root.join("osprey.toml"),
            "[project]\nsource_roots = broken\n",
        )?;
        assert!(repairs().is_empty(), "invalid manifest");
        Ok(())
    }
    use lspkit_vfs::{DocumentVersion, Position, PositionEncoding, Range, TextEdit};

    const SOURCE: &str = "decorate : string -> string\ndecorate text = text + \"!\"\n";

    // [TYPE-ANNOTATION-REDUNDANT], [LSP-CODE-ACTIONS-ANNOTATIONS]: module
    // contract copies cannot justify erasing source that was never written.
    #[test]
    fn manifest_alias_contracts_have_no_warning_or_deletion_action() {
        for (extension, source) in [
            ("osp", "signature Api { type Number = int\n fn next(value: Number) -> Number }\nmodule M : Api { type Number = int\n fn next(value) = wrapAdd(value, 1) }\n"),
            ("ospml", "signature Api\n    type Number = int\n    next : Number -> Number\nmodule M : Api\n    type Number = int\n    next value = wrapAdd value 1\n"),
        ] {
            let (vfs, uri) = document(source, extension);
            let diagnostics = crate::diagnostics::compute(source, uri.as_str(), vfs.encoding());
            assert!(diagnostics.is_empty(), "{extension}: {diagnostics:?}");
            assert!(actions(&vfs, &uri, (0, 0, 99, 0), &[]).is_empty());
        }
    }

    #[test]
    fn written_alias_annotations_remain_independently_removable() -> Result<(), String> {
        for (extension, source, expected) in [
            ("osp", "namespace app;\ntype Numbers = List<int>\nfn first() -> Numbers = [1]\nfn second() -> Numbers = [2]\n", "namespace app;\ntype Numbers = List<int>\nfn first() = [1]\nfn second() = [2]\n"),
            ("ospml", "namespace app\ntype Numbers = (List int)\nfirst : Unit -> Numbers\nfirst () = [1]\nsecond : Unit -> Numbers\nsecond () = [2]\n", "namespace app\ntype Numbers = (List int)\nfirst () = [1]\nsecond () = [2]\n"),
        ] {
            let (vfs, uri) = document(source, extension);
            let fixes = actions(&vfs, &uri, (0, 0, 99, 0), &[FIX_ALL.into()]);
            assert_eq!(fixes.len(), 1, "{extension}: {fixes:?}");
            let fix = fixes.first().ok_or("missing alias annotation fix")?;
            assert_eq!(fix.diagnostics.len(), 2, "{fix:?}");
            assert_eq!(apply(&vfs, &uri, fix), expected);
            let diagnostics = crate::diagnostics::compute(expected, uri.as_str(), vfs.encoding());
            assert!(diagnostics.is_empty(), "{extension}: {diagnostics:?}");
        }
        Ok(())
    }

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
