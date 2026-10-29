//! Effect row identity tests.
use super::*;

#[test]
fn unresolved_generic_sites_do_not_share_an_effect_identity() {
    let index = Index {
        effects: HashMap::from([("Carry".to_owned(), 1)]),
        ..Index::default()
    };
    let instances = Instances::default();
    let rows = [];
    let returns = [];
    let analyzer = Analyzer {
        index: &index,
        rows: &rows,
        returns: &returns,
        instances: &instances,
        file_scope: CallableEnv::default(),
    };
    let first = Some(Position { line: 2, column: 3 });
    let second = Some(Position { line: 4, column: 5 });
    let handlers = HashMap::new();
    let handler = analyzer.instance_arguments("Carry", first, &handlers, "handler");
    let perform = analyzer.perform_instance_arguments("Carry", first);
    let another = analyzer.perform_instance_arguments("Carry", second);
    assert_ne!(Some(&handler), perform.first());
    assert_ne!(perform, another);
    assert_ne!(perform, vec![Vec::<String>::new()]);
}

#[test]
fn unresolved_callback_provenance_survives_partial_application_and_widening() {
    let index = Index::default();
    let instances = Instances::default();
    let rows = [];
    let returns = [];
    let analyzer = Analyzer {
        index: &index,
        rows: &rows,
        returns: &returns,
        instances: &instances,
        file_scope: CallableEnv::default(),
    };
    let callback = Value {
        callable: Some(Callable::Parameter {
            level: 0,
            index: 1,
            projection: Vec::new(),
        }),
        ..Value::default()
    };
    let partial = analyzer.substitute_value_at(callback.clone(), 0, &[None]);
    assert!(matches!(
        partial.callable,
        Some(Callable::Parameter { index: 1, .. })
    ));
    let supplied_but_opaque = analyzer.substitute_value_at(callback, 0, &[None, None]);
    assert!(matches!(
        supplied_but_opaque.callable,
        Some(Callable::Unknown)
    ));

    let projected = Value {
        callable: Some(Callable::Parameter {
            level: 0,
            index: 0,
            projection: vec![Projection::Handled(BTreeMap::new())],
        }),
        ..Value::default()
    };
    let widened = widen_value(projected, MAX_PROVENANCE_DEPTH);
    assert!(matches!(widened.callable, Some(Callable::Unknown)));
    assert!(widened.deferred.unresolved_dynamic_call);
}

#[test]
fn branching_recursive_provenance_has_a_finite_size() {
    fn nodes(value: &Value) -> usize {
        1 + value.fields.values().map(nodes).sum::<usize>()
    }

    let mut value = Value::unknown_callable();
    for _ in 0..10 {
        value = Value {
            fields: BTreeMap::from([
                ("left".to_owned(), value.clone()),
                ("right".to_owned(), value),
            ]),
            ..Value::default()
        }
        .widened();
    }
    assert!(
        nodes(&value) <= 768,
        "recursive provenance grew without a size bound"
    );
}

#[test]
fn a_call_result_fed_back_through_the_same_symbolic_call_is_cut_after_four_layers() {
    // A recursion through a parameter's function field returns the result of
    // `p.f(x)` as the next `x`; each fixed-point sweep nests one more
    // `Returned` layer. Widening must cut that nesting well before the path
    // depth limit, or the sweep count grows with the depth limit
    // [EFFECTS-PROVENANCE].
    fn layer(inner: Value) -> Value {
        Value {
            callable: Some(Callable::Parameter {
                level: 0,
                index: 2,
                projection: vec![
                    Projection::Field("build".to_owned()),
                    Projection::Returned(Box::new(CallArguments {
                        positional: vec![Some(inner)],
                        named: Vec::new(),
                    })),
                ],
            }),
            ..Value::default()
        }
    }
    fn call_layers(value: &Value) -> usize {
        match &value.callable {
            Some(Callable::Parameter { projection, .. }) => projection
                .iter()
                .filter_map(|part| match part {
                    Projection::Returned(arguments) => arguments.positional.first(),
                    _ => None,
                })
                .flatten()
                .map(|inner| 1 + call_layers(inner))
                .max()
                .unwrap_or(1),
            _ => 0,
        }
    }
    let mut value = Value::unknown_callable();
    for _ in 0..12 {
        value = layer(value);
    }
    assert_eq!(call_layers(&value), 12);
    let widened = widen_value(value, 0);
    assert!(
        call_layers(&widened) <= 4,
        "{} layers survive",
        call_layers(&widened)
    );
}
