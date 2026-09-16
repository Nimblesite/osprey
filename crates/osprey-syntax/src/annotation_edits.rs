//! Exact source edits for written annotations. Syntax only identifies spelling;
//! the type checker's [TYPE-ANNOTATION-REDUNDANT] oracle authorizes erasure.
use crate::Flavor;
use osprey_ast::Position;
use std::ops::Range;

/// The surface slot holding an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationTarget {
    /// A standalone ML signature, including a value binding's header.
    Signature,
    /// A function or lambda parameter.
    Parameter {
        /// Source spelling of the parameter.
        name: String,
        /// Zero-based slot in the canonical parameter list.
        index: usize,
    },
    /// An inline function result.
    Return,
    /// An inline value binding type.
    Binding,
}

/// An exact byte edit; offsets always address the original source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEdit {
    /// The bytes to replace.
    pub range: Range<usize>,
    /// Replacement text; empty for annotation deletion.
    pub new_text: String,
}

/// Source identity, precise highlight and erasure of one written annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationEdit {
    /// The original annotation position.
    pub position: Position,
    /// The declaration position used by the canonical AST.
    pub owner_position: Position,
    /// Its surface slot, independent of diagnostic message wording.
    pub target: AnnotationTarget,
    /// Exact annotation spelling, excluding surrounding whitespace/newlines.
    pub highlight: Range<usize>,
    /// Nonoverlapping edits against the original source.
    pub edits: Vec<SourceEdit>,
}

/// Identify syntactically removable annotations; callers must prove redundancy.
/// Malformed input produces no edits, because recovery trees are not authority
/// to delete text. Comments and declaration bodies are preserved byte-for-byte.
#[must_use]
pub fn annotation_edits(source: &str, flavor: Flavor) -> Vec<AnnotationEdit> {
    if !crate::parse_program_with_flavor(source, flavor)
        .errors
        .is_empty()
    {
        return Vec::new();
    }
    let mut edits = match flavor {
        Flavor::Default => default_edits(source),
        Flavor::Ml => crate::ml::annotation_edits(source),
    };
    edits.extend(crate::fragment_ranges::collect(
        source,
        flavor,
        annotation_edits,
        crate::fragment_ranges::annotation,
    ));
    edits.sort_by_key(|edit| edit.highlight.start);
    edits
}

/// Coalesce tokens only across whitespace; comment bytes never enter an edit.
pub(crate) fn token_edits(
    source: &str,
    tokens: &[Range<usize>],
    whole_line: bool,
) -> Vec<SourceEdit> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for token in tokens.iter().filter(|r| !r.is_empty()) {
        if let Some(last) = ranges
            .last_mut()
            .filter(|last| source[last.end..token.start].trim().is_empty())
        {
            last.end = token.end;
        } else {
            ranges.push(token.clone());
        }
    }
    ranges
        .into_iter()
        .map(|range| SourceEdit {
            range: trim_erasure(source, range, whole_line),
            new_text: String::new(),
        })
        .collect()
}

fn trim_erasure(source: &str, range: Range<usize>, whole_line: bool) -> Range<usize> {
    let line_start = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |i| range.end + i + 1);
    if whole_line
        && source[line_start..range.start].trim().is_empty()
        && source[range.end..line_end].trim().is_empty()
    {
        return line_start..line_end;
    }
    let start = if whole_line {
        range.start
    } else {
        source[..range.start].trim_end_matches([' ', '\t']).len()
    };
    let end = if whole_line {
        range.end + source[range.end..].len()
            - source[range.end..].trim_start_matches([' ', '\t']).len()
    } else {
        range.end
    };
    start..end
}

fn point(node: tree_sitter::Node<'_>) -> Position {
    crate::default::position_from_point(node.start_position())
}

fn default_edits(source: &str) -> Vec<AnnotationEdit> {
    let Some(tree) = crate::parse_tree(source) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        default_annotations(source, node, &mut result);
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        pending.extend(children.into_iter().rev());
    }
    result.sort_by_key(|edit| edit.highlight.start);
    result
}

fn default_annotations(source: &str, node: tree_sitter::Node<'_>, out: &mut Vec<AnnotationEdit>) {
    let (owner, target, ty) = match node.kind() {
        "parameter" => {
            let Some(list) = node.parent() else {
                return;
            };
            let Some(owner) = list.parent() else {
                return;
            };
            if !matches!(owner.kind(), "function_declaration" | "lambda_expression") {
                return;
            }
            let mut cursor = list.walk();
            let index = list
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "parameter")
                .position(|n| n == node)
                .unwrap_or_default();
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .unwrap_or_default()
                .to_owned();
            (
                owner,
                AnnotationTarget::Parameter { name, index },
                node.child_by_field_name("type"),
            )
        }
        "let_declaration" => (
            node,
            AnnotationTarget::Binding,
            node.child_by_field_name("type"),
        ),
        "function_declaration" => (
            node,
            AnnotationTarget::Return,
            node.child_by_field_name("return_type"),
        ),
        "lambda_expression" => {
            let mut cursor = node.walk();
            let ty = node
                .children(&mut cursor)
                .find(|n| crate::default::Lowerer::is_type_kind(n.kind()));
            (node, AnnotationTarget::Return, ty)
        }
        _ => return,
    };
    if let Some(ty) = ty {
        add_default(source, owner, target, ty, out);
    }
}

fn add_default(
    source: &str,
    owner: tree_sitter::Node<'_>,
    target: AnnotationTarget,
    ty: tree_sitter::Node<'_>,
    out: &mut Vec<AnnotationEdit>,
) {
    let Some(marker) = std::iter::successors(ty.prev_sibling(), tree_sitter::Node::prev_sibling)
        .find(|n| matches!(n.kind(), ":" | "->"))
    else {
        return;
    };
    let highlight = marker.start_byte()..ty.end_byte();
    let mut tokens = vec![marker.byte_range()];
    terminal_ranges(ty, &mut tokens);
    let owner_node = match owner.kind() {
        "function_declaration" => owner.child_by_field_name("name"),
        "let_declaration" => owner.child_by_field_name("keyword"),
        _ => Some(owner),
    }
    .unwrap_or(owner);
    out.push(AnnotationEdit {
        position: point(ty),
        owner_position: point(owner_node),
        target,
        highlight,
        edits: token_edits(source, &tokens, false),
    });
}

fn terminal_ranges(node: tree_sitter::Node<'_>, out: &mut Vec<Range<usize>>) {
    if node.kind().contains("comment") {
        return;
    }
    if node.child_count() == 0 {
        out.push(node.byte_range());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        terminal_ranges(child, out);
    }
}
