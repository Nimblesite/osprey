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
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("grammar loads");
        let tree = parser.parse(source, None).expect("source parses");
        assert!(
            !tree.root_node().has_error(),
            "{source}: {}",
            tree.root_node().to_sexp()
        );
        tree
    }

    fn descendants<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Vec<tree_sitter::Node<'a>> {
        let mut found = Vec::new();
        if node.kind() == kind {
            found.push(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            found.extend(descendants(child, kind));
        }
        found
    }

    fn binary_expressions<'a>(
        node: tree_sitter::Node<'_>,
        source: &'a str,
        out: &mut Vec<(&'a str, &'a str)>,
    ) {
        out.extend(
            descendants(node, "binary_expression")
                .into_iter()
                .map(|node| {
                    let operator = node
                        .child_by_field_name("operator")
                        .expect("operator field");
                    (&source[node.byte_range()], &source[operator.byte_range()])
                }),
        );
    }

    #[test]
    fn interpolation_exposes_binary_expression_operators_and_byte_ranges() {
        let source = r#"fn describe(value) = "${if value > 0 { value + 1 } else { value - 1 }}""#;
        let tree = parse(source);
        let mut found = Vec::new();
        binary_expressions(tree.root_node(), source, &mut found);
        assert_eq!(
            found,
            [("value > 0", ">"), ("value + 1", "+"), ("value - 1", "-")]
        );
        let unicode = r#"let s = "ππ${1 + 2}""#;
        let tree = parse(unicode);
        let mut found = Vec::new();
        binary_expressions(tree.root_node(), unicode, &mut found);
        assert_eq!(found, [("1 + 2", "+")]);
    }

    #[test]
    fn interpolation_text_escapes_and_openers_keep_raw_ranges() {
        let source = r#"let s = "  before \t\${x + 1}  after $  \"""#;
        let tree = parse(source);
        let root = tree.root_node();
        let holes = descendants(root, "interpolation");
        assert_eq!(holes.len(), 1);
        assert_eq!(&source[holes[0].byte_range()], "${x + 1}");
        let text: String = descendants(root, "string_content")
            .iter()
            .map(|node| &source[node.byte_range()])
            .collect();
        assert_eq!(text, "  before \\  after $  ");
        let escapes: Vec<_> = descendants(root, "escape_sequence")
            .iter()
            .map(|node| &source[node.byte_range()])
            .collect();
        assert_eq!(escapes, [r"\t", r#"\""#]);
    }

    #[test]
    fn interpolation_parses_calls_records_members_and_nested_strings() {
        for (source, holes, kind) in [
            (r#"let s = "${f(1)} / ${point.x}""#, 2, "call_expression"),
            (
                r#"let s = "${(Point { x: 1, y: 2 }).x}""#,
                1,
                "type_constructor",
            ),
            (r#"let s = "${f(\"inner\")}""#, 1, "string"),
            (r#"let s = "${f("inner ${y}")}""#, 2, "interpolated_string"),
            (r#"let s = "${1\t+ 2}""#, 1, "binary_expression"),
        ] {
            let tree = parse(source);
            assert_eq!(
                descendants(tree.root_node(), "interpolation").len(),
                holes,
                "{source}"
            );
            assert!(
                !descendants(tree.root_node(), kind).is_empty(),
                "{source}: {kind}"
            );
        }
    }

    #[test]
    fn interpolation_rejects_invalid_expressions() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("grammar loads");
        for source in [
            r#"let s = "${1 + }""#,
            r#"let s = "${}""#,
            r#"let s = "${f(}""#,
        ] {
            let tree = parser.parse(source, None).expect("recovery tree");
            assert!(
                tree.root_node().has_error(),
                "accepted invalid interpolation: {source}"
            );
        }
    }

    #[test]
    fn interpolation_changes_operators_during_incremental_reparse() {
        let source = r#"let s = "${f(\"inner\")} ${1 + 2}""#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("grammar loads");
        let mut tree = parser.parse(source, None).expect("source parses");
        let start = source.find('+').expect("operator");
        let point = tree_sitter::Point::new(0, start);
        let end = tree_sitter::Point::new(0, start + 1);
        tree.edit(&tree_sitter::InputEdit {
            start_byte: start,
            old_end_byte: start + 1,
            new_end_byte: start + 1,
            start_position: point,
            old_end_position: end,
            new_end_position: end,
        });
        let changed = source.replacen('+', "-", 1);
        let updated = parser.parse(&changed, Some(&tree)).expect("reparse");
        assert!(!updated.root_node().has_error());
        let mut found = Vec::new();
        binary_expressions(updated.root_node(), &changed, &mut found);
        assert_eq!(found, [("1 - 2", "-")]);
        assert_eq!(
            updated.root_node().to_sexp(),
            parse(&changed).root_node().to_sexp()
        );
    }

    fn highlight_captures(source: &str) -> Vec<(String, String)> {
        use streaming_iterator::StreamingIterator;
        let tree = parse(source);
        let query = tree_sitter::Query::new(&super::LANGUAGE.into(),
            include_str!("../../queries/highlights.scm")).expect("query compiles");
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut captures = Vec::new();
        while let Some(found) = matches.next() {
            captures.extend(found.captures.iter().map(|capture| (
                query.capture_names()[capture.index as usize].to_owned(),
                source[capture.node.byte_range()].to_owned())));
        }
        captures
    }

    #[test]
    fn interpolation_highlights_text_separately_from_executable_code() {
        let captures = highlight_captures(r#"let s = "hello ${x + 1} end""#);
        for (name, text) in [("string", "hello "), ("string", " end"), ("operator", "+")] {
            assert!(captures.contains(&(name.into(), text.into())), "{captures:?}");
        }
        assert!(!captures.iter().any(|(name, text)| name == "string" && text.contains("x + 1")));
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
