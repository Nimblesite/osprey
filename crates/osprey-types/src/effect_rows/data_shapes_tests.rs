//! Values whose type can never hold a function carry no callable provenance.
use super::{data_shape, data_types};
use crate::check::CtorInfo;
use crate::effect_rows::Value;
use crate::ty::Type;
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn ctor(owner: &str, record: bool, params: &[&str], fields: &[(&str, &str)]) -> CtorInfo {
    CtorInfo {
        owner: owner.to_owned(),
        owner_is_record: record,
        type_params: params.iter().map(|param| (*param).to_owned()).collect(),
        fields: fields
            .iter()
            .map(|(name, ty)| ((*name).to_owned(), (*ty).to_owned()))
            .collect(),
    }
}

fn names(fields: &[&str]) -> BTreeSet<String> {
    fields.iter().map(|field| (*field).to_owned()).collect()
}

fn named(name: &str) -> Type {
    Type::Con {
        name: name.to_owned(),
        args: Vec::new(),
    }
}

fn kernel_like() -> HashMap<String, CtorInfo> {
    HashMap::from([
        (
            "Pair".into(),
            ctor("Pair", true, &[], &[("a", "int"), ("b", "string")]),
        ),
        (
            "Env".into(),
            ctor(
                "Env",
                true,
                &[],
                &[("left", "Pair"), ("decls", "List<Pair>")],
            ),
        ),
        (
            "Lam".into(),
            ctor("Term", false, &[], &[("body", "Term"), ("name", "string")]),
        ),
        ("Var".into(), ctor("Term", false, &[], &[("index", "int")])),
        (
            "Holder".into(),
            ctor("Holder", true, &[], &[("run", "fn() -> int")]),
        ),
        (
            "Wrap".into(),
            ctor("Wrap", true, &[], &[("inner", "Holder")]),
        ),
        ("Box".into(), ctor("Box", true, &["T"], &[("value", "T")])),
        (
            "Feed".into(),
            ctor("Feed", true, &[], &[("port", "Channel<int>")]),
        ),
        (
            "Loose".into(),
            ctor("Loose", true, &[], &[("anything", "any")]),
        ),
    ])
}

#[test]
fn data_types_are_the_named_types_no_value_of_which_can_be_called() {
    let data = data_types(&kernel_like());
    let expected = BTreeMap::from([
        ("Env".to_owned(), names(&["decls", "left"])),
        ("Pair".to_owned(), names(&["a", "b"])),
        ("Term".to_owned(), BTreeSet::new()),
    ]);
    assert_eq!(data.into_iter().collect::<BTreeMap<_, _>>(), expected);
}

#[test]
fn a_data_typed_value_keeps_only_its_field_names() {
    let data = data_types(&kernel_like());
    let shape = |ty: &Type| data_shape(ty, &data);
    let fields = |fields: &[&str]| {
        Some(Value {
            field_names: Some(names(fields)),
            ..Value::default()
        })
    };
    assert_eq!(shape(&named("Env")), fields(&["decls", "left"]));
    assert_eq!(shape(&named("Term")), fields(&[]));
    let list = Type::Con {
        name: "List".into(),
        args: vec![named("Pair")],
    };
    assert_eq!(shape(&list), fields(&[]));
    let record = Type::Record {
        name: "Point".into(),
        fields: BTreeMap::from([("x".into(), Type::int()), ("y".into(), Type::int())]),
    };
    assert_eq!(shape(&record), fields(&["x", "y"]));
}

#[test]
fn a_type_that_can_hold_a_function_keeps_its_provenance() {
    let data = data_types(&kernel_like());
    for name in ["Holder", "Wrap", "Box", "Feed", "Loose", "Undeclared"] {
        assert_eq!(data_shape(&named(name), &data), None, "{name}");
    }
    let callback = Type::Fun {
        params: Vec::new(),
        ret: Box::new(Type::int()),
    };
    let callbacks = Type::Con {
        name: "List".into(),
        args: vec![callback.clone()],
    };
    let holder = Type::Record {
        name: "Holder".into(),
        fields: BTreeMap::from([("run".into(), callback.clone())]),
    };
    for ty in [callback, callbacks, holder, Type::Var(0)] {
        assert_eq!(data_shape(&ty, &data), None, "{ty}");
    }
}
