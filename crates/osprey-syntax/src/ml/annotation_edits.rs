//! Source annotations recorded at the parser's actual type boundaries.
use super::token::{TokKind, Token};
use crate::annotation_edits::token_edits;
use crate::{AnnotationEdit, AnnotationTarget, SourceEdit};

pub(super) fn parameter(
    source: &str,
    tokens: &[Token],
    start: usize,
    end: usize,
    name: &str,
    out: &mut Vec<AnnotationEdit>,
) {
    let Some(first) = tokens.get(start) else {
        return;
    };
    let Some(ty) = tokens.get(start + 1) else {
        return;
    };
    let ranges: Vec<_> = tokens
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(|t| t.range.clone())
        .filter(|r| !r.is_empty())
        .collect();
    let Some(last) = ranges.last() else {
        return;
    };
    out.push(AnnotationEdit {
        position: ty.pos,
        owner_position: start
            .checked_sub(1)
            .and_then(|index| tokens.get(index))
            .map_or(ty.pos, |token| token.pos),
        target: AnnotationTarget::Parameter {
            name: name.to_owned(),
            index: 0,
        },
        highlight: first.range.start..last.end,
        edits: token_edits(source, &ranges, false),
    });
}

pub(super) fn signature(
    source: &str,
    tokens: &[Token],
    start: usize,
    end: usize,
    out: &mut Vec<AnnotationEdit>,
) {
    let Some(first) = tokens.get(start) else {
        return;
    };
    let ranges: Vec<_> = tokens
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(|t| t.range.clone())
        .filter(|r| !r.is_empty())
        .collect();
    let Some(last) = ranges.last() else {
        return;
    };
    let highlight = first.range.start..last.end;
    let exported = start
        .checked_sub(1)
        .and_then(|i| tokens.get(i))
        .filter(|t| t.kind == TokKind::KwExport);
    let mut removals = exported
        .into_iter()
        .map(|t| t.range.clone())
        .collect::<Vec<_>>();
    removals.extend(ranges);
    let mut edits = token_edits(source, &removals, true);
    if exported.is_some() {
        let Some(binding) = next_binding(tokens, end, &first.kind) else {
            return;
        };
        edits.push(SourceEdit {
            range: binding.range.start..binding.range.start,
            new_text: "export ".to_owned(),
        });
    }
    out.push(AnnotationEdit {
        position: first.pos,
        owner_position: first.pos,
        target: AnnotationTarget::Signature,
        highlight,
        edits,
    });
}

fn next_binding<'a>(tokens: &'a [Token], end: usize, name: &TokKind) -> Option<&'a Token> {
    tokens
        .get(end..)
        .unwrap_or_default()
        .iter()
        .find(|t| !t.range.is_empty() && !matches!(t.kind, TokKind::Doc(_) | TokKind::InnerDoc(_)))
        .filter(|t| &t.kind == name)
}

pub(super) fn resolve_owners(
    source: &str,
    annotations: Vec<AnnotationEdit>,
) -> Vec<AnnotationEdit> {
    if !annotations
        .iter()
        .any(|annotation| matches!(annotation.target, AnnotationTarget::Parameter { .. }))
    {
        return annotations;
    }
    let bindings = super::binding_ranges::collect(source);
    let (tokens, _) = super::lexer::lex(source);
    annotations
        .into_iter()
        .filter_map(|mut annotation| {
            if matches!(annotation.target, AnnotationTarget::Parameter { .. }) {
                let token = tokens.iter().find(|token| {
                    token.pos == annotation.owner_position && !token.range.is_empty()
                })?;
                let binding = bindings.iter().find(|binding| {
                    binding.kind == crate::BindingKind::Parameter && binding.range == token.range
                })?;
                annotation.owner_position = binding.owner_position?;
            }
            Some(annotation)
        })
        .collect()
}
