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

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&super::LANGUAGE.into()).expect("grammar loads");
        let tree = parser.parse(source, None).expect("source parses");
        assert!(!tree.root_node().has_error(), "{source}: {}", tree.root_node().to_sexp());
        tree
    }

    fn binary_expressions<'a>(node: tree_sitter::Node<'_>, source: &'a str, out: &mut Vec<(&'a str, &'a str)>) {
        if node.kind() == "binary_expression" {
            let operator = node.child_by_field_name("operator").expect("operator field");
            out.push((&source[node.byte_range()], &source[operator.byte_range()]));
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            binary_expressions(child, source, out);
        }
    }

    #[test]
    fn interpolation_exposes_binary_expression_operators_and_byte_ranges() {
        let source = r#"fn describe(value) = "${if value > 0 { value + 1 } else { value - 1 }}""#;
        let tree = parse(source);
        let mut found = Vec::new();
        binary_expressions(tree.root_node(), source, &mut found);
        assert_eq!(found, [("value > 0", ">"), ("value + 1", "+"), ("value - 1", "-")]);
        let unicode = r#"let s = "ππ${1 + 2}""#;
        let tree = parse(unicode);
        let mut found = Vec::new();
        binary_expressions(tree.root_node(), unicode, &mut found);
        assert_eq!(found, [("1 + 2", "+")]);
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
