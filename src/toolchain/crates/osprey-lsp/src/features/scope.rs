//! Source binders resolve only inside their lexical regions.
//! Implements [LSP-DEFINITION-LOCAL] and [LSP-REFERENCES-LOCAL].

use lspkit::vfs::PositionEncoding;
use osprey_ast::{Position, Program};
use osprey_syntax::{binding_ranges, binding_scope, BindingKind, BindingRange, Flavor};
use std::ops::Range;

use super::{declaration_occurrence, symbol_matches};
use crate::analysis::{collect_all_symbols, collect_symbols, SymbolInfo, SymbolKind};
use crate::text::{occurrences, prefix_to, Occurrence};

pub(crate) struct Binding {
    /// A pattern binder's or lambda parameter's lexical region start and its
    /// own position: the span holding the match arm or lambda its type is
    /// published under ([LSP-HOVER-BINDERS]). `None` for every other binder.
    pub(crate) introduced: Option<(Position, Position)>,
    /// Every binder of the same name that the same arm introduces: one per
    /// or-pattern alternative, all naming one variable ([PATTERN-OR-BINDINGS]).
    /// Just `at` for any other binder.
    pub(crate) group: Vec<Occurrence>,
    candidates: Vec<Scoped>,
}

struct Scoped {
    at: Occurrence,
    bytes: Range<usize>,
    scope: Range<usize>,
    /// Whether a match arm or lambda introduces it, rather than a `let`.
    introduces: bool,
}

pub(crate) fn binding(
    text: &str,
    program: &Program,
    flavor: Flavor,
    word: &str,
    cursor: (u32, u32),
    enc: PositionEncoding,
) -> Option<Binding> {
    let candidates = candidates(text, program, flavor, word, enc);
    let word = occurrences(text, word, enc)
        .into_iter()
        .find(|found| found.line == cursor.0 && (found.start..=found.end).contains(&cursor.1))?;
    let offset = byte_offset(text, (word.line, word.start), enc)?;
    let resolved = resolve(&candidates, offset)?;
    let introduced = resolved
        .introduces
        .then(|| position(text, resolved.scope.start).zip(position(text, resolved.bytes.start)))
        .flatten();
    let group = alternatives(&candidates, resolved);
    Some(Binding {
        introduced,
        group,
        candidates,
    })
}

fn candidates(
    text: &str,
    program: &Program,
    flavor: Flavor,
    word: &str,
    enc: PositionEncoding,
) -> Vec<Scoped> {
    binding_ranges(text, flavor)
        .into_iter()
        .chain(local_functions(text, program, word, enc))
        .filter(|b| b.name == word)
        .filter_map(|b| {
            let scope = binding_scope(text, flavor, &b);
            if b.kind == BindingKind::Variable && scope == (0..text.len()) {
                return None;
            }
            let at = occurrence(text, b.range.clone(), enc)?;
            Some(Scoped {
                at,
                bytes: b.range,
                scope,
                introduces: matches!(b.kind, BindingKind::PatternBinding | BindingKind::Parameter),
            })
        })
        .collect()
}

fn local_functions(
    text: &str,
    program: &Program,
    word: &str,
    enc: PositionEncoding,
) -> Vec<BindingRange> {
    let top = collect_symbols(program);
    collect_all_symbols(program)
        .into_iter()
        .filter(|s| s.kind == SymbolKind::Function && symbol_matches(s, word))
        .filter(|s| {
            !top.iter()
                .any(|t| t.name == s.name && t.position == s.position)
        })
        .filter_map(|s| {
            let at = declaration_occurrence(text, &s, enc)?;
            let start = byte_offset(text, (at.line, at.start), enc)?;
            Some(BindingRange {
                owner_position: s.position,
                kind: BindingKind::Variable,
                name: word.to_owned(),
                occurrence: 0,
                range: start..start + word.len(),
            })
        })
        .collect()
}

/// The binders `resolved` shares its arm with: a pattern binder's alternatives
/// have its name and its exact lexical region.
fn alternatives(candidates: &[Scoped], resolved: &Scoped) -> Vec<Occurrence> {
    if !resolved.introduces {
        return vec![resolved.at.clone()];
    }
    candidates
        .iter()
        .filter(|b| b.introduces && b.scope == resolved.scope)
        .map(|b| b.at.clone())
        .collect()
}

fn resolve(candidates: &[Scoped], offset: usize) -> Option<&Scoped> {
    candidates
        .iter()
        .filter(|b| b.bytes.start <= offset && b.scope.contains(&offset))
        .max_by_key(|b| b.bytes.start)
}

pub(super) fn uses(
    text: &str,
    word: &str,
    binding: &Binding,
    include_declaration: bool,
    enc: PositionEncoding,
) -> Vec<Occurrence> {
    occurrences(text, word, enc)
        .into_iter()
        .filter(|found| {
            byte_offset(text, (found.line, found.start), enc)
                .and_then(|offset| resolve(&binding.candidates, offset))
                .is_some_and(|resolved| binding.group.contains(&resolved.at))
        })
        .filter(|found| include_declaration || !binding.group.contains(found))
        .collect()
}

fn byte_offset(text: &str, cursor: (u32, u32), enc: PositionEncoding) -> Option<usize> {
    let row = usize::try_from(cursor.0).ok()?;
    let line = text.split_inclusive('\n').nth(row)?;
    let start: usize = text.split_inclusive('\n').take(row).map(str::len).sum();
    Some(start + prefix_to(line, cursor.1, enc).len())
}

/// The compiler's position (1-based line, 0-based byte column) of byte `offset`.
fn position(text: &str, offset: usize) -> Option<Position> {
    let before = text.get(..offset)?;
    let column = before
        .rfind('\n')
        .map_or(offset, |newline| offset - newline - 1);
    Some(Position {
        line: u32::try_from(before.matches('\n').count() + 1).ok()?,
        column: u32::try_from(column).ok()?,
    })
}

fn occurrence(text: &str, bytes: Range<usize>, enc: PositionEncoding) -> Option<Occurrence> {
    let (line, start, _, end) = crate::warning_actions::byte_span(text, &bytes, enc)?;
    Some(Occurrence { line, start, end })
}

/// The nearest function declaration preceding a hover request.
pub(crate) fn enclosing_function(symbols: &[SymbolInfo], line: u32) -> Option<&SymbolInfo> {
    let cursor = line.saturating_add(1);
    symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .filter(|s| s.position.is_some_and(|p| p.line <= cursor))
        .max_by_key(|s| s.position.map_or(0, |p| p.line))
}
