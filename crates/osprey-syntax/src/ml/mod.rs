//! The **ML flavor** frontend: a layout-based, curry-by-default source surface
//! that lowers to the same canonical [`osprey_ast::Program`] as the Default
//! frontend, after which the two are indistinguishable ([FLAVOR-BOUNDARY]).
//!
//! Surface reference: `docs/specs/0024-MLFlavorSyntax.md`. Boundary and
//! lowering contract: `docs/specs/0023-LanguageFlavors.md`. Build sequence:
//! `docs/plans/0013-ml-flavor-frontend.md`.
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
//! ([FLAVOR-ML-EFFECT]). First-class handler values (`handler`/`do`) are not yet
//! in the shared core, so the parser reports a precise "not yet supported" error
//! for those rather than misparsing them.

use crate::{Flavor, Parsed};

mod cst;
mod lexer;
mod lower;
mod module_lower;
mod module_parse;
mod modules;
mod parser;
mod token;

/// Parse ML-flavor source into the canonical [`Program`](osprey_ast::Program):
/// lex → parse to CST → lower to AST. Best-effort, carrying any syntax errors.
pub(crate) fn parse_ml(source: &str) -> Parsed {
    let (items, errors) = parser::parse(source);
    Parsed {
        program: lower::lower(items),
        errors,
        flavor: Flavor::Ml,
    }
}
