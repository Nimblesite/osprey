//! Flavor-neutral desugarings — the canonical AST shapes that more than one
//! surface form lowers to.
//!
//! A surface form that exists in both flavors must produce the *same* node from
//! both, or the twin pair stops emitting identical LLVM IR ([FLAVOR-IR-EQUIV]).
//! Stating the shape once here is what makes that true by construction rather
//! than by review.

use osprey_ast::{Expr, MatchArm, Pattern};

/// The two-arm boolean match that conditional forms in both flavors desugar
/// to: Default `if`/`else` and the Default ternary `{ c } ? a : b`.
pub(crate) fn bool_match(condition: Expr, then: Expr, otherwise: Expr) -> Expr {
    Expr::Match {
        value: Box::new(condition),
        arms: vec![bool_arm(true, then), bool_arm(false, otherwise)],
    }
}

/// `result ?: fallback` as an explicit exhaustive Result match. Keeping this
/// distinct from [`bool_match`] prevents an ordinary `if result` or ternary
/// condition from acquiring implicit Result-unwrapping semantics.
pub(crate) fn result_default(scrutinee: Expr, fallback: Expr) -> Expr {
    const PAYLOAD: &str = "$__osprey_result_default";
    Expr::Match {
        value: Box::new(scrutinee),
        arms: vec![
            MatchArm {
                pattern: Pattern::Constructor {
                    name: "Success".to_string(),
                    fields: vec![PAYLOAD.to_string()],
                    sub_patterns: Vec::new(),
                },
                body: Expr::Identifier(PAYLOAD.to_string()),
            },
            MatchArm {
                pattern: Pattern::Constructor {
                    name: "Error".to_string(),
                    fields: Vec::new(),
                    sub_patterns: Vec::new(),
                },
                body: fallback,
            },
        ],
    }
}

fn bool_arm(matches: bool, body: Expr) -> MatchArm {
    MatchArm {
        pattern: Pattern::Literal(Box::new(Expr::Bool(matches))),
        body,
    }
}
