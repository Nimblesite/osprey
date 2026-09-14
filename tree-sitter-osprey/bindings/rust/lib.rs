//! Rust bindings to the tree-sitter Osprey grammar.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_osprey() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_osprey) };

#[cfg(test)]
mod tests {
    #[test]
    fn can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading Osprey grammar");
        let tree = parser.parse("let x = 1\n", None).unwrap();
        assert!(!tree.root_node().has_error());
    }

    /// Every comment the grammar produces must reach a highlight capture. `//!`
    /// became its own `inner_doc_comment` node, and without a capture it
    /// rendered as plain text in every tree-sitter consumer.
    #[test]
    fn every_comment_kind_has_a_highlight_capture() {
        use streaming_iterator::StreamingIterator;
        let language: tree_sitter::Language = super::LANGUAGE.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).expect("grammar loads");
        let source = "//! The file.\n/// The function.\nfn f() = 1 // why\n";
        let tree = parser.parse(source, None).expect("source parses");
        let query =
            tree_sitter::Query::new(&language, include_str!("../../queries/highlights.scm"))
                .expect("highlight query compiles");
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut captured = Vec::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        while let Some(found) = matches.next() {
            captured.extend(found.captures.iter().map(|capture| capture.node.kind()));
        }
        for kind in ["line_comment", "doc_comment", "inner_doc_comment"] {
            assert!(
                captured.contains(&kind),
                "{kind} has no highlight capture: {captured:?}"
            );
        }
    }
}
