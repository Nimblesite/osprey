//! Resource and semantic controls for record owner projection.
use super::*;
use crate::effect_rows::{expression_site, Instances};

#[test]
fn record_owners_do_not_substitute_irrelevant_fields() {
    let (expression, index) = receiver();
    let record = crate::ty::Type::Record {
        name: "Control".to_owned(),
        fields: [("payload".to_owned(), crate::ty::Type::Var(7))]
            .into_iter()
            .collect(),
    };
    let instances = Instances {
        instantiation_types: (0..128_usize)
            .map(|site| (site, [(0, record.clone())].into_iter().collect()))
            .collect(),
        ..Instances::default()
    };
    let analyzer = analyzer(&index, &instances);
    let expected = [FieldInstance {
        name: "Control".to_owned(),
        arguments: Vec::new(),
    }]
    .into_iter()
    .collect();
    for ty in [
        crate::ty::Type::con("Control", Vec::new()),
        record,
        crate::ty::Type::Var(0),
    ] {
        instances.owner_substitutions.set(0);
        let _ = instances
            .expression_types
            .borrow_mut()
            .insert(expression_site(&expression), ty.clone());
        assert_eq!(analyzer.updated_records(&expression), expected, "{ty:?}");
        assert!(
            instances.owner_substitutions.get() <= 1,
            "{} irrelevant substitutions for {ty:?}",
            instances.owner_substitutions.get()
        );
    }
}

#[test]
fn generic_owner_projection_keeps_every_instantiated_argument() {
    let (expression, index) = receiver();
    let arguments = [crate::ty::Type::int(), crate::ty::Type::string()];
    let instances = Instances {
        instantiation_types: arguments
            .iter()
            .enumerate()
            .map(|(site, argument)| (site, [(7, argument.clone())].into_iter().collect()))
            .collect(),
        ..Instances::default()
    };
    let _ = instances.expression_types.borrow_mut().insert(
        expression_site(&expression),
        crate::ty::Type::con("Control", vec![crate::ty::Type::Var(7)]),
    );
    let analyzer = analyzer(&index, &instances);
    let expected = arguments
        .into_iter()
        .chain([crate::ty::Type::Var(7)])
        .map(|argument| FieldInstance {
            name: "Control".to_owned(),
            arguments: vec![argument.to_string()],
        })
        .collect();
    assert_eq!(analyzer.updated_records(&expression), expected);
    assert_eq!(instances.owner_substitutions.get(), 2);
}

fn receiver() -> (Expr, Index<'static>) {
    let mut index = Index::default();
    index.field_rows.register_parameters("Control", &[]);
    (
        Expr::Update {
            record: "control".to_owned(),
            fields: Vec::new(),
        },
        index,
    )
}

fn analyzer<'a>(index: &'a Index<'a>, instances: &'a Instances) -> Analyzer<'a> {
    Analyzer {
        index,
        instances,
        rows: &[],
        returns: &[],
        file_scope: CallableEnv::default(),
    }
}
