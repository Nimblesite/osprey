//! Python layout is formatted from its CST, preserving string/continuation bytes.
//! Implements [FLAVOR-PYTHON-CST], [FLAVOR-PYTHON-VERIFICATION].
use std::collections::BTreeMap;
use tree_sitter::Node;

pub(super) fn format(source: &str) -> String {
    let mut indents = BTreeMap::new();
    if let Ok(tree) = osprey_syntax::parse_python_tree(source) {
        collect(tree.root_node(), source, 0, &mut indents);
    }
    source
        .lines()
        .enumerate()
        .map(|(line, text)| formatted_line(indents.get(&line).copied(), text))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn formatted_line(depth: Option<usize>, text: &str) -> String {
    depth.map_or_else(
        || text.to_owned(),
        |depth| {
            format!(
                "{}{}",
                " ".repeat(depth * super::INDENT_WIDTH),
                text.trim_start()
            )
        },
    )
}

fn collect(node: Node<'_>, source: &str, depth: usize, indents: &mut BTreeMap<usize, usize>) {
    if node.kind() == "string" {
        return;
    }
    let point = node.start_position();
    if is_layout(node.kind())
        && source
            .get(node.start_byte().saturating_sub(point.column)..node.start_byte())
            .is_some_and(|prefix| prefix.trim().is_empty())
    {
        let _ = indents.insert(point.row, depth);
    }
    let next = depth + usize::from(node.kind() == "block");
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, next, indents);
    }
}

fn is_layout(kind: &str) -> bool {
    kind.ends_with("_statement")
        || kind.ends_with("_definition")
        || matches!(
            kind,
            "comment" | "decorator" | "elif_clause" | "else_clause" | "case_clause"
        )
}

#[cfg(test)]
mod tests {
    use crate::{format_source, Flavor};

    #[test]
    fn python_block_indentation_is_meaning_preserving_and_idempotent() -> Result<(), Vec<String>> {
        let source = "def answer(x):\n  offset = 2\n  return x + offset\n";
        let formatted = format_source(source, Flavor::Python)?;
        assert_eq!(
            formatted,
            "def answer(x):\n    offset = 2\n    return x + offset\n"
        );
        assert_eq!(format_source(&formatted, Flavor::Python)?, formatted);
        Ok(())
    }

    #[test]
    fn python_continuations_and_string_contents_survive_formatting() -> Result<(), Vec<String>> {
        let source = "def label():\n  text = \"\"\"first\n  second\n\"\"\"\n  return text\n";
        let formatted = format_source(source, Flavor::Python)?;
        assert!(formatted.contains("\"\"\"first\n  second\n\"\"\""));
        assert!(formatted.contains("    return text"));
        Ok(())
    }
}
