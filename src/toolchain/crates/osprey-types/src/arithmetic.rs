//! Arithmetic values and implicit requests. Implements [ARITH-EFFECT].

use crate::check::Checker;
use crate::effect_rows::ExprArgs;
use crate::error::TypeError;
use crate::numeric::Numeric;
use crate::ty::Type;
use osprey_ast::{AstNode, Expr, Position};
use std::collections::HashMap;

/// [BUILTIN-ABS]: one numeric overload per use; wrappers retain its open type
/// until their call sites resolve it, just like other arithmetic operators.
pub(crate) fn absolute_scheme() -> crate::ty::Scheme {
    let signature = Type::fun(vec![Type::Var(0)], Type::Var(0));
    let mut scheme = crate::ty::Scheme::poly(vec![0], signature.clone());
    scheme.obligations.push(("arith abs".into(), signature));
    scheme
        .obligations
        .push(("numeric abs".into(), Type::Var(0)));
    scheme
}

/// The open-scheme families: one row per operand obligation, each naming the
/// prefix its obligation is spelled with. All three share [`open_scheme`].
macro_rules! open_schemes {
    ($($(#[$doc:meta])* $name:ident = $prefix:ident;)*) => {
        $(
            $(#[$doc])*
            pub(crate) fn $name(op: &str, arity: usize, ret: Type) -> crate::ty::Scheme {
                open_scheme(crate::builtin_constraints::$prefix, op, arity, ret)
            }
        )*
    };
}

open_schemes! {
    /// [TYPE-NUMERIC-SIZED] A builtin `(T, …) -> ret` open over the integer
    /// types: one overload per use, retained by wrappers until their call sites
    /// resolve it, exactly as [`absolute_scheme`] is. `ret` is written over
    /// `Type::Var(0)`.
    open_integer_scheme = INTEGER_OPERAND_PREFIX;
    /// [BUILTIN-MATH] A builtin open over the three float widths.
    open_float_scheme = FLOAT_OPERAND_PREFIX;
    /// [BUILTIN-CONVERT-FLOAT] A conversion open over every numeric source type.
    open_numeric_scheme = NUMERIC_OPERAND_PREFIX;
}

fn open_scheme(prefix: &str, op: &str, arity: usize, ret: Type) -> crate::ty::Scheme {
    let operands = vec![Type::Var(0); arity];
    let mut scheme = crate::ty::Scheme::poly(vec![0], Type::fun(operands.clone(), ret));
    scheme
        .obligations
        .push((format!("arith {op}"), Type::fun(operands, Type::Var(0))));
    scheme
        .obligations
        .push((format!("{prefix}{op}"), Type::Var(0)));
    scheme
}

/// The float builtins whose unresolved operands default to `float`, never `int`.
pub(crate) fn is_float_open(op: &str) -> bool {
    op == "floatToInt" || UNARY_FLOAT_BUILTINS.contains(&op) || BINARY_FLOAT_BUILTINS.contains(&op)
}

/// The unary functions open over the float widths [BUILTIN-MATH].
pub(crate) const UNARY_FLOAT_BUILTINS: [&str; 25] = [
    "sqrt", "exp", "ln", "tanh", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh",
    "asinh", "acosh", "atanh", "log2", "log10", "log1p", "expm1", "erf", "erfc", "floor", "ceil",
    "trunc", "round",
];

/// The binary functions open over the float widths [BUILTIN-MATH].
pub(crate) const BINARY_FLOAT_BUILTINS: [&str; 6] =
    ["pow", "atan2", "hypot", "fmod", "copysign", "floatDiv"];

fn is_integer(ty: &Type) -> bool {
    Numeric::of_type(ty).is_some_and(Numeric::is_integer)
}

fn is_float(ty: &Type) -> bool {
    Numeric::of_type(ty).is_some_and(Numeric::is_float)
}

/// Open overloads resolve at the end of inference; known invalid types are
/// reported once by the numeric operand obligation.
pub(crate) fn absolute_operand(ty: &Type) -> bool {
    crate::builtin_constraints::is_numeric_scalar(ty) || matches!(ty, Type::Var(_))
}

/// Only the integer overload can request overflow; unknown types fail closed.
pub(crate) fn absolute_overflow_possible(ty: Option<&Type>) -> bool {
    match ty {
        Some(Type::Fun { params, .. }) => match params.as_slice() {
            [operand] => is_integer(operand) || matches!(operand, Type::Var(_)),
            _ => true,
        },
        _ => true,
    }
}

/// Whether an integer conversion can leave its target's range: never when the
/// target holds every value of the known source ([BUILTIN-CONVERT-INT]).
pub(crate) fn conversion_overflow_possible(name: &str, ty: Option<&Type>) -> bool {
    let Some(target) = Numeric::conversion_target(name).filter(|n| n.is_integer()) else {
        return false;
    };
    let source = match ty {
        Some(Type::Fun { params, .. }) => params.first().and_then(Numeric::of_type),
        _ => None,
    };
    let contained = source
        .and_then(Numeric::range)
        .zip(target.range())
        .is_some_and(|((low, high), (least, most))| least <= low && high <= most);
    !contained
}

/// Evaluate literal arithmetic without executing a fallible instruction.
/// Implements [ARITH-EFFECT-CONST]; a zero divisor remains a runtime request.
///
/// # Errors
///
/// Returns the `constant arithmetic overflows: …` diagnostic when a folded
/// integer result is outside the `int` range.
pub fn fold_arithmetic(expr: &Expr) -> Result<Option<Expr>, String> {
    osprey_ast::with_stack(|| match expr {
        Expr::Integer(_) | Expr::Float(_) => Ok(Some(expr.clone())),
        Expr::Binary {
            op, left, right, ..
        } => match (fold_arithmetic(left)?, fold_arithmetic(right)?) {
            (Some(left), Some(right)) => fold_binary(op, &left, &right),
            _ => Ok(None),
        },
        Expr::Unary { op, operand } if op == "-" => fold_negation(operand),
        _ => Ok(None),
    })
}

/// The overflow a constant expression introduces at its own operator, located
/// there. An overflowing operand is reported at that operand instead, so one
/// fault is one diagnostic. Implements [ARITH-EFFECT-CONST].
pub(crate) fn constant_overflow(expr: &Expr) -> Option<TypeError> {
    let operands_fold = match expr {
        Expr::Binary { left, right, .. } => {
            fold_arithmetic(left).is_ok() && fold_arithmetic(right).is_ok()
        }
        Expr::Unary { op, operand } if op == "-" => fold_arithmetic(operand).is_ok(),
        _ => false,
    };
    let message = fold_arithmetic(expr).err().filter(|_| operands_fold)?;
    Some(TypeError::new(message).with_pos(operator_position(expr)))
}

/// A constant whose type may still be decided: a literal, or a folded operator
/// over literals. Checked at its resolved width once inference has finished.
/// Implements [TYPE-NUMERIC-LITERAL] and [ARITH-EFFECT-CONST].
pub(crate) struct ConstantSite {
    value: Constant,
    ty: Type,
    position: Option<Position>,
    /// The operator's own spelling; `None` for a literal.
    operator: Option<String>,
}

/// A source constant checked once against its resolved numeric type.
#[derive(Clone, Copy)]
enum Constant {
    Integer(i64),
    Float(f64),
}

impl Constant {
    /// Whether the value fits `numeric`: an integer against its range, a float
    /// by whether its nearest narrower value is infinite — `float32` rounds
    /// `1e40` to infinity; `float16` overflows from `65520.0` up
    /// ([TYPE-NUMERIC-LITERAL]).
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "the narrowing cast IS the question: whether the literal's nearest float32 is infinite"
    )]
    fn fits(self, numeric: Numeric) -> bool {
        match self {
            Constant::Integer(value) => numeric.fits(i128::from(value)),
            Constant::Float(value) if numeric == Numeric::FLOAT32 => !(value as f32).is_infinite(),
            Constant::Float(value) if numeric == Numeric::FLOAT16 => value.abs() < 65520.0,
            Constant::Float(_) => true,
        }
    }

    /// The literal as its diagnostic spells it ([TYPE-NUMERIC-LITERAL]).
    fn spelling(self) -> String {
        match self {
            Constant::Integer(value) => value.to_string(),
            Constant::Float(value) => format!("{value:e}"),
        }
    }
}

impl Checker {
    /// Bank a constant site for the width check that follows inference.
    fn bank_constant(
        &mut self,
        value: Constant,
        ty: &Type,
        position: Option<Position>,
        operator: Option<String>,
    ) {
        self.constants.push(ConstantSite {
            value,
            ty: ty.clone(),
            position,
            operator,
        });
    }

    /// The type of a numeric literal: open until a context decides it.
    fn literal(&mut self, value: Constant, numeric: Numeric) -> Type {
        let ty = self.ctx.fresh_literal(numeric);
        self.bank_constant(value, &ty, self.expr_position, None);
        ty
    }

    /// The type of an integer literal: open until a context decides it.
    pub(crate) fn integer_literal(&mut self, value: i64) -> Type {
        self.literal(Constant::Integer(value), Numeric::INT)
    }

    /// The type of a float literal: open until a float context decides it.
    pub(crate) fn float_literal(&mut self, value: f64) -> Type {
        self.literal(Constant::Float(value), Numeric::FLOAT)
    }

    /// Bank a folded operator for the width check.
    pub(crate) fn record_constant(&mut self, expr: &Expr, ty: &Type) {
        let (Expr::Binary { .. } | Expr::Unary { .. }) = expr else {
            return;
        };
        if let Ok(Some(Expr::Integer(value))) = fold_arithmetic(expr) {
            self.bank_constant(
                Constant::Integer(value),
                ty,
                operator_position(expr).or(self.expr_position),
                Some(spelling(expr)),
            );
        }
    }

    /// Reject each constant outside the type it resolved to.
    pub(crate) fn validate_constants(&mut self) {
        for site in std::mem::take(&mut self.constants) {
            let ty = self.ctx.apply(&site.ty);
            let Some(numeric) = Numeric::of_type(&ty).filter(|n| !site.value.fits(*n)) else {
                continue;
            };
            let name = numeric.name();
            let message = match site.operator {
                Some(written) => format!("constant arithmetic overflows: {written} at {name}"),
                None => format!("literal {} does not fit {name}", site.value.spelling()),
            };
            self.errors
                .push(TypeError::new(message).with_pos(site.position));
        }
    }
}

/// A constant operator written out over its folded operands.
fn spelling(expr: &Expr) -> String {
    let folded = |e: &Expr| match fold_arithmetic(e) {
        Ok(Some(Expr::Integer(value))) => value.to_string(),
        _ => String::new(),
    };
    match expr {
        Expr::Binary {
            op, left, right, ..
        } => format!("{} {op} {}", folded(left), folded(right)),
        Expr::Unary { operand, .. } => format!("-({})", folded(operand)),
        _ => String::new(),
    }
}

/// A binary operator's own position; a negation borrows its operand's.
fn operator_position(expr: &Expr) -> Option<Position> {
    match expr {
        Expr::Binary { position, .. } => *position,
        Expr::Unary { operand, .. } => operator_position(operand),
        _ => None,
    }
}

fn fold_negation(operand: &Expr) -> Result<Option<Expr>, String> {
    match fold_arithmetic(operand)? {
        Some(Expr::Integer(value)) => value
            .checked_neg()
            .map(Expr::Integer)
            .map(Some)
            .ok_or_else(|| format!("constant arithmetic overflows: -({value})")),
        Some(Expr::Float(value)) => Ok(Some(Expr::Float(-value))),
        _ => Ok(None),
    }
}

fn fold_binary(op: &str, left: &Expr, right: &Expr) -> Result<Option<Expr>, String> {
    if let (Expr::Integer(left), Expr::Integer(right)) = (left, right) {
        return fold_integers(op, *left, *right);
    }
    Ok(None)
}

fn fold_integers(op: &str, left: i64, right: i64) -> Result<Option<Expr>, String> {
    let value = match op {
        "+" => left.checked_add(right),
        "-" => left.checked_sub(right),
        "*" => left.checked_mul(right),
        "%" if right != 0 => Some(if right == -1 { 0 } else { left % right }),
        _ => return Ok(None),
    };
    value
        .map(Expr::Integer)
        .map(Some)
        .ok_or_else(|| format!("constant arithmetic overflows: {left} {op} {right}"))
}

/// The row seed for an operator, after resolving its numeric overload.
pub(crate) fn operation(expr: &Expr, ty: Option<&Type>) -> Option<&'static str> {
    if !matches!(fold_arithmetic(expr), Ok(None)) {
        return None;
    }
    match expr {
        Expr::Binary { op, right, .. } if op == "/" || op == "%" => {
            if nonzero_literal(right) {
                return None;
            }
            Some(if op == "/" || ty.is_some_and(is_float) {
                "divideByZero"
            } else {
                "remainderByZero"
            })
        }
        Expr::Binary { op, .. } if matches!(op.as_str(), "+" | "-" | "*") => {
            ty.filter(|ty| is_integer(ty)).map(|_| "overflow")
        }
        Expr::Unary { op, .. } if op == "-" => ty.filter(|ty| is_integer(ty)).map(|_| "overflow"),
        _ => None,
    }
}

fn nonzero_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Integer(n) if *n != 0) || matches!(expr, Expr::Float(n) if *n != 0.0)
}

/// Discharge proven-total direct numeric builtin calls, retaining alias requirements.
pub(crate) fn total_builtin_operations(name: &str, arguments: ExprArgs<'_>) -> Vec<&'static str> {
    let integer = |index| {
        arguments
            .get(index)
            .and_then(|expr| match fold_arithmetic(expr) {
                Ok(Some(Expr::Integer(n))) => Some(n),
                _ => None,
            })
    };
    match name {
        "abs" if integer(0).is_some_and(|n| n != i64::MIN) => vec!["overflow"],
        _ if Numeric::conversion_target(name)
            .is_some_and(|to| integer(0).is_some_and(|n| to.fits(i128::from(n)))) =>
        {
            vec!["overflow"]
        }
        "intDiv" => {
            let mut total = Vec::new();
            if integer(1).is_some_and(|n| n != 0) {
                total.push("remainderByZero");
            }
            if integer(1).is_some_and(|n| n != -1) || integer(0).is_some_and(|n| n != i64::MIN) {
                total.push("overflow");
            }
            total
        }
        _ => Vec::new(),
    }
}

/// Preserve inferred site identities when pipe inference borrows a cloned call.
pub(crate) fn transfer(source: &Expr, original: &Expr, types: &mut HashMap<usize, Type>) {
    let mut pending = vec![(AstNode::Expression(source), AstNode::Expression(original))];
    while let Some((source, original)) = pending.pop() {
        if let (AstNode::Expression(source), AstNode::Expression(original)) = (source, original) {
            if let Some(ty) = types.get(&std::ptr::from_ref(source).addr()).cloned() {
                let _ = types.insert(std::ptr::from_ref(original).addr(), ty);
            }
        }
        let mut source_children = Vec::new();
        let mut original_children = Vec::new();
        source.for_each_child(|child| source_children.push(child));
        original.for_each_child(|child| original_children.push(child));
        pending.extend(source_children.into_iter().zip(original_children));
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod absolute_tests;
