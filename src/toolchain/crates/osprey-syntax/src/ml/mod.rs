//! The **ML flavor** frontend: a layout-based, curry-by-default source surface
//! that lowers to the same canonical [`osprey_ast::Program`] as the Default
//! frontend, after which the two are indistinguishable ([FLAVOR-BOUNDARY]).
//!
//! Surface reference: `docs/specs/language/0024-MLFlavorSyntax.md`. Boundary and
//! lowering contract: `docs/specs/language/0023-LanguageFlavors.md`.
//!
//! Clean three-stage frontend with a parse/lower seam ([FLAVOR-FRONTEND]):
//! 1. [`lexer`] turns source into a layout-resolved [`token`] stream (the
//!    offside rule, [FLAVOR-ML-LAYOUT]);
//! 2. [`parser`] is a hand-written recursive-descent + Pratt parser producing
//!    the faithful ML [`cst`] (no canonicalisation);
//! 3. [`lower`] normalises the CST into the canonical AST (currying, pipe
//!    desugaring, interpolation), the sole place ML meets the shared core.
//!
//! Algebraic effects are supported: `effect`/`perform`/`handle`/`resume` lower
//! to the same canonical [`Stmt::Effect`](osprey_ast::Stmt::Effect),
//! [`Expr::Perform`](osprey_ast::Expr::Perform),
//! [`Expr::Handler`](osprey_ast::Expr::Handler), and
//! [`Expr::Resume`](osprey_ast::Expr::Resume) the Default flavor emits
//! ([FLAVOR-ML-EFFECT]). Callable `handler` values lower to closures around the
//! same lexical handler. A standalone `do` remains reserved.

use crate::{Flavor, Parsed};

mod annotation_edits;
mod binding_ranges;
mod clauses;
mod cst;
mod doc_text;
mod kernel;
mod lexer;
mod lower;
mod module_lower;
mod module_parse;
mod modules;
mod parser;
mod token;

/// The indentation depth at each physical line's first ML token and whether
/// the line continues a block doc comment. Implements [FLAVOR-ML-LAYOUT].
#[must_use]
pub fn ml_line_layout(source: &str) -> Vec<(Option<i32>, bool)> {
    use token::TokKind;
    let mut lines = vec![(None, false); source.lines().count()];
    let mut depth = 0;
    for tok in lexer::lex(source).0 {
        match tok.kind {
            TokKind::Indent => depth += 1,
            TokKind::Dedent => depth -= 1,
            TokKind::Newline | TokKind::Eof => {}
            _ => record_line_layout(source, &mut lines, tok, depth),
        }
    }
    lines
}

fn record_line_layout(
    source: &str,
    lines: &mut [(Option<i32>, bool)],
    tok: token::Token,
    depth: i32,
) {
    let Ok(index) = usize::try_from(tok.pos.line.saturating_sub(1)) else {
        return;
    };
    if let Some(line) = lines.get_mut(index) {
        let _ = line.0.get_or_insert(depth);
    }
    if matches!(tok.kind, token::TokKind::Doc(_)) {
        let text = source.get(tok.range).unwrap_or_default();
        let continuation = text.bytes().filter(|&byte| byte == b'\n').count();
        for line in lines.iter_mut().skip(index + 1).take(continuation) {
            line.1 = true;
        }
    }
}

/// Parse ML-flavor source into the canonical [`Program`](osprey_ast::Program):
/// lex → parse to CST → lower to AST. Best-effort, carrying any syntax errors.
pub(crate) fn parse_ml(source: &str) -> Parsed {
    let (items, mut errors) = parser::parse(source);
    // Clause sets collapse to `match` before lowering, so the shared core only
    // ever sees the plain definition form ([FLAVOR-ML-CLAUSES]).
    let items = clauses::merge(items, &mut errors);
    let (program, lower_errors) = binding_ranges::isolated(|| lower::lower(items));
    errors.extend(lower_errors);
    Parsed {
        program,
        errors,
        flavor: Flavor::Ml,
    }
}

pub(crate) fn annotation_edits(source: &str) -> Vec<crate::AnnotationEdit> {
    annotation_edits::resolve_owners(source, parser::parse_annotations(source).2)
}

pub(crate) fn binding_ranges(source: &str) -> Vec<crate::BindingRange> {
    binding_ranges::collect(source)
}

pub(crate) use binding_ranges::binding_scope;

pub(crate) fn string_literals(source: &str) -> Vec<crate::fragment_ranges::Literal> {
    binding_ranges::literals(source)
}
