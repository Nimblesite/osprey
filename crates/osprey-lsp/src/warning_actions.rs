//! Source warnings and their proven edits share one analysis result.
//! Implements [LSP-CODE-ACTIONS-ANNOTATIONS].

use lspkit_server::Diagnostic;
use lspkit_vfs::PositionEncoding;
use osprey_ast::{Position, Program};
use osprey_syntax::{AnnotationEdit, AnnotationTarget};
use osprey_types::{RedundantAnnotation, RedundantTarget};

use crate::model::{Span, TextChange};

#[derive(Debug, Default)]
pub(crate) struct Analysis {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) fixes: Vec<Fix>,
}

#[derive(Debug)]
pub(crate) struct Fix {
    pub(crate) diagnostic: Diagnostic,
    pub(crate) edits: Vec<TextChange>,
    pub(crate) signature: bool,
}

impl From<Vec<Diagnostic>> for Analysis {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            diagnostics,
            fixes: Vec::new(),
        }
    }
}

impl Analysis {
    pub(crate) fn add_warnings(
        &mut self,
        source: &str,
        program: &Program,
        flavor: osprey_syntax::Flavor,
        encoding: PositionEncoding,
        locate: &impl Fn(Position) -> Option<Position>,
    ) {
        let edits = osprey_syntax::annotation_edits(source, flavor);
        for mut site in osprey_types::redundant_annotation_sites_where(program, |position| {
            position.and_then(locate).is_some()
        }) {
            site.warning.position = site.warning.position.and_then(locate);
            site.annotation_position = site.annotation_position.and_then(locate);
            self.add_annotation(source, &site, &edits, encoding);
        }
        self.add_unused(source, program, flavor, encoding, locate);
    }

    fn add_unused(
        &mut self,
        source: &str,
        program: &Program,
        flavor: osprey_syntax::Flavor,
        encoding: PositionEncoding,
        locate: &impl Fn(Position) -> Option<Position>,
    ) {
        let ranges = osprey_syntax::binding_ranges(source, flavor);
        for symbol in osprey_types::unused_symbols(program) {
            let Some(position) = symbol.owner_position.and_then(locate) else {
                continue;
            };
            let mut diagnostic = crate::diagnostics::warning(
                source,
                position,
                &symbol.warning.message,
                symbol.warning.rule,
                encoding,
            );
            if let Some(range) = ranges.iter().find(|range| {
                range.owner_position == Some(position)
                    && range.name == symbol.name
                    && range.occurrence == symbol.occurrence
                    && binding_kind_matches(range.kind, symbol.kind)
            }) {
                if let Some(span) = byte_span(source, &range.range, encoding) {
                    diagnostic.range = span;
                }
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn add_annotation(
        &mut self,
        source: &str,
        site: &RedundantAnnotation,
        candidates: &[AnnotationEdit],
        encoding: PositionEncoding,
    ) {
        let candidate = candidates
            .iter()
            .find(|candidate| matches_site(site, candidate));
        let range = candidate.and_then(|edit| byte_span(source, &edit.highlight, encoding));
        let mut diagnostic = crate::diagnostics::warning(
            source,
            site.warning.position.unwrap_or_default(),
            &site.warning.message,
            site.warning.rule,
            encoding,
        );
        diagnostic.range = range.unwrap_or(diagnostic.range);
        if let Some(candidate) = candidate {
            if let Some(edits) = changes(source, candidate, encoding) {
                self.fixes.push(Fix {
                    diagnostic: diagnostic.clone(),
                    edits,
                    signature: candidate.target == AnnotationTarget::Signature,
                });
            }
        }
        self.diagnostics.push(diagnostic);
    }
}

fn matches_site(site: &RedundantAnnotation, candidate: &AnnotationEdit) -> bool {
    match site.annotation_position {
        Some(position) => position == candidate.position,
        None => {
            site.warning.position == Some(candidate.owner_position)
                && targets_match(&site.target, &candidate.target)
        }
    }
}

fn targets_match(left: &RedundantTarget, right: &AnnotationTarget) -> bool {
    match (left, right) {
        (RedundantTarget::Signature, AnnotationTarget::Signature)
        | (RedundantTarget::Return, AnnotationTarget::Return)
        | (RedundantTarget::Binding, AnnotationTarget::Binding) => true,
        (
            RedundantTarget::Parameter { name, index },
            AnnotationTarget::Parameter {
                name: other,
                index: slot,
            },
        ) => name == other && index == slot,
        _ => false,
    }
}

fn changes(
    source: &str,
    edit: &AnnotationEdit,
    encoding: PositionEncoding,
) -> Option<Vec<TextChange>> {
    edit.edits
        .iter()
        .map(|change| {
            Some(TextChange {
                range: byte_span(source, &change.range, encoding)?,
                new_text: change.new_text.clone(),
            })
        })
        .collect()
}

pub(crate) fn byte_span(
    source: &str,
    range: &std::ops::Range<usize>,
    encoding: PositionEncoding,
) -> Option<Span> {
    let start = byte_position(source, range.start, encoding)?;
    let end = byte_position(source, range.end, encoding)?;
    (range.start <= range.end).then_some((start.0, start.1, end.0, end.1))
}

fn byte_position(source: &str, offset: usize, encoding: PositionEncoding) -> Option<(u32, u32)> {
    let prefix = source.get(..offset)?;
    let line = u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count()).ok()?;
    Some((
        line,
        crate::text::measure(prefix.rsplit('\n').next()?, encoding),
    ))
}

fn binding_kind_matches(left: osprey_syntax::BindingKind, right: osprey_types::UnusedKind) -> bool {
    use osprey_syntax::BindingKind as Source;
    use osprey_types::UnusedKind as Typed;
    matches!(
        (left, right),
        (Source::Variable, Typed::Variable)
            | (Source::Parameter, Typed::Parameter)
            | (Source::PatternBinding, Typed::PatternBinding)
            | (Source::HandlerParameter, Typed::HandlerParameter)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_findings_have_exact_binder_ranges_and_never_deletion_actions() {
        let source = "type Pair = { left: int, right: int }\nfn pick(used, ignored) = {\n let unused = 7\n let pair = Pair { left: used, right: 2 }\n match pair { { left, right } => left }\n}\n";
        let analysis = crate::diagnostics::analyze(source, "unused.osp", PositionEncoding::Utf16);
        let findings: Vec<_> = analysis
            .diagnostics
            .iter()
            .map(|d| (d.code.as_deref(), d.message.as_str(), d.range))
            .collect();
        assert_eq!(
            findings,
            [
                (
                    Some("unused-parameter"),
                    "unused parameter `ignored`",
                    (1, 14, 1, 21)
                ),
                (
                    Some("unused-variable"),
                    "unused variable `unused`",
                    (2, 5, 2, 11)
                ),
                (
                    Some("unused-pattern-binding"),
                    "unused pattern binding `right`",
                    (4, 22, 4, 27)
                ),
            ]
        );
        assert!(analysis.fixes.is_empty());
        let wire = crate::wire::publish_diagnostics("file:///unused.osp", &analysis.diagnostics);
        for diagnostic in wire
            .get("diagnostics")
            .and_then(serde_json::Value::as_array)
            .expect("diagnostics")
        {
            assert_eq!(diagnostic["severity"], 2);
            assert_eq!(diagnostic["source"], "osprey");
            assert_eq!(diagnostic["tags"], serde_json::json!([1]));
        }
    }

    #[test]
    fn used_names_and_handler_captures_do_not_mask_unused_operation_parameters() {
        let source = "effect Pick { choose: fn(int, int) -> int }\nfn run(seed) = handle Pick\n choose first second => resume(first)\nin await (spawn (perform Pick.choose(seed, 2)))\nlet result = run(7)\n";
        let analysis = crate::diagnostics::analyze(source, "unused.osp", PositionEncoding::Utf16);
        assert_eq!(analysis.diagnostics.len(), 1, "{analysis:?}");
        let diagnostic = analysis.diagnostics.first().expect("one warning");
        assert_eq!(diagnostic.code.as_deref(), Some("unused-handler-parameter"));
        assert_eq!(
            diagnostic.message,
            "unused handler parameter `second` of `Pick.choose`"
        );
        assert_eq!(diagnostic.range, (2, 14, 2, 20));
        assert!(analysis.fixes.is_empty());
    }

    #[test]
    fn escaped_paths_and_current_sibling_buffers_control_the_safe_set() {
        let root = std::env::temp_dir().join(format!("osprey warnings {}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("project directory");
        std::fs::write(root.join("osprey.toml"), "[project]\nname = \"warnings\"\nsource_roots = [\"src\"]\ndefault_namespace = \"review\"\nentry = \"src/main.ospml\"\n").expect("manifest");
        let main = "import review::Helper\ndecorate : string -> string\ndecorate text = Helper::suffix text\nprint (decorate \"x\")\n";
        let helper = "module Helper\n    export suffix text = text + \"!\"\n";
        std::fs::write(root.join("src/main.ospml"), main).expect("entry source");
        std::fs::write(root.join("src/helper.ospml"), helper).expect("helper source");
        let uri = lspkit_server::uri::path_to_uri(&root.join("src/main.ospml")).expect("entry URI");
        let sibling = lspkit_vfs::DocumentUri::new(
            lspkit_server::uri::path_to_uri(&root.join("src/helper.ospml")).expect("sibling URI"),
        );
        let vfs = lspkit_vfs::Vfs::new(PositionEncoding::Utf16);
        let analyze = || crate::diagnostics::analyze_live(main, &uri, vfs.encoding(), Some(&vfs));
        assert_eq!(analyze().fixes.len(), 1, "{:?}", analyze());
        vfs.open(
            sibling.clone(),
            "module Helper\n    export suffix text = text\n",
            lspkit_vfs::DocumentVersion::new(1),
        );
        assert!(
            analyze().fixes.is_empty(),
            "the signature becomes necessary with the live generic helper"
        );
        vfs.open(
            sibling.clone(),
            "module Helper\n    export suffix text = unknown\n",
            lspkit_vfs::DocumentVersion::new(2),
        );
        assert!(
            analyze().fixes.is_empty(),
            "invalid live sibling forbids speculative erasure"
        );
        vfs.open(
            sibling.clone(),
            "module Helper\n    export suffix text = (\n",
            lspkit_vfs::DocumentVersion::new(3),
        );
        assert!(
            analyze().fixes.is_empty(),
            "unparseable live sibling forbids speculative erasure"
        );
        vfs.close(&sibling);
        assert_eq!(
            analyze().fixes.len(),
            1,
            "closing the buffer returns to the valid saved helper"
        );
        std::fs::remove_dir_all(&root).expect("remove own project fixture");
    }

    #[test]
    fn top_level_pattern_warnings_retain_exact_ranges_in_both_flavors() {
        for (path, source, ranges) in [
            (
                "top.osp",
                "match 1 { unused => print(\"a\") }\nmatch 2 { unused => print(\"b\") }\n",
                vec![(0, 10, 0, 16), (1, 10, 1, 16)],
            ),
            (
                "top.ospml",
                "match 1\n    unused => print \"a\"\nmatch 2\n    unused => print \"b\"\n",
                vec![(1, 4, 1, 10), (3, 4, 3, 10)],
            ),
            (
                "fragment.osp",
                "print(\"🦅\\n${match 1 { unused => 2 }}\")\n",
                vec![(0, 23, 0, 29)],
            ),
            (
                "fragment.ospml",
                "print \"🦅\\n${match 1\\n    unused => 2}\"\n",
                vec![(0, 26, 0, 32)],
            ),
        ] {
            let analysis = crate::diagnostics::analyze(source, path, PositionEncoding::Utf16);
            assert_eq!(
                analysis.diagnostics.len(),
                ranges.len(),
                "{path}\n{analysis:?}"
            );
            assert!(analysis.fixes.is_empty());
            for (diagnostic, range) in analysis.diagnostics.iter().zip(ranges) {
                assert_eq!(diagnostic.code.as_deref(), Some("unused-pattern-binding"));
                assert_eq!(diagnostic.message, "unused pattern binding `unused`");
                assert_eq!(diagnostic.range, range, "{path}");
                assert_eq!(diagnostic.source.as_deref(), Some("osprey"));
                assert_eq!(diagnostic.severity, lspkit_server::Severity::Warning);
            }
        }
    }
}
