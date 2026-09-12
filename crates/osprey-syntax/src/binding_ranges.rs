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
