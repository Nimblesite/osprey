//! Flavor-neutral desugarings — the canonical AST shapes that more than one
//! surface form lowers to.
//!
//! A surface form that exists in both flavors must produce the *same* node from
//! both, or the twin pair stops emitting identical LLVM IR ([FLAVOR-IR-EQUIV]).
//! Stating the shape once here is what makes that true by construction rather
//! than by review.

use osprey_ast::{Expr, MatchArm, Pattern};

/// The built-in `Result` constructors. Their payload binds by ROLE — `value`
/// carries the success payload, `message` the error text — rather than by
/// column, so they are the one constructor pattern whose binders stay in
/// `fields` in both flavors.
const RESULT_OK: &str = "Success";
const RESULT_ERR: &str = "Error";

fn is_result_ctor(name: &str) -> bool {
    name == RESULT_OK || name == RESULT_ERR
}

/// The canonical constructor pattern for a list of binders written *positionally*.
///
/// Default spells its two destructures apart: `Ctor { a, b }` binds each binder
/// to the field of the same name, while `Ctor(a, b)` binds by column. ML has
/// only the positional `Ctor a b`, so its binders must land in `sub_patterns`
/// too — lowering them into `fields` made a well-typed ML arm read the slot
/// that happened to share the binder's spelling, and read nothing at all when
/// no field did ([TYPE-UNION-POSITIONAL], [FLAVOR-LOWER-CONTRACT]).
pub(crate) fn ctor_pattern(name: String, binders: Vec<String>) -> Pattern {
    if is_result_ctor(&name) {
        return Pattern::Constructor {
            name,
            fields: binders,
            sub_patterns: Vec::new(),
        };
    }
    Pattern::Constructor {
        name,
        fields: Vec::new(),
        sub_patterns: binders.into_iter().map(Pattern::Binding).collect(),
    }
}

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
    let payload = osprey_ast::RESULT_DEFAULT_PAYLOAD;
    Expr::Match {
        value: Box::new(scrutinee),
        arms: vec![
            MatchArm {
                pattern: Pattern::Constructor {
                    name: RESULT_OK.to_string(),
                    fields: vec![payload.to_string()],
                    sub_patterns: Vec::new(),
                },
                body: Expr::Identifier(payload.to_string()),
            },
            MatchArm {
                pattern: Pattern::Constructor {
                    name: RESULT_ERR.to_string(),
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
