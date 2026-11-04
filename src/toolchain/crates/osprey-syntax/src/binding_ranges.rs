//! Source identities for lexical binders, preserved through flavor lowering.
use crate::Flavor;
use osprey_ast::Position;
use std::ops::Range;

/// The lexical binding forms diagnosed by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// A local value or mutable cell.
    Variable,
    /// A function or lambda parameter.
    Parameter,
    /// A match/select/clause pattern binder.
    PatternBinding,
    /// An effect handler operation parameter.
    HandlerParameter,
}

/// Exact identifier range associated with a canonical binding identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingRange {
    /// Canonical declaration owner, including generated curry positions.
    pub owner_position: Option<Position>,
    /// The binding's source form.
    pub kind: BindingKind,
    /// Source spelling of the name.
    pub name: String,
    /// Earlier binders with the same owner, kind and name, including used ones.
    pub occurrence: usize,
    /// Exact source bytes of the identifier only.
    pub range: Range<usize>,
}

/// The lexical region containing a binder, in original source bytes.
#[must_use]
pub fn binding_scope(source: &str, flavor: Flavor, binding: &BindingRange) -> Range<usize> {
    if flavor == Flavor::Ml {
        return crate::ml::binding_scope(source, binding);
    }
    let tree = match flavor {
        Flavor::Python => crate::parse_python_tree(source).ok(),
        _ => crate::parse_tree(source),
    };
    tree.and_then(|tree| tree_binding_scope(tree.root_node(), binding))
        .unwrap_or(0..source.len())
}

fn tree_binding_scope(root: tree_sitter::Node<'_>, binding: &BindingRange) -> Option<Range<usize>> {
    let mut node = root.descendant_for_byte_range(binding.range.start, binding.range.end);
    while let Some(current) = node {
        if owns_binding(current.kind(), binding.kind) {
            return Some(current.byte_range());
        }
        node = current.parent();
    }
    None
}

fn owns_binding(node: &str, kind: BindingKind) -> bool {
    match kind {
        BindingKind::Variable => node == "block",
        BindingKind::Parameter => matches!(
            node,
            "function_declaration" | "function_definition" | "lambda_expression" | "lambda"
        ),
        BindingKind::HandlerParameter => node == "handler_arm" || node == "function_definition",
        BindingKind::PatternBinding => matches!(node, "match_arm" | "select_arm" | "case_clause"),
    }
}

/// Locate actual source binders; references and type names are never candidates.
#[must_use]
pub fn binding_ranges(source: &str, flavor: Flavor) -> Vec<BindingRange> {
    if !crate::parse_program_with_flavor(source, flavor)
        .errors
        .is_empty()
    {
        return Vec::new();
    }
    let mut ranges = match flavor {
        Flavor::Default => crate::default::binding_ranges(source),
        Flavor::Ml => crate::ml::binding_ranges(source),
        Flavor::Python => crate::python::binding_ranges(source),
    };
    ranges.extend(crate::fragment_ranges::collect(
        source,
        flavor,
        binding_ranges,
        crate::fragment_ranges::binding,
    ));
    ranges.sort_by_key(|range| range.range.start);
    for index in 0..ranges.len() {
        let (earlier, current) = ranges.split_at_mut(index);
        if let Some(binding) = current.first_mut() {
            binding.occurrence = earlier
                .iter()
                .filter(|other| {
                    other.owner_position == binding.owner_position
                        && other.kind == binding.kind
                        && other.name == binding.name
                })
                .count();
        }
    }
    ranges
}
