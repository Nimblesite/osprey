//! Inference context tests.

use super::*;

#[test]
fn unification_keeps_unchanged_type_storage() {
    let mut ctx = InferCtx::new();
    let left = Type::list(Type::int());
    let right = Type::list(Type::string());
    let (seen_left, seen_right) = ctx.seen_through(&left, &right);
    for (original, seen) in [(&left, seen_left), (&right, seen_right)] {
        assert!(std::ptr::eq(original, std::borrow::Borrow::borrow(&seen)));
    }
}

#[test]
fn fresh_vars_are_distinct_and_unbound() {
    let mut c = InferCtx::new();
    let a = c.fresh();
    let integer = c.fresh_literal(Numeric::INT);
    let b = c.fresh();
    let float = c.fresh_literal(Numeric::FLOAT);
    let trailing = c.fresh();
    assert_ne!(a, b);
    c.settle_all_literals();
    assert_eq!(c.prune(&a), a);
    assert_eq!(c.prune(&b), b);
    assert_eq!(c.prune(&trailing), trailing);
    assert_eq!(c.prune(&integer), Type::int());
    assert_eq!(c.prune(&float), Type::prim(Numeric::FLOAT.name()));
    assert_eq!(c.bound_count(), 2);
    c.settle_all_literals();
    assert_eq!(c.bound_count(), 2);
}

#[test]
fn prune_follows_and_compresses_chains() -> Result<(), &'static str> {
    let mut c = InferCtx::new();
    let (Type::Var(a), Type::Var(b), Type::Var(root)) = (c.fresh(), c.fresh(), c.fresh()) else {
        return Err("fresh did not return inference variables");
    };
    let payload = rec(uni(Type::list(Type::int())));
    c.bind(a, Type::Var(b));
    c.bind(b, Type::Var(root));
    assert_eq!(c.prune(&Type::Var(a)), Type::Var(root));
    assert_eq!(c.bound_count(), 2);
    c.bind(root, payload.clone());
    assert_eq!(c.prune(&Type::Var(a)), payload);
    assert_eq!(c.prune(&Type::Var(b)), payload);
    assert_eq!(c.binding(a), Some(&Type::Var(root)));
    assert_eq!(c.binding(b), Some(&Type::Var(root)));
    assert_eq!(c.bound_count(), 3);
    Ok(())
}

#[test]
fn occurs_check_detects_cycles() {
    let mut c = InferCtx::new();
    let a = c.fresh();
    let id = if let Type::Var(id) = a { id } else { 0 };
    assert!(c.occurs(id, &Type::list(a.clone())));
    assert!(!c.occurs(id, &Type::list(Type::int())));
}

fn rec(field: Type) -> Type {
    Type::Record {
        name: "R".into(),
        fields: [("x".to_string(), field)].into_iter().collect(),
    }
}
fn uni(variant: Type) -> Type {
    Type::Union {
        name: "U".into(),
        variants: vec![variant],
    }
}

#[test]
fn occurs_walks_records_unions_and_functions() {
    let mut c = InferCtx::new();
    let v = c.fresh();
    let id = if let Type::Var(id) = v { id } else { 0 };
    assert!(c.occurs(id, &rec(v.clone())));
    assert!(c.occurs(id, &uni(v.clone())));
    // The variable in a function's parameter / return is found too.
    assert!(c.occurs(id, &Type::fun(vec![v.clone()], Type::int())));
    assert!(c.occurs(id, &Type::fun(vec![Type::int()], v.clone())));
}

#[test]
fn apply_and_free_vars_descend_records_and_unions() {
    let mut c = InferCtx::new();
    let v = c.fresh();
    let id = if let Type::Var(id) = v { id } else { 0 };
    let literal = c.fresh_literal(Numeric::INT);
    c.bind(id, literal);
    let w = c.fresh();
    let wid = if let Type::Var(wid) = w { wid } else { 0 };
    let mixed = rec(uni(Type::fun(vec![v.clone()], w.clone())));
    assert_eq!(c.free_vars_after_defaulting(&mixed), [wid].into());
    let ctx = &c;
    // `apply` rebuilds records/unions with the binding resolved.
    assert_eq!(ctx.apply(&rec(v.clone())), rec(Type::int()));
    assert_eq!(ctx.apply(&uni(v.clone())), uni(Type::int()));
    // `free_vars` reaches into record fields and union variants.
    let mut fv = BTreeSet::new();
    ctx.free_vars(&rec(w.clone()), &mut fv);
    ctx.free_vars(&uni(w.clone()), &mut fv);
    assert!(fv.contains(&wid));
}
