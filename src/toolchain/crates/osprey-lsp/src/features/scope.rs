//! Source binders resolve only inside their lexical regions.
//! Implements [LSP-DEFINITION-LOCAL] and [LSP-REFERENCES-LOCAL].

use lspkit::vfs::PositionEncoding;
use osprey_ast::Program;
use osprey_syntax::{binding_ranges, binding_scope, BindingKind, BindingRange, Flavor};
use std::ops::Range;

use super::{declaration_occurrence, symbol_matches};
use crate::analysis::{collect_all_symbols, collect_symbols, SymbolInfo, SymbolKind};
use crate::text::{occurrences, prefix_to, Occurrence};

pub(super) struct Binding {
    pub(super) at: Occurrence,
    candidates: Vec<Scoped>,
}

struct Scoped {
    at: Occurrence,
    bytes: Range<usize>,
    scope: Range<usize>,
}

pub(super) fn binding(
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
    let at = resolve(&candidates, offset)?.at.clone();
    Some(Binding { at, candidates })
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
                .is_some_and(|resolved| resolved.at == binding.at)
        })
        .filter(|found| include_declaration || *found != binding.at)
        .collect()
}

fn byte_offset(text: &str, cursor: (u32, u32), enc: PositionEncoding) -> Option<usize> {
    let row = usize::try_from(cursor.0).ok()?;
    let line = text.split_inclusive('\n').nth(row)?;
    let start: usize = text.split_inclusive('\n').take(row).map(str::len).sum();
    Some(start + prefix_to(line, cursor.1, enc).len())
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
