//! Apply the compiler's exact string fragment source map to editor ranges.
use crate::strings::FragmentMap;
use crate::{AnnotationEdit, BindingRange, Flavor, SourceEdit};
use osprey_ast::Position;
use std::ops::Range;

pub(crate) struct Literal {
    pub range: Range<usize>,
    pub position: Position,
    pub owner_position: Option<Position>,
}

pub(crate) fn collect<T>(
    source: &str,
    flavor: Flavor,
    parse: fn(&str, Flavor) -> Vec<T>,
    map: fn(T, &FragmentMap<'_>, &Literal, usize, Flavor) -> Option<T>,
) -> Vec<T> {
    if !source.contains("${") {
        return Vec::new();
    }
    let literals = match flavor {
        Flavor::Default => crate::default::string_literals(source),
        Flavor::Ml => crate::ml::string_literals(source),
    };
    let prefix = crate::strings::fragment_binding(flavor);
    let mut result = Vec::new();
    for literal in literals {
        let Some(raw) = source.get(literal.range.clone()) else {
            continue;
        };
        for fragment in crate::strings::fragments(raw, literal.position) {
            let wrapped = format!("{prefix}{}\n", fragment.text);
            result.extend(
                parse(&wrapped, flavor)
                    .into_iter()
                    .filter_map(|item| map(item, &fragment, &literal, prefix.len(), flavor)),
            );
        }
    }
    result
}

fn range(
    inner: Range<usize>,
    map: &FragmentMap<'_>,
    literal: &Literal,
    prefix: usize,
) -> Option<Range<usize>> {
    let relative = map.map_range(inner, prefix)?;
    Some(
        relative.start.checked_add(literal.range.start)?
            ..relative.end.checked_add(literal.range.start)?,
    )
}

pub(crate) fn annotation(
    edit: AnnotationEdit,
    map: &FragmentMap<'_>,
    literal: &Literal,
    prefix: usize,
    flavor: Flavor,
) -> Option<AnnotationEdit> {
    Some(AnnotationEdit {
        position: map.map_position(edit.position, prefix, flavor)?,
        owner_position: map.map_position(edit.owner_position, prefix, flavor)?,
        highlight: range(edit.highlight, map, literal, prefix)?,
        edits: edit
            .edits
            .into_iter()
            .map(|edit| {
                Some(SourceEdit {
                    range: range(edit.range, map, literal, prefix)?,
                    new_text: edit.new_text,
                })
            })
            .collect::<Option<Vec<_>>>()?,
        target: edit.target,
    })
}

pub(crate) fn binding(
    binding: BindingRange,
    map: &FragmentMap<'_>,
    literal: &Literal,
    prefix: usize,
    flavor: Flavor,
) -> Option<BindingRange> {
    Some(BindingRange {
        owner_position: binding
            .owner_position
            .and_then(|p| map.map_position(p, prefix, flavor))
            .or(literal.owner_position),
        range: range(binding.range, map, literal, prefix)?,
        ..binding
    })
}
