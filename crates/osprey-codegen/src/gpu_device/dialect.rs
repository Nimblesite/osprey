//! The device dialect's spellings and the AST shapes the emitter recognises:
//! literals, the two binary-operator tables, the desugared `?:` gather and the
//! boolean `match` [GPU-BACKEND-DEVICE].

use super::emit::Emitted;
use super::Ty;
use osprey_ast::{Expr, MatchArm, Pattern};

const RESULT_OK: &str = "Success";
const RESULT_ERR: &str = "Error";

pub(super) fn is_literal(expression: &Expr) -> bool {
    match expression {
        Expr::Integer(_) | Expr::Float(_) | Expr::Bool(_) => true,
        Expr::Unary { op, operand } => op == "-" && is_literal(operand),
        _ => false,
    }
}

pub(super) fn int_literal(n: i64) -> String {
    match n {
        i64::MIN => "LONG_MIN_WORD".to_string(),
        n if n < 0 => format!("(-(long){})", -n),
        n => format!("((long){n})"),
    }
}

pub(super) fn float_literal(f: f64) -> String {
    format!("0x{:016x}ul", f.to_bits())
}

pub(super) fn describe(expression: &Expr) -> &'static str {
    match expression {
        Expr::Lambda { .. } => "a function value",
        Expr::Str(_) | Expr::InterpolatedStr(_) => "a string",
        Expr::List(..) => "a list",
        Expr::FieldAccess { .. } => "a field access",
        Expr::TypeConstructor { .. } | Expr::Object(_) | Expr::Update { .. } => "a record",
        Expr::Perform { .. } => "an effect operation",
        _ => "this expression",
    }
}

/// The callee behind an explicit type application.
pub(super) fn callee(function: &Expr) -> &Expr {
    match function {
        Expr::TypeApply { function, .. } => callee(function),
        other => other,
    }
}

/// `gpuGet(buffer, index) ?: fallback`, as the parser desugars it.
pub(super) fn elvis_gather<'a>(
    value: &'a Expr,
    arms: &'a [MatchArm],
) -> Option<(&'a Expr, &'a Expr, &'a Expr)> {
    let Expr::Call {
        function,
        arguments,
        ..
    } = value
    else {
        return None;
    };
    let [buffer, index] = arguments.as_slice() else {
        return None;
    };
    let [ok, err] = arms else {
        return None;
    };
    let is_get = matches!(callee(function), Expr::Identifier(name) if name == "gpuGet");
    let is_ok = matches!((&ok.pattern, &ok.body), (Pattern::Constructor { name, fields, .. }, Expr::Identifier(bound))
        if name == RESULT_OK && fields.first() == Some(bound));
    let is_err = matches!(&err.pattern, Pattern::Constructor { name, .. } if name == RESULT_ERR);
    (is_get && is_ok && is_err).then_some((buffer, index, &err.body))
}

/// A two-arm boolean match, in either order; a wildcard second arm is the
/// other case.
pub(super) fn bool_arms(arms: &[MatchArm]) -> Option<(&Expr, &Expr)> {
    let [first, second] = arms else {
        return None;
    };
    let literal = |pattern: &Pattern| match pattern {
        Pattern::Literal(value) => match value.as_ref() {
            Expr::Bool(b) => Some(*b),
            _ => None,
        },
        _ => None,
    };
    let (Some(a), b) = (literal(&first.pattern), literal(&second.pattern)) else {
        return None;
    };
    let other_is_rest = b == Some(!a) || matches!(second.pattern, Pattern::Wildcard);
    other_is_rest.then_some(if a {
        (&first.body, &second.body)
    } else {
        (&second.body, &first.body)
    })
}

pub(super) fn int_binary(op: &str, l: &str, r: &str) -> Emitted {
    Ok(match op {
        "+" => (Ty::INT, format!("osprey_add({l}, {r}, fault)")),
        "-" => (Ty::INT, format!("osprey_sub({l}, {r}, fault)")),
        "*" => (Ty::INT, format!("osprey_mul({l}, {r}, fault)")),
        "%" => (Ty::INT, format!("osprey_rem({l}, {r}, fault)")),
        "/" => (
            Ty::FLOAT,
            format!("osprey_fdiv(from_long({l}), from_long({r}), fault)"),
        ),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => (Ty::Bool, format!("({l} {op} {r})")),
        _ => return Err(format!("`{op}` over ints has no device form")),
    })
}

pub(super) fn float_binary(op: &str, l: &str, r: &str) -> Emitted {
    Ok(match op {
        "+" => (Ty::FLOAT, format!("add64({l}, {r})")),
        "-" => (Ty::FLOAT, format!("sub64({l}, {r})")),
        "*" => (Ty::FLOAT, format!("multiply64({l}, {r})")),
        "/" => (Ty::FLOAT, format!("osprey_fdiv({l}, {r}, fault)")),
        "==" => (Ty::Bool, format!("eq64({l}, {r})")),
        "!=" => (Ty::Bool, format!("(!eq64({l}, {r}))")),
        "<" => (Ty::Bool, format!("lt64({l}, {r})")),
        "<=" => (Ty::Bool, format!("le64({l}, {r})")),
        ">" => (Ty::Bool, format!("lt64({r}, {l})")),
        ">=" => (Ty::Bool, format!("le64({r}, {l})")),
        _ => return Err(format!("`{op}` over floats has no device form")),
    })
}
