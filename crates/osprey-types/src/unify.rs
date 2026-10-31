//! Unification — the heart of the inferencer: one entry point plus
//! category-specific helpers for each pair of type shapes, including the
//! Osprey-specific rules:
//!   * `any` unifies with anything;
//!   * the bare-generic wildcard rule: a bare constructor name (`List`, `Map`,
//!     `Fiber`, `Channel`) unifies with any parameterization of itself
//!     (`List<T>`);
//!   * structural record unification by field name+type;
//!   * directional assignability, including safe implicit `Success` wrapping,
//!     via [`unify_assignable`].

use crate::ctx::InferCtx;
use crate::error::TypeError;
use crate::ty::{names, Type, VarId};
use osprey_ast::Variance;

mod erasure;

/// Unify two types, recording the solution in `ctx`. Errors are structural; a
/// failing call may have applied partial bindings, so callers that want to
/// "try" a unification should pre-check shapes rather than relying on rollback.
pub(crate) fn unify(ctx: &mut InferCtx, a: &Type, b: &Type) -> Result<(), TypeError> {
    let (a, b) = ctx.seen_through(a, b);
    match (&a, &b) {
        (Type::Var(x), Type::Var(y)) if x == y => Ok(()),
        (Type::Var(x), _) => bind_var(ctx, *x, &b),
        (_, Type::Var(y)) => bind_var(ctx, *y, &a),

        // `any` is the universal wildcard.
        // `any` is the erased compatibility wildcard [TYPE-ANY].
        _ if a.is_named(names::ANY) || b.is_named(names::ANY) => Ok(()),

        (Type::Con { name: n1, args: a1 }, Type::Con { name: n2, args: a2 }) => {
            unify_con(ctx, n1, a1, n2, a2, &a, &b)
        }

        (
            Type::Fun {
                params: p1,
                ret: r1,
            },
            Type::Fun {
                params: p2,
                ret: r2,
            },
        ) => unify_fun(ctx, p1, r1, p2, r2),

        (Type::Record { fields: f1, .. }, Type::Record { fields: f2, .. }) => {
            unify_record(ctx, f1, f2, &a, &b)
        }

        // A nominal nullary constructor and a record of the same name describe
        // the same type — a record-typed annotation (`Con "Point"`) meeting a
        // constructed record value (`Record "Point"{..}`).
        (Type::Con { name: n, args }, Type::Record { name: rn, .. })
        | (Type::Record { name: rn, .. }, Type::Con { name: n, args })
            if args.is_empty() && n == rn =>
        {
            Ok(())
        }

        (
            Type::Con { name, args },
            Type::Record {
                name: actual,
                fields,
            },
        )
        | (
            Type::Record {
                name: actual,
                fields,
            },
            Type::Con { name, args },
        ) if name == actual => match ctx.record_fields(name, args) {
            Some(declared) => unify_record(ctx, &declared, fields, &a, &b),
            None => Err(TypeError::mismatch(&a, &b)),
        },

        (
            Type::Union {
                name: n1,
                variants: v1,
            },
            Type::Union {
                name: n2,
                variants: v2,
            },
        ) => {
            if n1 != n2 || v1.len() != v2.len() {
                return Err(TypeError::mismatch(&a, &b));
            }
            unify_seq(ctx, v1, v2, &a, &b)
        }

        _ => Err(ctx.mismatch(&a, &b)),
    }
}

/// Directional assignment-site unification: `actual` must be usable where
/// `expected` is demanded. Plain [`unify`] stays symmetric — this is the only
/// place a one-way rule may live.
pub(crate) fn unify_assignable(
    ctx: &mut InferCtx,
    expected: &Type,
    actual: &Type,
) -> Result<(), TypeError> {
    let (expected, actual) = ctx.seen_through(expected, actual);
    // Erasure is ONE-WAY [TYPE-ANY]. Every value assigns into an `any` slot;
    // no annotation recovers one back out. Recovery by declared type was an
    // unchecked cast — it printed a heap address as a decimal integer (#209)
    // and segfaulted on a word that never was a pointer — so it is rejected
    // here rather than repaired in the backend.
    erasure::check(ctx, &expected, &actual)?;
    // A bare `T` value satisfies a `Result<T, E>` slot (implicit
    // `Success`), e.g. `fn f() -> Result<bool, E> = x > 0`.
    if let Type::Con { name, args } = &expected {
        if name == names::RESULT
            && !matches!(actual, Type::Var(_))
            && !actual.is_named(names::RESULT)
        {
            if let Some(inner) = args.first() {
                erasure::check(ctx, inner, &actual)?;
                return unify(ctx, inner, &actual);
            }
        }
    }
    // Function values unify assignably in both positions: returns are
    // covariant and parameters match assignably with the roles flipped. The
    // recursive calls retain the one safe coercion above (bare T -> Success),
    // while Result -> T is rejected in every direction.
    if let (
        Type::Fun {
            params: ep,
            ret: er,
        },
        Type::Fun {
            params: ap,
            ret: ar,
        },
    ) = (&expected, &actual)
    {
        if ep.len() == ap.len() {
            for (e, a) in ep.iter().zip(ap) {
                unify_assignable(ctx, a, e)?;
            }
            return unify_assignable(ctx, er, ar);
        }
    }
    // Declared variance directs how a constructor's arguments match at
    // assignment sites: a covariant (`out`) argument matches assignably, a
    // contravariant (`in`) argument matches assignably with the roles flipped,
    // and an invariant argument must unify exactly — plain `unify` (and with
    // it HM principal types) is untouched. Implements [TYPE-VARIANCE-ASSIGN].
    if let Some(result) = unify_declared_variance_args(ctx, &expected, &actual) {
        return result;
    }
    unify(ctx, &expected, &actual)
}

/// Match a constructor's expected/actual argument lists under the declared
/// per-parameter variance. The leaves use EXACT unification; the directional
/// bare-to-Result Success coercion is representation-changing and therefore
/// applies only at direct value sites. Implements [TYPE-VARIANCE-ASSIGN].
fn unify_args_with_variance(
    ctx: &mut InferCtx,
    expected: &[Type],
    actual: &[Type],
    variances: &[Variance],
) -> Result<(), TypeError> {
    for ((e, a), v) in expected.iter().zip(actual).zip(variances) {
        match v {
            Variance::Covariant => unify_variant_arg(ctx, e, a)?,
            Variance::Contravariant => unify_variant_arg(ctx, a, e)?,
            Variance::Invariant => unify(ctx, e, a)?,
        }
    }
    Ok(())
}

/// One variance-position argument: recurse directionally through same-name
/// variance-declared constructors, and unify exactly everywhere else (plain
/// `unify` already normalizes function returns representation-safely).
fn unify_variant_arg(ctx: &mut InferCtx, expected: &Type, actual: &Type) -> Result<(), TypeError> {
    let expected = ctx.prune(expected);
    let actual = ctx.prune(actual);
    if let Some(result) = unify_declared_variance_args(ctx, &expected, &actual) {
        return result;
    }
    unify(ctx, &expected, &actual)
}

/// Recurse through a matching constructor when it declares non-invariant
/// parameter variance. `None` leaves the caller to use ordinary unification.
fn unify_declared_variance_args(
    ctx: &mut InferCtx,
    expected: &Type,
    actual: &Type,
) -> Option<Result<(), TypeError>> {
    let (
        Type::Con {
            name: expected_name,
            args: expected_args,
        },
        Type::Con {
            name: actual_name,
            args: actual_args,
        },
    ) = (expected, actual)
    else {
        return None;
    };
    if expected_name != actual_name
        || expected_args.len() != actual_args.len()
        || expected_args.is_empty()
    {
        return None;
    }
    let variances = ctx.variance_of(expected_name)?.to_vec();
    if variances.len() != expected_args.len()
        || !variances
            .iter()
            .any(|variance| *variance != Variance::Invariant)
    {
        return None;
    }
    Some(unify_args_with_variance(
        ctx,
        expected_args,
        actual_args,
        &variances,
    ))
}

fn bind_var(ctx: &mut InferCtx, id: VarId, t: &Type) -> Result<(), TypeError> {
    if let Type::Var(v) = t {
        if *v == id {
            return Ok(());
        }
    }
    if ctx.occurs(id, t) {
        return Err(TypeError::recursive(&Type::Var(id), t));
    }
    ctx.bind(id, t.clone());
    Ok(())
}

fn unify_con(
    ctx: &mut InferCtx,
    n1: &str,
    a1: &[Type],
    n2: &str,
    a2: &[Type],
    a: &Type,
    b: &Type,
) -> Result<(), TypeError> {
    if n1 == n2 && a1.len() == a2.len() {
        return unify_seq(ctx, a1, a2, a, b);
    }
    // A bare constructor name unifies with its applied form (`Fiber` ~
    // `Fiber<int>`, `Box` ~ `Box<int>`) — the bare-generic wildcard rule,
    // applied to every nominal type so a bare-named annotation accepts a
    // parameterized value.
    if n1 == n2 && (a1.is_empty() || a2.is_empty()) {
        return Ok(());
    }
    Err(ctx.mismatch(a, b))
}

fn unify_fun(
    ctx: &mut InferCtx,
    p1: &[Type],
    r1: &Type,
    p2: &[Type],
    r2: &Type,
) -> Result<(), TypeError> {
    if p1.len() != p2.len() {
        return Err(TypeError::new(format!(
            "function arity mismatch: {} vs {} parameters",
            p1.len(),
            p2.len()
        )));
    }
    for (x, y) in p1.iter().zip(p2) {
        unify(ctx, x, y)?;
    }
    unify(ctx, r1, r2)
}

fn unify_record(
    ctx: &mut InferCtx,
    f1: &std::collections::BTreeMap<String, Type>,
    f2: &std::collections::BTreeMap<String, Type>,
    a: &Type,
    b: &Type,
) -> Result<(), TypeError> {
    if f1.len() != f2.len() {
        return Err(TypeError::mismatch(a, b));
    }
    for (name, t1) in f1 {
        match f2.get(name) {
            Some(t2) => unify(ctx, t1, t2)?,
            None => {
                return Err(TypeError::new(format!(
                    "record field mismatch: {a} has no field `{name}` to match {b}"
                )))
            }
        }
    }
    Ok(())
}

fn unify_seq(
    ctx: &mut InferCtx,
    xs: &[Type],
    ys: &[Type],
    a: &Type,
    b: &Type,
) -> Result<(), TypeError> {
    if xs.len() != ys.len() {
        return Err(TypeError::mismatch(a, b));
    }
    for (x, y) in xs.iter().zip(ys) {
        unify(ctx, x, y)?;
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    unused_results,
    reason = "tests drive unification for its side effects and discard the returned types"
)]
mod tests;
