//! Tests for the typing environment.
use super::*;
use crate::unify::unify;

std::thread_local! {
    pub(super) static SCHEME_SCRATCH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static FLATTEN_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ENVIRONMENT_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(crate) static MARKER_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) const ENVIRONMENT_MARKER: &str = "EnvScanMarker";

pub(super) fn record_environment_visit(scheme: &Scheme) {
    ENVIRONMENT_VISITS.with(|visits| visits.set(visits.get() + 1));
    if matches!(&scheme.ty, Type::Record { name, .. } | Type::Con { name, .. }
        if name == ENVIRONMENT_MARKER)
    {
        MARKER_VISITS.with(|visits| visits.set(visits.get() + 1));
    }
}

#[test]
fn closed_types_generalize_without_scanning_unrelated_bindings() {
    let mut ctx = InferCtx::new();
    let mut env = TypeEnv::new();
    for index in 0..512 {
        env.insert(format!("global{index}"), Scheme::mono(Type::int()));
    }
    let ty = Type::fun(vec![Type::int()], Type::string());
    ENVIRONMENT_VISITS.with(|visits| visits.set(0));
    let (scheme, external) = generalize(&mut ctx, &env, &ty);
    assert_eq!(scheme.ty, ty);
    assert_eq!(scheme.vars, Vec::new());
    assert_eq!(scheme.obligations, Vec::new());
    assert!(external.is_empty());
    let visits = ENVIRONMENT_VISITS.with(std::cell::Cell::get);
    assert_eq!(
        visits, 0,
        "a closed type cannot quantify variables from unrelated bindings"
    );
}

#[test]
fn instantiation_gives_fresh_independent_vars() {
    // id : forall t0. (t0) -> t0
    let mut ctx = InferCtx::new();
    let scheme = Scheme::poly(vec![0], Type::fun(vec![Type::Var(0)], Type::Var(0)));

    // Two uses must not share a variable: id(1) and id("x") both type-check.
    let a = instantiate(&mut ctx, &scheme);
    let b = instantiate(&mut ctx, &scheme);
    if let (Type::Fun { params: pa, .. }, Type::Fun { params: pb, .. }) = (&a, &b) {
        unify(&mut ctx, &pa[0], &Type::int()).unwrap();
        unify(&mut ctx, &pb[0], &Type::string()).unwrap();
    } else {
        panic!("expected function types");
    }
}

#[test]
fn instantiate_substitutes_into_records_and_unions() {
    let mut ctx = InferCtx::new();
    // forall t0. Union "U" { Record "R" { x: t0 }, t0 }
    let body = Type::Union {
        name: "U".into(),
        variants: vec![
            Type::Record {
                name: "R".into(),
                fields: [("x".to_string(), Type::Var(0))].into_iter().collect(),
            },
            Type::Var(0),
        ],
    };
    let inst = instantiate(&mut ctx, &Scheme::poly(vec![0], body));
    // The single quantified var is renamed to one consistent fresh var that
    // appears in both the record field and the bare variant.
    if let Type::Union { variants, .. } = &inst {
        assert!(matches!(variants[1], Type::Var(_)));
        if let Type::Record { fields, .. } = &variants[0] {
            assert_eq!(fields["x"], variants[1]);
        } else {
            panic!("expected record variant");
        }
    } else {
        panic!("expected union");
    }
}

#[test]
fn a_child_scope_neither_leaks_nor_sees_later_parent_bindings() {
    let mut parent = TypeEnv::new();
    parent.insert_mutable("count", Scheme::mono(Type::int()));
    parent.declare_type_params("count", vec![Type::Var(7)]);
    parent.mark_runtime_builtin("count");
    let mut child = parent.child();
    child.insert("count", Scheme::mono(Type::string()));
    child.insert("local", Scheme::mono(Type::bool()));
    parent.insert("later", Scheme::mono(Type::int()));
    assert_eq!(
        child.get("count").map(|s| s.ty.clone()),
        Some(Type::string())
    );
    assert!(!child.is_mutable("count") && !child.is_runtime_builtin("count"));
    assert!(child.type_params("count").is_none() && child.get("later").is_none());
    assert_eq!(parent.get("count").map(|s| s.ty.clone()), Some(Type::int()));
    assert!(parent.is_mutable("count") && parent.is_runtime_builtin("count"));
    assert!(parent.type_params("count").is_some() && parent.get("local").is_none());
}

#[test]
fn removing_or_shadowing_hides_an_outer_binding_from_every_query() {
    let mut ctx = InferCtx::new();
    let (parent, mut child) = scoped(&[("pinned", 0), ("gone", 1)]);
    child.insert("pinned", Scheme::mono(Type::int()));
    child.remove("gone");
    assert!(child.get("gone").is_none() && parent.get("gone").is_some());
    assert_eq!(
        child.bound_names(),
        ["pinned".to_owned()].into_iter().collect()
    );
    assert!(child.free_vars(&mut ctx).is_empty());
    assert_eq!(parent.free_vars(&mut ctx), [0, 1].into_iter().collect());
}

#[test]
fn deep_nesting_flattens_without_changing_what_a_scope_sees() {
    let mut env = TypeEnv::new();
    env.insert("base", Scheme::mono(Type::int()));
    let snapshot = env.clone();
    for depth in 0..(MAX_DEPTH * 2) {
        env = env.child();
        env.insert(format!("v{depth}"), Scheme::mono(Type::bool()));
    }
    assert!(env.depth <= MAX_DEPTH + 1);
    assert!(env.get("base").is_some() && env.get("v0").is_some());
    assert_eq!(env.bound_names().len(), MAX_DEPTH * 2 + 1);
    assert!(
        matches!((snapshot.get("base"), env.get("base")), (Some(before), Some(after))
            if std::ptr::eq(before, after)),
        "flattening must share immutable schemes instead of cloning their full type trees"
    );
}

#[test]
fn function_body_scopes_do_not_force_their_parent_to_flatten() -> Result<(), std::fmt::Error> {
    use std::fmt::Write;
    let mut source = String::new();
    for index in 0..256 {
        writeln!(source, "fn family{index}(x) = x")?;
    }
    source.push_str("print(family0(1))\nprint(family255(true))\n");
    let parsed = osprey_syntax::parse_program(&source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    FLATTEN_PASSES.with(|count| count.set(0));
    let errors = crate::check_program(&parsed.program);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(FLATTEN_PASSES.with(std::cell::Cell::get), 0);
    Ok(())
}

#[test]
fn a_scope_written_under_many_snapshots_stays_shallow() {
    let mut env = TypeEnv::new();
    let mut snapshots = Vec::new();
    for index in 0..(MAX_DEPTH * 3) {
        snapshots.push(env.clone());
        env.insert(format!("v{index}"), Scheme::mono(Type::int()));
    }
    assert!(env.depth <= MAX_DEPTH && env.child().depth <= MAX_DEPTH + 1);
    assert_eq!(env.bound_names().len(), MAX_DEPTH * 3);
    assert!(snapshots
        .iter()
        .enumerate()
        .all(|(i, s)| s.bound_names().len() == i));
}

#[test]
fn generalize_skips_env_bound_vars() {
    let mut ctx = InferCtx::new();
    // An outer binding pins t0, so a let over (t0 -> t1) may only quantify t1.
    let env = open_env(&[("outer", 0)]);
    let (scheme, external) =
        generalize(&mut ctx, &env, &Type::fun(vec![Type::Var(0)], Type::Var(1)));
    assert_eq!(scheme.vars, vec![1]);
    assert_eq!(external, BTreeSet::from([0]));
}

/// A scheme whose body has no variable free outside its own quantifiers — it
/// cannot name any variable the environment could constrain.
fn closed_scheme() -> Scheme {
    Scheme::mono(Type::fun(vec![Type::int()], Type::string()))
}

/// The same, polymorphic: `forall t0. t0 -> t0` is closed even though its body
/// is nothing but a variable.
fn closed_poly_scheme() -> Scheme {
    Scheme::poly(vec![0], Type::fun(vec![Type::Var(0)], Type::Var(0)))
}

fn environment_visits() -> usize {
    ENVIRONMENT_VISITS.with(std::cell::Cell::get)
}

/// An environment holding one monomorphic open scheme per `(name, free
/// variable)` pair — the starting shape every shadowing test asserts over.
fn open_env(bindings: &[(&str, VarId)]) -> TypeEnv {
    let mut env = TypeEnv::new();
    for &(name, var) in bindings {
        env.insert(name, Scheme::mono(Type::Var(var)));
    }
    env
}

/// `open_env(bindings)` and the child scope derived from it.
fn scoped(bindings: &[(&str, VarId)]) -> (TypeEnv, TypeEnv) {
    let parent = open_env(bindings);
    let child = parent.child();
    (parent, child)
}

#[test]
fn free_vars_skips_every_closed_binding_in_a_deep_environment() {
    let mut ctx = InferCtx::new();
    let mut env = TypeEnv::new();
    for index in 0..512 {
        let scheme = if index % 2 == 0 {
            closed_scheme()
        } else {
            closed_poly_scheme()
        };
        env.insert(format!("global{index}"), scheme);
    }
    for depth in 0..(MAX_DEPTH * 3) {
        env = env.child();
        env.insert(format!("v{depth}"), closed_scheme());
        env.insert_mutable(format!("m{depth}"), closed_poly_scheme());
    }
    ENVIRONMENT_VISITS.with(|visits| visits.set(0));
    assert!(env.free_vars(&mut ctx).is_empty());
    assert_eq!(
        environment_visits(),
        0,
        "a closed scheme can never contribute a free variable, whatever the \
         environment it sits in, so free_vars must not walk it"
    );
}

#[test]
fn a_visible_open_binding_still_reports_its_free_variables() {
    let mut ctx = InferCtx::new();
    let mut env = TypeEnv::new();
    for index in 0..64 {
        env.insert(format!("global{index}"), closed_scheme());
    }
    env.insert("open", Scheme::mono(Type::Var(0)));
    env.insert("pending", closed_poly_scheme());
    for depth in 0..8 {
        env = env.child();
        env.insert(format!("v{depth}"), closed_scheme());
    }
    assert_eq!(env.free_vars(&mut ctx), [0].into_iter().collect());
}

#[test]
fn a_closed_binding_hides_the_outer_open_one_it_shadows() {
    let mut ctx = InferCtx::new();
    let (parent, mut child) = scoped(&[("shadowed", 0), ("shared", 1), ("visible", 2)]);
    child.insert("shadowed", closed_scheme());
    child.insert("shared", closed_poly_scheme());
    // A third level keeps a closed name of its own that hides nothing.
    let mut grandchild = child.child();
    grandchild.insert("local", closed_scheme());
    assert_eq!(grandchild.free_vars(&mut ctx), [2].into_iter().collect());
    assert_eq!(child.free_vars(&mut ctx), [2].into_iter().collect());
    assert_eq!(parent.free_vars(&mut ctx), [0, 1, 2].into_iter().collect());
    assert!(grandchild.shares_open_vars(&ctx, &[0, 1].into_iter().collect()));
    assert!(!grandchild.shares_open_vars(&ctx, &[3].into_iter().collect()));
}

#[test]
fn monomorphic_bindings_collect_free_variables_without_per_binding_scratch() {
    let mut ctx = InferCtx::new();
    let mut env = TypeEnv::new();
    for index in 0..128 {
        env.insert(format!("open{index}"), Scheme::mono(Type::Var(index)));
    }
    env.insert(
        "mixed",
        Scheme {
            vars: vec![129],
            ty: Type::fun(vec![Type::Var(128)], Type::Var(129)),
            obligations: vec![("constraint".into(), Type::Var(130))],
        },
    );
    SCHEME_SCRATCH.with(|count| count.set(0));
    assert_eq!(env.free_vars(&mut ctx), (0..=128).chain([130]).collect());
    assert_eq!(SCHEME_SCRATCH.with(std::cell::Cell::get), 1);
}

#[test]
fn a_shadowing_open_binding_reports_only_the_innermost_variables() {
    let mut ctx = InferCtx::new();
    let (_, mut child) = scoped(&[("x", 0)]);
    child.insert("x", Scheme::mono(Type::Var(7)));
    let mut grandchild = child.child();
    grandchild.insert("y", Scheme::mono(Type::Var(9)));
    assert_eq!(grandchild.free_vars(&mut ctx), [7, 9].into_iter().collect());
    ENVIRONMENT_VISITS.with(|visits| visits.set(0));
    assert_eq!(grandchild.free_vars(&mut ctx), [7, 9].into_iter().collect());
    assert_eq!(
        environment_visits(),
        2,
        "only the two visible open schemes are walked, once each"
    );
}

#[test]
fn a_removed_name_still_hides_the_outer_open_binding() {
    let mut ctx = InferCtx::new();
    let (_, mut child) = scoped(&[("gone", 0)]);
    child.remove("gone");
    assert_eq!(child.free_vars(&mut ctx), BTreeSet::new());
}

#[test]
fn flattening_keeps_a_shadowed_or_removed_outer_binding_hidden() {
    let mut ctx = InferCtx::new();
    let mut root = TypeEnv::new();
    for index in 0..(MAX_DEPTH * 8) {
        root.insert(format!("global{index}"), Scheme::mono(Type::int()));
    }
    // A local scope, so flattening merges it with the frame that shadows it.
    let mut parent = root.child();
    for (name, var) in [("closed_over", 0), ("rebound", 1), ("gone", 2), ("seen", 3)] {
        parent.insert(name, Scheme::mono(Type::Var(var)));
    }
    let mut env = parent.child();
    env.insert("closed_over", Scheme::mono(Type::int()));
    env.insert("rebound", Scheme::mono(Type::Var(4)));
    env.remove("gone");
    for depth in 0..(MAX_DEPTH * 3) {
        env = env.child();
        let var = VarId::try_from(depth % 2).unwrap_or_default() + 5;
        env.insert(format!("v{depth}"), Scheme::mono(Type::Var(var)));
        assert_eq!(
            env.free_vars(&mut ctx)
                .intersection(&[0, 1, 2].into_iter().collect())
                .count(),
            0,
            "depth {depth} exposed a hidden outer binding"
        );
        assert!(env
            .free_vars(&mut ctx)
            .is_superset(&[3, 4].into_iter().collect()));
    }
    assert!(env.get("gone").is_none());
    assert!(matches!(env.get("closed_over"), Some(scheme) if scheme.ty == Type::int()));
}

#[test]
fn open_scheme_scans_resolve_unique_roots_and_follow_later_constraints() {
    let mut ctx = InferCtx::new();
    let mut env = TypeEnv::new();
    let root = ctx.fresh();
    let later = ctx.fresh();
    let quantified = ctx.fresh();
    let Type::Var(quantified_id) = quantified else {
        panic!("fresh variable")
    };
    env.insert(
        "wide",
        Scheme {
            vars: vec![quantified_id],
            ty: Type::fun(
                vec![Type::fun(vec![Type::int(), root.clone()], quantified.clone()); 512],
                root.clone(),
            ),
            obligations: vec![(
                "constraint".into(),
                Type::Record {
                    name: "Pending".into(),
                    fields: [(
                        "choice".into(),
                        Type::Union {
                            name: "Choice".into(),
                            variants: vec![
                                Type::con("Pending", vec![later.clone(), root.clone()]),
                                quantified.clone(),
                            ],
                        },
                    )]
                    .into_iter()
                    .collect(),
                },
            )],
        },
    );
    let mut child = env.child();
    crate::ctx::FREE_VAR_VISITS.with(|count| count.set(0));
    assert_eq!(child.free_vars(&mut ctx), [0, 1].into_iter().collect());
    assert!(
        crate::ctx::FREE_VAR_VISITS.with(std::cell::Cell::get) <= 2,
        "an immutable wide scheme must resolve each distinct unquantified root once"
    );
    unify(&mut ctx, &root, &Type::con("Expanded", vec![later.clone()])).unwrap();
    assert_eq!(child.free_vars(&mut ctx), [1].into_iter().collect());
    unify(&mut ctx, &later, &Type::int()).unwrap();
    assert!(child.free_vars(&mut ctx).is_empty());
    assert!(!child.shares_open_vars(&ctx, &[0, 1, quantified_id].into_iter().collect()));
    child.remove("wide");
    assert!(child.free_vars(&mut ctx).is_empty());
    assert!(env.get("wide").is_some());
}
