//! Resource and semantic controls for record owner projection.
use super::*;
use crate::effect_rows::{
    expression_site, identity_tests::with_empty_analyzer, provenance::measured_clones,
    value_shapes::value_shape, BTreeSet, Instances, Projection,
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

pub(in crate::effect_rows) fn analyzer<'a>(
    index: &'a Index<'a>,
    instances: &'a Instances,
) -> Analyzer<'a> {
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

#[test]
fn cyclic_reference_graph_preserves_finite_callback_spines() {
    for depth in [20, 30] {
        crate::effect_rows_tests::assert_accepted(&cyclic_spine_source(depth, "7"));
    }
}

#[test]
fn callback_record_survives_a_cyclic_reference_graph() {
    for depth in [20, 30] {
        let spine = cyclic_spine_source(depth, "Packet{callback:fn()=>7}");
        let source =
            format!("type Packet={{callback:fn()->int}}\n{spine}let result=answer.callback()\n");
        crate::effect_rows_tests::assert_accepted(&source);
    }
}

fn cyclic_spine_source(depth: usize, leaf: &str) -> String {
    let relays = relay_definitions(depth - 1, |index| {
        format!("fn spine{index}(stop)=fn()=>spine{}(true)", index + 1)
    });
    let last = depth - 1;
    let calls = "()".repeat(last);
    format!(
        "effect Alarm {{ring:fn()->int}}\nfn finish(callback)=0\n{relays}\nfn spine{last}(stop)={{\n\
         let ignored=if stop {{0}} else {{finish(spine0(true))}}\n{leaf}\n}}\n\
         let answer=spine0(true){calls}\n"
    )
}

fn relay_definitions(depth: usize, body: impl Fn(usize) -> String) -> String {
    (0..depth).map(body).collect::<Vec<_>>().join("\n")
}

std::thread_local! {
    static VALUE_VISITS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static VISITED_SITE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

pub(in crate::effect_rows) fn record_value_visit(expression: &Expr) {
    VISITED_SITE.with(|slot| {
        if slot.get() == Some(expression_site(expression)) {
            crate::effect_rows::provenance::record_counter(&VALUE_VISITS);
        }
    });
}

pub(in crate::effect_rows) fn measured_value_visits<T>(
    expression: &Expr,
    run: impl FnOnce() -> T,
) -> (T, usize) {
    VISITED_SITE.with(|slot| slot.set(Some(expression_site(expression))));
    let measured = crate::effect_rows::provenance::measured_counter(&VALUE_VISITS, run);
    VISITED_SITE.with(|slot| slot.set(None));
    measured
}

#[test]
fn nested_callback_values_do_not_revisit_the_same_leaf_exponentially() {
    let samples = [2, 4, 6, 8].map(nested_callback_sample);
    for (depth, visits) in samples {
        assert!(
            visits <= depth + 1,
            "depth {depth} revisited one leaf {visits} times"
        );
    }
}

pub(in crate::effect_rows) fn nested_callback_body(depth: usize, leaf: &str) -> String {
    (0..depth).fold(leaf.to_owned(), |inner, _| {
        format!("(fn(work)=>work())(fn()=>{inner})")
    })
}

fn nested_callback_sample(depth: usize) -> (usize, usize) {
    let body = nested_callback_body(depth, "leaf");
    let source = format!("let leaf=7\nfn main()={body}\nlet answer=main()\n");
    crate::effect_rows_tests::assert_accepted(&source);
    let visits = fixture_function_values(&source, |_, body| pure_callback_visits(body))
        .into_iter()
        .sum();
    eprintln!("nested callback depth {depth}: leaf visits={visits}");
    (depth, visits)
}

fn pure_callback_visits(body: &Expr) -> usize {
    let leaf = nested_callback_leaf(body);
    assert!(matches!(leaf, Expr::Identifier(name) if name == "leaf"));
    with_scalar_callback_analyzer(body, |analyzer| {
        let expected = Value {
            field_names: Some(BTreeSet::new()),
            ..Value::default()
        };
        let mut env = CallableEnv::default();
        let _ = env.values.insert("leaf".to_owned(), expected.clone());
        let (actual, visits) = measured_value_visits(leaf, || analyzer.value(body, &[], &env));
        assert_eq!(actual, Some(expected));
        assert_eq!(analyzer.expression(body, &[], &env), Summary::default());
        visits
    })
}

fn with_scalar_callback_analyzer<T>(body: &Expr, run: impl FnOnce(Analyzer<'_>) -> T) -> T {
    let index = Index::default();
    let instances = Instances::default();
    seed_callback_scalar_types(body, &instances);
    run(analyzer(&index, &instances))
}

fn seed_callback_scalar_types(expression: &Expr, instances: &Instances) {
    match expression {
        Expr::Call {
            function,
            arguments,
            ..
        } => {
            record_callback_scalar_type(expression, instances);
            seed_callback_arguments(function, arguments, instances);
        }
        Expr::Lambda { body, .. } => seed_callback_scalar_types(body, instances),
        Expr::Identifier(name) if name == "leaf" => {
            record_callback_scalar_type(expression, instances);
        }
        _ => {}
    }
}

fn seed_callback_arguments(function: &Expr, arguments: &[Expr], instances: &Instances) {
    for expression in std::iter::once(function).chain(arguments) {
        seed_callback_scalar_types(expression, instances);
    }
}

fn record_callback_scalar_type(expression: &Expr, instances: &Instances) {
    let _ = instances
        .expression_types
        .borrow_mut()
        .insert(expression_site(expression), crate::ty::Type::int());
}

pub(in crate::effect_rows) fn nested_callback_leaf(expression: &Expr) -> &Expr {
    match expression {
        Expr::Call { arguments, .. } => arguments.first().map_or(expression, nested_callback_leaf),
        Expr::Lambda { body, .. } => nested_callback_leaf(body),
        _ => expression,
    }
}

#[test]
fn nested_callbacks_preserve_effectful_leaf_requirements() {
    for depth in [2, 4, 6, 8] {
        let body = nested_callback_body(depth, "perform Alarm.ring()");
        let declaration = "effect Alarm { ring: fn()->int }\n";
        crate::effect_rows_tests::assert_rejected_with(
            &format!("{declaration}fn main() ![] = {body}\n"),
            &["outside its declared row", "Alarm.ring"],
        );
        crate::effect_rows_tests::assert_accepted(&format!(
            "{declaration}fn main() ![] = {{\nlet h=handler Alarm {{ring=>7}}\nh(fn()=>{body})\n}}\n"
        ));
    }
}

pub(in crate::effect_rows) fn fixture_function_values<T>(
    source: &str,
    run: impl Fn(&Index<'_>, &Expr) -> T,
) -> Vec<T> {
    let parsed = osprey_syntax::parse_program(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let index = Index::collect(&parsed.program);
    assert_eq!(index.functions.len(), 1);
    index
        .functions
        .iter()
        .map(|function| run(&index, function.body))
        .collect()
}
