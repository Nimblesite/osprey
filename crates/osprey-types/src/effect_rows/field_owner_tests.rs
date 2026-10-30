//! Resource and semantic controls for record owner projection.
use super::*;
use crate::effect_rows::{
    expression_site, identity_tests::with_empty_analyzer, provenance::measured_clones,
    value_shapes::value_shape, Instances, Projection,
};

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

#[test]
fn primitive_lookups_do_not_clone_erased_provenance() {
    for expression in lookup_expressions() {
        with_empty_analyzer(|analyzer| assert_primitive_lookup(&analyzer, &expression));
    }
}

#[test]
fn primitive_field_lookups_do_not_clone_erased_provenance() {
    for target in lookup_expressions() {
        let expression = Expr::FieldAccess {
            target: Box::new(target),
            field: "enabled".to_owned(),
        };
        with_empty_analyzer(|analyzer| assert_primitive_lookup(&analyzer, &expression));
    }
}

fn lookup_expressions() -> [Expr; 2] {
    [
        Expr::Identifier("candidate".to_owned()),
        Expr::Path(osprey_ast::SymbolPath::new(["Scope", "candidate"])),
    ]
}

fn assert_primitive_lookup(analyzer: &Analyzer<'_>, expression: &Expr) {
    bind_lookup_types(analyzer, expression);
    let env = analyzer.scoped_env(&["candidate".to_owned()]);
    let (actual, clones) = measured_clones(|| analyzer.value(expression, &[], &env));
    assert_eq!(actual, value_shape(&crate::ty::Type::bool()));
    assert_eq!(
        clones, 0,
        "primitive lookups must not clone discarded provenance"
    );
}

fn bind_lookup_types(analyzer: &Analyzer<'_>, expression: &Expr) {
    let mut types = analyzer.instances.expression_types.borrow_mut();
    let _ = types.insert(expression_site(expression), crate::ty::Type::bool());
    if let Expr::FieldAccess { target, .. } = expression {
        let ty = crate::ty::Type::Record {
            name: "Control".to_owned(),
            fields: [("enabled".to_owned(), crate::ty::Type::bool())]
                .into_iter()
                .collect(),
        };
        let _ = types.insert(expression_site(target), ty);
    }
}

#[test]
fn borrowed_named_projection_preserves_generic_declared_rows_and_stored_effects() {
    let source = "effect Read<T>{get:fn()->T}\neffect Alarm{ring:fn()->int}\n\
                  type Control<T>={press:fn()->int ![Read<T>]}\n\
                  type Other<T>={press:fn()->int ![]}\n";
    let parsed = osprey_syntax::parse_program(source);
    assert_eq!(parsed.errors, []);
    let index = Index::collect(&parsed.program);
    let instances = Instances::default();
    let analyzer = analyzer(&index, &instances);
    for owner in ["Control", "Other"] {
        for argument in ["int", "string"] {
            for stored in [None, Some(false), Some(true)] {
                assert_declared_projection(&analyzer, owner, argument, stored);
            }
        }
    }
}

fn assert_declared_projection(a: &Analyzer<'_>, owner: &str, argument: &str, stored: Option<bool>) {
    let value = declared_projection_value(owner, argument, stored);
    let path = [Projection::Field("press".to_owned())];
    let actual = a.project_borrowed_path(&value, &path);
    assert_eq!(actual, a.project_path(value.clone(), &path));
    let invoked = actual
        .and_then(|value| value.callable)
        .map(|callable| a.invoke_with_values(callable, &[]));
    assert_eq!(
        invoked,
        Some(expected_projection_row(owner, argument, stored))
    );
}

fn declared_projection_value(owner: &str, argument: &str, stored: Option<bool>) -> Value {
    Value {
        field_owner: Some(FieldInstance {
            name: owner.to_owned(),
            arguments: vec![argument.to_owned()],
        }),
        fields: stored
            .into_iter()
            .map(|known| {
                (
                    "press".to_owned(),
                    stored_projection_callback(argument, known),
                )
            })
            .collect(),
        ..Value::default()
    }
}

fn stored_projection_callback(argument: &str, known: bool) -> Value {
    if known {
        Index::known_value(expected_projection_row("Other", argument, Some(true)))
    } else {
        Value::unknown_value()
    }
}

fn expected_projection_row(owner: &str, argument: &str, stored: Option<bool>) -> Summary {
    let declared =
        (owner == "Control").then(|| Requirement::new("Read", "get", vec![argument.to_owned()]));
    let concrete = (stored == Some(true)).then(|| Requirement::new("Alarm", "ring", Vec::new()));
    Summary {
        required: declared.into_iter().chain(concrete).collect(),
        ..Summary::default()
    }
}

#[test]
fn acyclic_holder_callback_relays_keep_their_precise_return() {
    let relays = relay_definitions(16, |index| {
        format!(
            "fn relay{index}()=Holder{{callback:relay{}().callback}}",
            index + 1
        )
    });
    let source = format!(
        "type Holder={{callback:fn()->int}}\n{relays}\n\
        fn relay16()=Holder{{callback:fn()=>7}}\n\
        let holder=relay0()\nlet answer=holder.callback()\n"
    );
    crate::effect_rows_tests::assert_accepted(&source);
}

#[test]
fn twenty_acyclic_pure_callback_relays_remain_accepted() {
    let relays = relay_definitions(20, |index| {
        format!("fn relay{index}()=relay{}()", index + 1)
    });
    let source = format!(
        "{relays}\nfn relay20()=fn()=>7\n\
        let callback=relay0()\nlet answer=callback()\n"
    );
    crate::effect_rows_tests::assert_accepted(&source);
}

fn relay_definitions(depth: usize, body: impl Fn(usize) -> String) -> String {
    (0..depth).map(body).collect::<Vec<_>>().join("\n")
}
