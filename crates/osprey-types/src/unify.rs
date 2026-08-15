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

/// Unify two types, recording the solution in `ctx`. Errors are structural; a
/// failing call may have applied partial bindings, so callers that want to
/// "try" a unification should pre-check shapes rather than relying on rollback.
pub fn unify(ctx: &mut InferCtx, a: &Type, b: &Type) -> Result<(), TypeError> {
    let a = ctx.prune(a);
    let b = ctx.prune(b);
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

        _ => Err(TypeError::mismatch(&a, &b)),
    }
}

/// Directional assignment-site unification: `actual` must be usable where
/// `expected` is demanded. Plain [`unify`] stays symmetric — this is the only
/// place a one-way rule may live.
pub fn unify_assignable(
    ctx: &mut InferCtx,
    expected: &Type,
    actual: &Type,
) -> Result<(), TypeError> {
    let expected = ctx.prune(expected);
    let actual = ctx.prune(actual);
    // Erasure is ONE-WAY [TYPE-ANY]. Every value assigns into an `any` slot;
    // no annotation recovers one back out. Recovery by declared type was an
    // unchecked cast — it printed a heap address as a decimal integer (#209)
    // and segfaulted on a word that never was a pointer — so it is rejected
    // here rather than repaired in the backend.
    if let Some(e) = erasure_is_one_way(&expected, &actual) {
        return Err(e);
    }
    if let Some(e) = anonymous_erasure(&expected, &actual) {
        return Err(e);
    }
    // A bare `T` value satisfies a `Result<T, E>` slot (implicit
    // `Success`), e.g. `fn f() -> Result<bool, E> = x > 0`.
    if let Type::Con { name, args } = &expected {
        if name == names::RESULT
            && !matches!(actual, Type::Var(_))
            && !actual.is_named(names::RESULT)
        {
            if let Some(inner) = args.first() {
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

/// The error for recovering a concrete type out of an erased `any`, or `None`
/// when this assignment is not a recovery. An `expected` that is still a
/// variable is inference, not an annotation, so it takes the erasure and stays
/// `any`. Implements [TYPE-ANY].
fn erasure_is_one_way(expected: &Type, actual: &Type) -> Option<TypeError> {
    let recovering = actual.is_named(names::ANY)
        && !expected.is_named(names::ANY)
        && !matches!(expected, Type::Var(_));
    recovering.then(|| {
        TypeError::new(format!(
            "cannot recover `{expected}` from an erased `any`: \
             match its structure instead"
        ))
    })
}

/// The error for erasing an anonymous record into `any`, or `None` otherwise.
/// Narrowing selects among DECLARED row shapes by descriptor identity, so a
/// shape only a literal spells could never be selected by any arm —
/// accepted-then-unmatchable would be a silent wrong answer, not a
/// capability. Implements [TYPE-ANY], [TYPE-RECORD-ANON].
fn anonymous_erasure(expected: &Type, actual: &Type) -> Option<TypeError> {
    let erasing = expected.is_named(names::ANY)
        && matches!(actual, Type::Record { name, .. } if name.is_empty());
    erasing.then(|| {
        TypeError::new(
            "cannot erase an anonymous record into `any`: declare its row as a named type first",
        )
    })
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
    Err(TypeError::mismatch(a, b))
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
mod tests {
    use super::*;

    #[test]
    fn unifies_var_with_concrete() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        unify(&mut c, &v, &Type::int()).unwrap();
        assert_eq!(c.apply(&v), Type::int());
    }

    #[test]
    fn rejects_distinct_primitives() {
        let mut c = InferCtx::new();
        assert!(unify(&mut c, &Type::int(), &Type::string()).is_err());
    }

    #[test]
    fn any_unifies_with_anything() {
        let mut c = InferCtx::new();
        unify(&mut c, &Type::any(), &Type::int()).unwrap();
        unify(&mut c, &Type::list(Type::string()), &Type::any()).unwrap();
    }

    #[test]
    fn bare_collection_unifies_with_parameterized() {
        let mut c = InferCtx::new();
        unify(
            &mut c,
            &Type::prim("Fiber"),
            &Type::con("Fiber", vec![Type::int()]),
        )
        .unwrap();
    }

    #[test]
    fn result_is_not_assignable_to_its_payload() {
        let mut c = InferCtx::new();
        let r = Type::result(Type::int(), Type::prim("MathError"));
        assert!(unify_assignable(&mut c, &Type::int(), &r).is_err());
        assert!(unify(&mut c, &Type::int(), &r).is_err());
    }

    #[test]
    fn structural_records_ignore_field_order() {
        use std::collections::BTreeMap;
        let mut c = InferCtx::new();
        let mut f1 = BTreeMap::new();
        f1.insert("x".to_string(), Type::int());
        f1.insert("y".to_string(), Type::int());
        let r1 = Type::Record {
            name: "A".into(),
            fields: f1.clone(),
        };
        let r2 = Type::Record {
            name: "B".into(),
            fields: f1,
        };
        unify(&mut c, &r1, &r2).unwrap();
    }

    #[test]
    fn occurs_check_blocks_infinite_type() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        assert!(unify(&mut c, &v, &Type::list(v.clone())).is_err());
    }

    #[test]
    fn function_types_unify_on_arity_params_and_return() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        // (int) -> v  ~  (int) -> string  binds v := string.
        unify(
            &mut c,
            &Type::fun(vec![Type::int()], v.clone()),
            &Type::fun(vec![Type::int()], Type::string()),
        )
        .unwrap();
        assert_eq!(c.apply(&v), Type::string());
        // Arity mismatch is an error.
        let e = unify(
            &mut c,
            &Type::fun(vec![Type::int()], Type::int()),
            &Type::fun(vec![Type::int(), Type::int()], Type::int()),
        )
        .unwrap_err();
        assert!(format!("{e:?}").contains("arity"));
    }

    #[test]
    fn assignable_wraps_bare_value_into_result_return() {
        let mut c = InferCtx::new();
        // A bare `bool` satisfies a `Result<bool, E>` slot (implicit Success).
        let want = Type::result(Type::bool(), Type::prim("E"));
        unify_assignable(&mut c, &want, &Type::bool()).unwrap();
    }

    #[test]
    fn assignable_function_return_cannot_erase_result() {
        let mut c = InferCtx::new();
        let slot = Type::fun(vec![Type::int()], Type::int());
        let lambda = Type::fun(
            vec![Type::int()],
            Type::result(Type::int(), Type::prim("MathError")),
        );
        assert!(unify_assignable(&mut c, &slot, &lambda).is_err());
    }

    #[test]
    fn record_mismatches_are_rejected() {
        use std::collections::BTreeMap;
        let mut c = InferCtx::new();
        let rec = |pairs: &[(&str, Type)]| Type::Record {
            name: "R".into(),
            fields: pairs
                .iter()
                .map(|(k, t)| ((*k).to_string(), t.clone()))
                .collect::<BTreeMap<_, _>>(),
        };
        // Same arity, different field name.
        assert!(unify(
            &mut c,
            &rec(&[("x", Type::int())]),
            &rec(&[("y", Type::int())])
        )
        .is_err());
        // Different number of fields.
        assert!(unify(
            &mut c,
            &rec(&[("x", Type::int())]),
            &rec(&[("x", Type::int()), ("y", Type::int())]),
        )
        .is_err());
    }

    #[test]
    fn nominal_constructor_unifies_with_same_named_record() {
        use std::collections::BTreeMap;
        let mut c = InferCtx::new();
        let point_con = Type::con("Point", vec![]);
        let point_rec = Type::Record {
            name: "Point".into(),
            fields: BTreeMap::new(),
        };
        unify(&mut c, &point_con, &point_rec).unwrap();
        unify(&mut c, &point_rec, &point_con).unwrap();
        // Distinct constructor names still clash.
        assert!(unify(
            &mut c,
            &Type::con("List", vec![Type::int()]),
            &Type::con("Map", vec![Type::int(), Type::int()])
        )
        .is_err());
    }

    #[test]
    fn binding_a_var_to_itself_is_a_noop() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        // `t ~ t` short-circuits even after `v` aliases another fresh var.
        let w = c.fresh();
        unify(&mut c, &v, &w).unwrap();
        unify(&mut c, &v, &w).unwrap();
        assert_eq!(c.apply(&v), c.apply(&w));
    }

    #[test]
    fn assignable_functions_reject_result_erasure_but_allow_success_wrapping() {
        let mut c = InferCtx::new();
        let slot = Type::fun(vec![Type::int()], Type::int());
        let value = Type::fun(
            vec![Type::int()],
            Type::result(Type::int(), Type::prim("MathError")),
        );
        assert!(unify_assignable(&mut c, &slot, &value).is_err());
        // The safe direction remains: a bare return is implicitly Success.
        let result_slot = Type::fun(
            vec![Type::int()],
            Type::result(Type::int(), Type::prim("E")),
        );
        let bare = Type::fun(vec![Type::int()], Type::int());
        unify_assignable(&mut c, &result_slot, &bare).unwrap();
    }

    #[test]
    fn plain_unify_of_functions_keeps_result_returns_distinct() {
        let mut c = InferCtx::new();
        let res = |ok: Type| Type::result(ok, Type::prim("MathError"));
        assert!(unify(
            &mut c,
            &Type::fun(vec![Type::int()], Type::int()),
            &Type::fun(vec![Type::int()], res(Type::int())),
        )
        .is_err());
        assert!(unify(
            &mut c,
            &Type::fun(vec![Type::int()], res(Type::int())),
            &Type::fun(vec![Type::int()], Type::int()),
        )
        .is_err());
    }

    #[test]
    fn unify_seq_rejects_length_mismatch() {
        let mut c = InferCtx::new();
        // Same constructor name, different arity is a sequence-length mismatch.
        assert!(unify(
            &mut c,
            &Type::con("Pair", vec![Type::int(), Type::int()]),
            &Type::con("Pair", vec![Type::int()]),
        )
        .is_err());
    }

    #[test]
    fn incompatible_shapes_hit_the_final_mismatch_arm() {
        let mut c = InferCtx::new();
        // A function vs a constructor matches no structural arm: the catch-all
        // `_ => Err(mismatch)` fires.
        assert!(unify(
            &mut c,
            &Type::fun(vec![Type::int()], Type::int()),
            &Type::con("List", vec![Type::int()]),
        )
        .is_err());
    }

    #[test]
    fn assignable_rejects_a_result_value_in_a_concrete_slot() {
        let mut c = InferCtx::new();
        let r = Type::result(Type::int(), Type::prim("E"));
        assert!(unify_assignable(&mut c, &Type::int(), &r).is_err());
    }

    #[test]
    fn variance_argument_matching_is_exact_at_the_leaves() {
        // Implements [TYPE-VARIANCE-ASSIGN]: the coercive Result unwrap NEVER
        // applies under a container — it is a representation-changing
        // coercion codegen emits only at direct value sites — so a
        // Result-payload instantiation is rejected under EVERY variance.
        let mut c = InferCtx::new();
        c.set_variance("Feed", vec![Variance::Covariant]);
        c.set_variance("Gate", vec![Variance::Contravariant]);
        let feed = |t: Type| Type::con("Feed", vec![t]);
        let gate = |t: Type| Type::con("Gate", vec![t]);
        let res = Type::result(Type::int(), Type::prim("MathError"));
        assert!(unify_assignable(&mut c, &feed(Type::int()), &feed(res.clone())).is_err());
        assert!(unify_assignable(&mut c, &gate(res.clone()), &gate(Type::int())).is_err());
        // Function returns likewise remain exact beneath containers.
        let fnres = Type::fun(vec![Type::int()], res.clone());
        let fnint = Type::fun(vec![Type::int()], Type::int());
        assert!(unify_assignable(&mut c, &feed(fnint.clone()), &feed(fnres.clone())).is_err());
        // Directional recursion continues through nested variance-declared
        // constructors and still bottoms out exactly.
        assert!(unify_assignable(&mut c, &feed(feed(fnint)), &feed(feed(fnres))).is_err());
        assert!(unify_assignable(&mut c, &feed(feed(Type::int())), &feed(feed(res))).is_err());
    }

    #[test]
    fn unions_unify_by_name_and_variants() {
        let mut c = InferCtx::new();
        let u = |name: &str, vs: Vec<Type>| Type::Union {
            name: name.into(),
            variants: vs,
        };
        unify(
            &mut c,
            &u("E", vec![Type::int()]),
            &u("E", vec![Type::int()]),
        )
        .unwrap();
        // Different name.
        assert!(unify(
            &mut c,
            &u("E", vec![Type::int()]),
            &u("F", vec![Type::int()])
        )
        .is_err());
        // Different variant count.
        assert!(unify(
            &mut c,
            &u("E", vec![Type::int()]),
            &u("E", vec![Type::int(), Type::bool()])
        )
        .is_err());
    }
}
