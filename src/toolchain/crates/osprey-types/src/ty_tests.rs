use super::*;

#[test]
fn renders_primitives_and_generics() {
    assert_eq!(Type::int().to_string(), "int");
    assert_eq!(Type::list(Type::string()).to_string(), "List<string>");
    assert_eq!(
        Type::result(Type::int(), Type::prim("string")).to_string(),
        "Result<int, string>"
    );
    assert_eq!(
        Type::fun(vec![Type::int(), Type::int()], Type::bool()).to_string(),
        "(int, int) -> bool"
    );
    assert_eq!(Type::Var(3).to_string(), "t3");
}

#[test]
fn the_reader_rendering_gives_a_declared_record_its_name_and_its_row() {
    // A DECLARED record's name is inferred and carried (`infer_constructor`
    // builds `Record { name: owner, .. }`), but rendering dropped it, so a
    // function returning `Point` was reported as returning
    // `{ x: int, y: int }` — a spelling the author never wrote and, for a
    // record declared with `type`, not even a valid annotation.
    //
    // The name belongs in the READER rendering only: `Display` stays
    // structural because `check.rs` keys effect-row sites off it, and a
    // name-only key collapses `Box<int>` with `Box<string>`.
    let point = Type::Record {
        name: "Point".into(),
        fields: [
            ("x".to_string(), Type::int()),
            ("y".to_string(), Type::int()),
        ]
        .into_iter()
        .collect(),
    };
    assert_eq!(
        render_with_holes(&point),
        "Point { x: int, y: int }",
        "the reader gets the author's NAME and the row, because a record \
             carries no type arguments to put back and the row is where an \
             instantiation survives (#214)"
    );
    assert_eq!(
        point.to_string(),
        "{ x: int, y: int }",
        "Display stays faithful so it is safe to key off"
    );
    // An anonymous literal's row has no name to render, so it stays
    // structural in BOTH renderings — that spelling is the only
    // description it has.
    let anonymous = Type::Record {
        name: String::new(),
        fields: [("x".to_string(), Type::int())].into_iter().collect(),
    };
    assert_eq!(anonymous.to_string(), "{ x: int }");
    assert_eq!(render_with_holes(&anonymous), "{ x: int }");
}

#[test]
fn a_partially_resolved_type_renders_its_proven_part_with_holes_for_the_rest() {
    // `fn bothArms(f) = if f { Success { value: 1 } } else { Error { .. } }`
    // infers `Result<int, e0>`: the payload is PROVEN, the error side is
    // free because `Error { message }` unifies with whichever error type
    // the call site supplies. Tooling had only two moves — render `t6`, an
    // unstable private name, or fall back to `Unit` — and it chose `Unit`,
    // a positive claim the checker itself refutes with
    // "cannot unify Unit with Result<t5, t6>". A hole says exactly what is
    // known and no more. Implements [TYPE-RENDER-HOLES].
    let partial = Type::Con {
        name: "Result".into(),
        args: vec![Type::int(), Type::Var(6)],
    };
    assert_eq!(render_with_holes(&partial), "Result<int, _>");
    // A fully resolved type is untouched — holes appear only where the
    // checker genuinely proved nothing.
    assert_eq!(render_with_holes(&Type::int()), "int");
    // Holes reach every nested position Display can walk.
    let nested = Type::Fun {
        params: vec![Type::Var(1)],
        ret: Box::new(Type::Con {
            name: "List".into(),
            args: vec![Type::Var(2)],
        }),
    };
    assert_eq!(render_with_holes(&nested), "(_) -> List<_>");
}

#[test]
fn both_renderings_keep_two_instantiations_of_one_record_apart() {
    // `check.rs` keys effect-row perform/handler sites by rendering each
    // argument type with `Display` (check.rs:1011/1027/1050). Rendering a
    // nominal record as its bare NAME there made `Box<int>` and
    // `Box<string>` the same key, and a `Stash<Box<string>>` handler
    // discharged a `Stash<Box<int>>` perform with zero errors.
    //
    // So `Display` is the FAITHFUL rendering — it must distinguish whatever
    // the checker distinguishes — and the friendlier nominal spelling lives
    // in the reader path alone ([`render_with_holes`]). Anything that keys
    // off `Display` by accident is then still correct.
    let boxed = |inner: Type| Type::Record {
        name: "Box".into(),
        fields: [("value".to_string(), inner)].into_iter().collect(),
    };
    assert_ne!(
        boxed(Type::int()).to_string(),
        boxed(Type::string()).to_string(),
        "Display is an identity: two instantiations must not collide"
    );
    // The reader path keeps them apart too, and must: naming the record
    // alone was tried, and it lost the very instantiation `Display` is
    // being kept faithful to preserve.
    assert_eq!(render_with_holes(&boxed(Type::int())), "Box { value: int }");
    assert_eq!(
        render_with_holes(&boxed(Type::string())),
        "Box { value: string }"
    );
}

#[test]
fn a_hole_is_a_display_spelling_and_not_a_parseable_type() {
    // [`HOLE`] is deliberately NOT wildcard syntax: the parser has no
    // wildcard, so `_` arrives here as an ordinary nominal type name and
    // unifies with nothing. Rendering must never be mistaken for producing
    // an annotation a reader can paste back, and if a wildcard is ever
    // added to the grammar this assertion is what says so.
    let hole = Type::prim(HOLE);
    assert_eq!(
        hole,
        Type::Con {
            name: HOLE.into(),
            args: Vec::new()
        }
    );
    assert_ne!(
        hole,
        Type::unit(),
        "a hole is not Unit, the claim it replaced"
    );
    assert!(
        !has_type_var(&hole),
        "a hole is a rendered NAME, not a variable: the variable it stands \
             for is gone by the time it exists"
    );
}

#[test]
fn ptr_is_the_named_pointer_primitive() {
    assert!(Type::ptr().is_named(names::PTR));
    assert_eq!(Type::ptr().to_string(), names::PTR);
}

#[test]
fn has_type_var_walks_records_and_unions() {
    // A record whose only field is a variable is polymorphic.
    let rec = Type::Record {
        name: "R".into(),
        fields: [("x".to_string(), Type::Var(0))].into_iter().collect(),
    };
    assert!(has_type_var(&rec));
    // A union mentioning a variable in a variant is polymorphic; a fully
    // concrete one is not.
    let poly_union = Type::Union {
        name: "U".into(),
        variants: vec![Type::int(), Type::Var(1)],
    };
    let mono_union = Type::Union {
        name: "U".into(),
        variants: vec![Type::int(), Type::string()],
    };
    assert!(has_type_var(&poly_union));
    assert!(!has_type_var(&mono_union));
    // A generic constructor application is polymorphic via its args.
    assert!(has_type_var(&Type::list(Type::Var(2))));
    assert!(!has_type_var(&Type::list(Type::int())));
}
