//! Flavor-neutral desugarings — the canonical AST shapes that more than one
//! surface form lowers to.
//!
//! A surface form that exists in both flavors must produce the *same* node from
//! both, or the twin pair stops emitting identical LLVM IR ([FLAVOR-IR-EQUIV]).
//! Stating the shape once here is what makes that true by construction rather
//! than by review.

use osprey_ast::{Expr, MatchArm, Pattern, Stmt, TypeExpr};

/// The built-in `Result` constructors. Their payload binds by ROLE — `value`
/// carries the success payload, `message` the error text — rather than by
/// column, so they are the one constructor pattern whose binders stay in
/// `fields` in both flavors.
const RESULT_OK: &str = "Success";
const RESULT_ERR: &str = "Error";

pub(crate) fn is_result_ctor(name: &str) -> bool {
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
    // A `_` payload binds nothing, exactly as the Default flavor's
    // `Ctor(_)` lowers ([FLAVOR-IR-EQUIV]).
    let sub_pattern = |binder: String| match binder.as_str() {
        "_" => Pattern::Wildcard,
        _ => Pattern::Binding(binder),
    };
    Pattern::Constructor {
        name,
        fields: Vec::new(),
        sub_patterns: binders.into_iter().map(sub_pattern).collect(),
    }
}

/// `Ctor(p, …)` in either flavor. A lone binder under a `Result`
/// constructor binds the payload by role, exactly as `Success found` does
/// ([PATTERN-RESULT-BINDERS]); anything else takes its column
/// ([PATTERN-NESTED]).
pub(crate) fn positional_ctor_pattern(name: String, slots: Vec<Pattern>) -> Pattern {
    if let [Pattern::Binding(binder)] = slots.as_slice() {
        if is_result_ctor(&name) {
            return ctor_pattern(name, vec![binder.clone()]);
        }
    }
    Pattern::Constructor {
        name,
        fields: Vec::new(),
        sub_patterns: slots,
    }
}

/// `Ctor { a, b: p }` / ML `Ctor(a = p)`: bare fields bind by name; once any
/// slot is a pattern the arm carries every slot as a labelled alias
/// ([PATTERN-NESTED]).
pub(crate) fn labelled_ctor_pattern(name: String, slots: Vec<(String, Pattern)>) -> Pattern {
    if slots
        .iter()
        .all(|(field, slot)| slot.slot_binder() == field)
    {
        return Pattern::Constructor {
            name,
            fields: slots.into_iter().map(|(field, _)| field).collect(),
            sub_patterns: Vec::new(),
        };
    }
    let (fields, sub_patterns) = slots.into_iter().unzip();
    Pattern::Constructor {
        name,
        fields,
        sub_patterns,
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

/// A layout block: its statements and trailing value. A block with no
/// statements IS its value — an ML layout body and a Default `handle` region
/// lower to one node ([FLAVOR-IR-EQUIV]). Braces the user wrote are not layout
/// and keep their [`Expr::Block`] (see the Default block lowering).
pub(crate) fn block(
    statements: Vec<Stmt>,
    value: Option<Box<Expr>>,
    position: Option<osprey_ast::Position>,
) -> Expr {
    match (statements.is_empty(), value) {
        (true, Some(value)) => *value,
        (_, value) => Expr::Block {
            statements,
            value,
            position,
        },
    }
}

/// Preserve an executable expression's source anchor without adding work.
/// Implements [TESTING-COVERAGE-CODEGEN]; position-blind rendering erases this scope.
pub(crate) fn positioned(value: Expr, position: Option<osprey_ast::Position>) -> Expr {
    Expr::Block {
        statements: Vec::new(),
        value: Some(Box::new(value)),
        position,
    }
}

/// A record field's type in the canonical spelling [`osprey_ast::TypeField`]
/// stores, shared by every frontend that lowers typed annotations.
pub(crate) fn field_type(ty: &TypeExpr) -> String {
    if ty.is_function {
        let result = ty
            .return_type
            .as_deref()
            .map_or_else(|| "Unit".into(), field_type);
        return format!(
            "({}) -> {result}",
            ty.parameter_types
                .iter()
                .map(field_type)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    osprey_ast::effect_name::instantiated(&ty.name, &ty.generic_params)
}
