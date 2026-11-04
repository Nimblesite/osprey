//! Assignment boundaries never recover erased values inside compound types.
use crate::ctx::InferCtx;
use crate::error::TypeError;
use crate::ty::{names, Type};
use osprey_ast::Variance;
use std::collections::BTreeMap;

pub(super) fn check(ctx: &mut InferCtx, expected: &Type, actual: &Type) -> Result<(), TypeError> {
    if matches!(expected, Type::Var(_)) || matches!(actual, Type::Var(_)) {
        return resolved(ctx, expected, actual);
    }
    if let Some(error) = recovery(expected, actual).or_else(|| anonymous(expected, actual)) {
        return Err(error);
    }
    compound(ctx, expected, actual)
}

fn resolved(ctx: &mut InferCtx, expected: &Type, actual: &Type) -> Result<(), TypeError> {
    let expected = ctx.prune(expected);
    let actual = ctx.prune(actual);
    if matches!(expected, Type::Var(_)) || matches!(actual, Type::Var(_)) {
        return Ok(());
    }
    check(ctx, &expected, &actual)
}

fn compound(ctx: &mut InferCtx, expected: &Type, actual: &Type) -> Result<(), TypeError> {
    match (expected, actual) {
        (Type::Con { name: e, args: ep }, Type::Con { name: a, args: ap }) if e == a => {
            constructors(ctx, e, ep, ap)
        }
        (Type::Union { variants: ep, .. }, Type::Union { variants: ap, .. }) => {
            variants(ctx, ep, ap)
        }
        (_, Type::Fun { params, ret }) => function(ctx, expected, params, ret),
        (Type::Record { fields: ep, .. }, Type::Record { fields: ap, .. }) => records(ctx, ep, ap),
        _ => Ok(()),
    }
}

fn variants(ctx: &mut InferCtx, expected: &[Type], actual: &[Type]) -> Result<(), TypeError> {
    for (e, a) in expected.iter().zip(actual) {
        check(ctx, e, a)?;
    }
    Ok(())
}

fn function(
    ctx: &mut InferCtx,
    expected: &Type,
    params: &[Type],
    ret: &Type,
) -> Result<(), TypeError> {
    let Type::Fun {
        params: ep,
        ret: er,
    } = expected
    else {
        return Ok(());
    };
    for (e, a) in ep.iter().zip(params) {
        check(ctx, a, e)?;
    }
    check(ctx, er, ret)
}

fn records(
    ctx: &mut InferCtx,
    expected: &BTreeMap<String, Type>,
    actual: &BTreeMap<String, Type>,
) -> Result<(), TypeError> {
    for (name, e) in expected {
        if let Some(a) = actual.get(name) {
            check(ctx, e, a)?;
        }
    }
    Ok(())
}

fn constructors(
    ctx: &mut InferCtx,
    name: &str,
    expected: &[Type],
    actual: &[Type],
) -> Result<(), TypeError> {
    let variances = declared_variances(ctx, name);
    for (index, (e, a)) in expected.iter().zip(actual).enumerate() {
        // Result's error type is phantom: no value has this representation.
        if name != names::RESULT || index == 0 {
            let variance = variances.get(index).copied().unwrap_or(Variance::Invariant);
            variant(ctx, e, a, variance)?;
        }
    }
    Ok(())
}

fn declared_variances(ctx: &InferCtx, name: &str) -> Vec<Variance> {
    ctx.variance_of(name)
        .map(<[Variance]>::to_vec)
        .unwrap_or_default()
}

fn variant(
    ctx: &mut InferCtx,
    expected: &Type,
    actual: &Type,
    variance: Variance,
) -> Result<(), TypeError> {
    match variance {
        Variance::Covariant => check(ctx, expected, actual),
        Variance::Contravariant => check(ctx, actual, expected),
        Variance::Invariant => {
            check(ctx, expected, actual)?;
            check(ctx, actual, expected)
        }
    }
}

fn recovery(expected: &Type, actual: &Type) -> Option<TypeError> {
    let recovering = actual.is_named(names::ANY)
        && !expected.is_named(names::ANY)
        && !matches!(expected, Type::Var(_));
    recovering.then(|| {
        TypeError::new(format!(
            "cannot recover `{expected}` from an erased `any`: match its structure instead"
        ))
    })
}

fn anonymous(expected: &Type, actual: &Type) -> Option<TypeError> {
    let erasing = expected.is_named(names::ANY)
        && matches!(actual, Type::Record { name, .. } if name.is_empty());
    erasing.then(|| {
        TypeError::new(
            "cannot erase an anonymous record into `any`: declare its row as a named type first",
        )
    })
}
