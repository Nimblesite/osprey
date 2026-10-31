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
    let r = Type::result(Type::int(), Type::prim("string"));
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
        Type::result(Type::int(), Type::prim("string")),
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
        Type::result(Type::int(), Type::prim("string")),
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
    let res = |ok: Type| Type::result(ok, Type::prim("string"));
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
    let res = Type::result(Type::int(), Type::prim("string"));
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

#[test]
fn assignment_never_recovers_erased_sum_payloads() {
    let sum = |payload| Type::Union {
        name: "HeldSum".into(),
        variants: vec![Type::Record {
            name: "Held".into(),
            fields: std::collections::BTreeMap::from([("value".into(), payload)]),
        }],
    };
    let mut context = InferCtx::new();
    assert!(unify_assignable(&mut context, &sum(Type::int()), &sum(Type::any())).is_err());
}
