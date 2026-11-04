//! Binder token provenance survives currying and equational-clause lowering.
use super::{cst::MlBinder, token::Token};
use crate::{BindingKind, BindingRange};
use osprey_ast::Position;
use std::cell::RefCell;

pub(crate) fn binding_scope(source: &str, binding: &BindingRange) -> std::ops::Range<usize> {
    let (tokens, _) = super::lexer::lex(source);
    if let Some(scope) = lambda_scope(&tokens, binding, source.len()) {
        return scope;
    }
    let mut stack = Vec::new();
    let mut scopes = Vec::new();
    let mut header = 0;
    for (index, token) in tokens.iter().enumerate() {
        layout_scope(source, &tokens, index, &mut header, &mut stack, &mut scopes);
        if !token.range.is_empty() {
            header = line_start(source, token.range.start);
        }
    }
    scopes
        .into_iter()
        .filter(|(scope, body)| {
            scope.contains(&binding.range.start)
                && (binding.kind != BindingKind::Variable || binding.range.start >= *body)
                && (binding.kind == BindingKind::Variable
                    || scope.start == line_start(source, binding.range.start))
        })
        .map(|(scope, _)| scope)
        .min_by_key(std::ops::Range::len)
        .unwrap_or_else(|| inline_scope(source, binding))
}

fn lambda_scope(
    tokens: &[Token],
    binding: &BindingRange,
    end: usize,
) -> Option<std::ops::Range<usize>> {
    use super::token::TokKind;
    if binding.kind != BindingKind::Parameter {
        return None;
    }
    let owner = binding.owner_position?;
    let index = tokens.iter().rposition(|t| {
        t.pos.line == owner.line && t.pos.column <= owner.column && t.kind == TokKind::Backslash
    })?;
    let start = tokens.get(index)?.range.start;
    Some(start..lambda_end(tokens, index + 1, end))
}

fn lambda_end(tokens: &[Token], start: usize, end: usize) -> usize {
    use super::token::TokKind;
    let mut depth = 0_i32;
    for (offset, token) in tokens.iter().enumerate().skip(start) {
        let delta = delimiter_depth(&token.kind);
        if depth == 0 && (delta < 0 || token.kind == TokKind::Newline) {
            return next_byte(tokens, offset, end);
        }
        depth += delta;
    }
    end
}

fn delimiter_depth(kind: &super::token::TokKind) -> i32 {
    use super::token::TokKind;
    match kind {
        TokKind::LParen | TokKind::LBracket | TokKind::LBrace | TokKind::Indent => 1,
        TokKind::RParen | TokKind::RBracket | TokKind::RBrace | TokKind::Dedent => -1,
        _ => 0,
    }
}

fn next_byte(tokens: &[Token], start: usize, end: usize) -> usize {
    tokens
        .iter()
        .skip(start)
        .find(|t| !t.range.is_empty())
        .map_or(end, |t| t.range.start)
}

fn layout_scope(
    source: &str,
    tokens: &[Token],
    index: usize,
    header: &mut usize,
    stack: &mut Vec<(usize, usize)>,
    scopes: &mut Vec<(std::ops::Range<usize>, usize)>,
) {
    use super::token::TokKind;
    let Some(token) = tokens.get(index) else {
        return;
    };
    let next = || next_byte(tokens, index + 1, source.len());
    match token.kind {
        TokKind::Newline => *header = line_start(source, next()),
        TokKind::Indent => stack.push((*header, next())),
        TokKind::Dedent => {
            if let Some((start, body)) = stack.pop() {
                scopes.push((start..next(), body));
            }
        }
        _ => {}
    }
}

fn line_start(source: &str, offset: usize) -> usize {
    source
        .get(..offset)
        .and_then(|prefix| prefix.rfind('\n'))
        .map_or(0, |at| at + 1)
}

fn inline_scope(source: &str, binding: &BindingRange) -> std::ops::Range<usize> {
    if binding.kind == BindingKind::Variable {
        return 0..source.len();
    }
    let end = source
        .get(binding.range.end..)
        .and_then(|tail| tail.find('\n'))
        .map_or(source.len(), |offset| binding.range.end + offset);
    line_start(source, binding.range.start)..end
}

#[derive(Default)]
struct Recorder {
    tokens: Vec<Token>,
    bindings: Vec<BindingRange>,
    owner: Option<Position>,
    literals: Vec<crate::fragment_ranges::Literal>,
}
thread_local! {
    static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) };
}

struct Recording(Option<Recorder>);
impl Recording {
    fn install(recorder: Option<Recorder>) -> Self {
        Self(RECORDER.with(|cell| cell.replace(recorder)))
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        RECORDER.with(|cell| {
            let _ = cell.replace(self.0.take());
        });
    }
}

/// A fragment's positions belong to its synthetic mini-program until the
/// shared interpolation lowerer rebases them. Never borrow outer-file tokens.
pub(super) fn isolated<T>(action: impl FnOnce() -> T) -> T {
    let _guard = Recording::install(None);
    action()
}

pub(super) fn collect(source: &str) -> Vec<BindingRange> {
    scan(source).0
}

pub(super) fn literals(source: &str) -> Vec<crate::fragment_ranges::Literal> {
    scan(source).1
}

fn scan(source: &str) -> (Vec<BindingRange>, Vec<crate::fragment_ranges::Literal>) {
    let (tokens, _) = super::lexer::lex(source);
    let (items, mut errors) = super::parser::parse(source);
    let items = super::clauses::merge(items, &mut errors);
    let _guard = Recording::install(Some(Recorder {
        tokens,
        ..Recorder::default()
    }));
    let _ = super::lower::lower(items);
    let bindings = RECORDER.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .map(|r| std::mem::take(&mut r.bindings))
            .unwrap_or_default()
    });
    let literals = RECORDER.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .map(|r| std::mem::take(&mut r.literals))
            .unwrap_or_default()
    });
    (bindings, literals)
}

struct Owner(Option<Position>);
impl Drop for Owner {
    fn drop(&mut self) {
        RECORDER.with(|cell| {
            if let Some(recorder) = cell.borrow_mut().as_mut() {
                recorder.owner = self.0;
            }
        });
    }
}

pub(super) fn with_owner<T>(position: Position, action: impl FnOnce() -> T) -> T {
    let _guard = Owner(RECORDER.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .and_then(|r| r.owner.replace(position))
    }));
    action()
}

pub(super) fn parameter(binder: MlBinder, owner: Position) -> String {
    record(&binder, BindingKind::Parameter, Some(owner));
    binder.name
}

pub(super) fn with_expression_owner<T>(position: Position, action: impl FnOnce() -> T) -> T {
    with_owner(current_owner().unwrap_or(position), action)
}

fn current_owner() -> Option<Position> {
    RECORDER.with(|cell| cell.borrow().as_ref().and_then(|r| r.owner))
}

pub(super) fn pattern(binder: MlBinder) -> String {
    record(&binder, BindingKind::PatternBinding, current_owner());
    binder.name
}

pub(super) fn handler(binder: MlBinder, owner: Position) -> String {
    record(&binder, BindingKind::HandlerParameter, Some(owner));
    binder.name
}

pub(super) fn literal(position: Option<Position>) {
    RECORDER.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(recorder) = cell.as_mut() else {
            return;
        };
        let Some(position) = position else {
            return;
        };
        if let Some(token) = recorder.tokens.iter().find(|token| {
            token.pos == position && matches!(token.kind, super::token::TokKind::Str(_))
        }) {
            recorder.literals.push(crate::fragment_ranges::Literal {
                range: token.range.clone(),
                position,
                owner_position: recorder.owner,
            });
        }
    });
}

pub(super) fn variable(name: &str, position: Position) {
    record(
        &MlBinder {
            name: name.to_owned(),
            pos: Some(position),
        },
        BindingKind::Variable,
        Some(position),
    );
}

fn record(binder: &MlBinder, kind: BindingKind, owner_position: Option<Position>) {
    RECORDER.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(recorder) = cell.as_mut() else { return; };
        let Some(position) = binder.pos else { return; };
        let Some(index) = recorder.tokens.iter().position(|token| token.pos == position && !token.range.is_empty()) else { return; };
        let token = recorder.tokens.get(index).filter(|t| matches!(t.kind, super::token::TokKind::Ident(_)))
            .or_else(|| {
                recorder.tokens.get(index).filter(|token| token.kind == super::token::TokKind::KwMut)
                    .and_then(|_| recorder.tokens.get(index + 1))
            });
        let Some(token) = token.filter(|token| matches!(&token.kind, super::token::TokKind::Ident(name) if name == &binder.name)) else { return; };
        recorder.bindings.push(BindingRange { owner_position, kind, name: binder.name.clone(), occurrence: 0, range: token.range.clone() });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{binding_ranges, parse_program_with_flavor, Flavor};

    fn ranges(source: &str) -> Vec<BindingRange> {
        let parsed = parse_program_with_flavor(source, Flavor::Ml);
        assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
        let ranges = binding_ranges(source, Flavor::Ml);
        for range in &ranges {
            assert_eq!(source.get(range.range.clone()), Some(range.name.as_str()));
        }
        ranges
    }

    #[test]
    fn curried_parameters_patterns_and_shadowed_values_keep_source_identity() {
        let source = "choose first second =\n    copy = first\n    match second\n        0 => copy\n        name => name\n        name => copy\n";
        let ranges = ranges(source);
        let parameters: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == BindingKind::Parameter)
            .collect();
        assert_eq!(
            parameters
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(
            parameters
                .iter()
                .map(|r| r.owner_position)
                .collect::<Vec<_>>(),
            [
                Some(Position { line: 1, column: 0 }),
                Some(Position { line: 1, column: 1 })
            ]
        );
        let patterns: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == BindingKind::PatternBinding)
            .collect();
        assert_eq!(patterns.len(), 2);
        assert_eq!(
            patterns.iter().map(|r| r.occurrence).collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(patterns
            .iter()
            .all(|r| r.owner_position == Some(Position { line: 1, column: 1 })));
        assert_eq!(ranges.iter().filter(|r| r.name == "copy").count(), 1);
    }

    #[test]
    fn handler_parameters_and_clause_payloads_are_real_binders() {
        let source = "effect Pick\n    control choose : (int, int) => int\nrun seed =\n    handle Pick\n        choose first second => resume first\n    seed\nextract (Box payload) ignored = payload\nextract other ignored = 0\n";
        let ranges = ranges(source);
        let handlers: Vec<_> = ranges
            .iter()
            .filter(|r| r.kind == BindingKind::HandlerParameter)
            .collect();
        assert_eq!(
            handlers.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(handlers
            .iter()
            .all(|r| r.owner_position == Some(Position { line: 5, column: 8 })));
        assert_eq!(
            ranges
                .iter()
                .filter(|r| r.kind == BindingKind::PatternBinding
                    && matches!(r.name.as_str(), "payload" | "other"))
                .count(),
            2
        );
        assert!(ranges.iter().any(|r| r.name == "ignored"
            && r.kind == BindingKind::Parameter
            && r.owner_position == Some(Position { line: 7, column: 1 })));
    }

    #[test]
    fn interpolation_and_nested_collection_cannot_steal_outer_tokens() {
        let source = "outer alpha = \"${(\\alpha => 1) 2}\"\nnext beta = beta\n";
        let result = ranges(source);
        assert_eq!(
            result
                .iter()
                .filter(|r| r.kind == BindingKind::Parameter)
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "alpha", "beta"]
        );
        let starts: Vec<_> = result
            .iter()
            .filter(|r| r.name == "alpha")
            .map(|r| r.range.start)
            .collect();
        assert_eq!(
            starts,
            source
                .match_indices("alpha")
                .map(|(start, _)| start)
                .collect::<Vec<_>>()
        );
        let (tokens, _) = super::super::lexer::lex("sentinel = 1\n");
        let _guard = Recording::install(Some(Recorder {
            tokens,
            ..Recorder::default()
        }));
        let inner = ranges("inner name = name\n");
        assert_eq!(inner.iter().filter(|r| r.name == "name").count(), 1);
        let _ = parse_program_with_flavor("sentinel = 1\n", Flavor::Ml);
        variable("sentinel", Position { line: 1, column: 0 });
        RECORDER.with(|cell| {
            let cell = cell.borrow();
            let Some(recorder) = cell.as_ref() else {
                panic!("outer recorder lost");
            };
            assert_eq!(recorder.bindings.len(), 1);
            assert_eq!(
                recorder.bindings.first().map(|r| r.name.as_str()),
                Some("sentinel")
            );
        });
    }
}
