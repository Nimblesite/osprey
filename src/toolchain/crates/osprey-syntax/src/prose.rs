//! The source text that is prose rather than code: comments, documentation
//! comments and the literal text of strings. A name written there refers to
//! nothing, so a textual symbol search skips it, while a `${...}`
//! interpolation inside a string is code and is kept
//! ([LSP-REFERENCES-CODE-ONLY]).

use std::ops::Range;

use crate::Flavor;

/// The byte ranges of `source` that are prose in `flavor`. A flavor without a
/// prose reader, or ML source that does not lex, yields none, so a search
/// over it stays as wide as before rather than hiding a real use.
#[must_use]
pub fn prose_ranges(source: &str, flavor: Flavor) -> Vec<Range<usize>> {
    match flavor {
        Flavor::Default => crate::default::parse_tree(source)
            .map_or_else(Vec::new, |tree| nodes_of(&tree, &DEFAULT_PROSE)),
        Flavor::Python => crate::python::parse_python_tree(source)
            .map_or_else(|_| Vec::new(), |tree| nodes_of(&tree, &PYTHON_PROSE)),
        Flavor::Ml => crate::ml::prose_ranges(source),
        Flavor::TypeScript => Vec::new(),
    }
}

/// Default node kinds whose text is prose; an interpolation is its own node.
const DEFAULT_PROSE: [&str; 5] = [
    "line_comment",
    "doc_comment",
    "inner_doc_comment",
    "comment",
    "string_content",
];

/// Python node kinds whose text is prose; an f-string's `{...}` is its own node.
const PYTHON_PROSE: [&str; 2] = ["comment", "string_content"];

/// Byte ranges of every node of `tree` whose kind is in `kinds`.
fn nodes_of(tree: &tree_sitter::Tree, kinds: &[&str]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if kinds.contains(&node.kind()) {
            ranges.push(node.start_byte()..node.end_byte());
        } else {
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
    }
    ranges
}

/// The parts of the string literal at `range` outside its `${...}`
/// interpolations.
pub(crate) fn string_text(source: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let (mut start, mut depth, mut at) = (range.start, 0usize, range.start);
    while at < range.end {
        match (bytes.get(at), bytes.get(at + 1)) {
            (Some(b'$'), Some(b'{')) if depth == 0 => {
                parts.push(start..at);
                depth = 1;
                at += 2;
                continue;
            }
            (Some(b'{'), _) if depth > 0 => depth += 1,
            (Some(b'}'), _) if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    start = at + 1;
                }
            }
            _ => {}
        }
        at += 1;
    }
    if depth == 0 {
        parts.push(start..range.end);
    }
    parts.retain(|part| part.start < part.end);
    parts
}
