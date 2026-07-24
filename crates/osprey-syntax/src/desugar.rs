//! Flavor-neutral desugarings — the canonical AST shapes that more than one
//! surface form lowers to.
//!
//! A surface form that exists in both flavors must produce the *same* node from
//! both, or the twin pair stops emitting identical LLVM IR ([FLAVOR-IR-EQUIV]).
//! Stating the shape once here is what makes that true by construction rather
//! than by review.

use osprey_ast::{Expr, MatchArm, Pattern};

/// The two-arm boolean match that every conditional form in both flavors
/// desugars to: Default `if`/`else`, the Default ternary `{ c } ? a : b`, and
/// the Result default `e ?: d` in both flavors ([PATTERN-RESULT-DEFAULT]).
///
/// The arms are boolean *literal patterns* rather than a `Success`/`Wildcard`
/// pair. That is load-bearing for `?:`: the checker reads a two-arm boolean
/// match over a `Result` as the truthiness test that yields the unwrapped
/// success payload, and a constructor pattern pair would be a different node
/// with a different type.
pub(crate) fn bool_match(condition: Expr, then: Expr, otherwise: Expr) -> Expr {
    Expr::Match {
        value: Box::new(condition),
        arms: vec![bool_arm(true, then), bool_arm(false, otherwise)],
    }
}

fn bool_arm(matches: bool, body: Expr) -> MatchArm {
    MatchArm {
        pattern: Pattern::Literal(Box::new(Expr::Bool(matches))),
        body,
    }
}
