//! Effect row solver return-history tests.
use super::super::model_values::{
    measure_snapshot_step, snapshot_function, snapshot_row, snapshot_value, with_snapshot_analyzer,
};
use super::{ReturnHistory, Value};
use crate::effect_rows::expr_args::{
    assert_metadata_written, assert_model_metadata_matches as assert_lambda_metadata,
};
use crate::effect_rows::field_rows::tests::{analyzer, fixture_function_values};
use crate::effect_rows::{
    expression_site, Analyzer, Callable, CallableEnv, Expr, Index, Instances, KnownCallable,
    Summary,
};

const FUNCTION: usize = 0;
const STABLE_SWEEPS: usize = 512;
const HISTORY_WIDTH: usize = 512;
const HISTORY_COMPARISONS_PER_LEVEL: u32 = 16;

#[test]
fn stable_return_sweeps_do_not_retain_duplicate_provenance() {
    let value = Value::channel(7);
    let mut history = ReturnHistory::default();
    let mut before = None;
    for _ in 0..STABLE_SWEEPS {
        let returned = history.settle(FUNCTION, before.as_ref(), value.clone());
        assert_eq!(returned, value);
        before = Some(returned);
    }
    let retained = history
        .returns
        .values()
        .map(|returns| returns.seen.len())
        .sum::<usize>();
    assert_eq!(
        retained, 1,
        "stable sweeps must retain one provenance value"
    );
}

#[test]
fn seeded_stable_return_still_detects_a_later_cycle() {
    let first = Value::channel(7);
    let second = Value::channel(9);
    let mut history = ReturnHistory::default();
    assert_eq!(history.settle(FUNCTION, Some(&first), first.clone()), first);
    assert_eq!(
        history.settle(FUNCTION, Some(&first), second.clone()),
        second
    );
    let returned = history.settle(FUNCTION, Some(&second), first.clone());
    let mut expected = first;
    expected.union(second);
    assert_eq!(returned, expected);
}

#[test]
fn non_last_revisits_keep_joining_later_provenance() {
    let [first, second, third] = [7, 9, 11].map(Value::channel);
    let mut history = ReturnHistory::default();
    let mut before = None;
    for value in [&first, &first, &second, &second, &third, &first] {
        before = Some(history.settle(FUNCTION, before.as_ref(), value.clone()));
    }
    let mut expected = first;
    expected.union(third);
    assert_eq!(before, Some(expected.clone()));
    let joined = history.settle(FUNCTION, before.as_ref(), second.clone());
    expected.union(second);
    assert_eq!(joined, expected);
}

fn reference_settle(
    history: &mut super::HashMap<usize, (Vec<Value>, bool)>,
    id: usize,
    before: Option<&Value>,
    computed: Value,
) -> Value {
    let (seen, joined) = history.entry(id).or_default();
    *joined |= before != Some(&computed) && seen.contains(&computed);
    if !*joined {
        if seen.last() != Some(&computed) {
            seen.push(computed.clone());
        }
        return computed;
    }
    let mut value = before.cloned().unwrap_or_default();
    value.union(computed);
    value.widened().widened()
}

#[test]
fn history_matches_reference_for_seeded_and_interleaved_sequences() {
    for sequence in 0..1024 {
        for seed in [None, Some(Value::channel(7)), Some(Value::unknown_value())] {
            for functions in [1, 2] {
                assert_history_sequence(sequence, seed.as_ref(), functions);
            }
        }
    }
}

fn assert_history_sequence(sequence: usize, seed: Option<&Value>, functions: usize) {
    let mut actual = ReturnHistory::default();
    let mut reference = super::HashMap::new();
    let mut before: super::HashMap<_, _> = (0..functions)
        .filter_map(|id| seed.cloned().map(|value| (id, value)))
        .collect();
    for step in 0..5 {
        let id = step % functions;
        let computed = history_value((sequence >> (step * 2)) % 4);
        let expected = reference_settle(&mut reference, id, before.get(&id), computed.clone());
        let returned = actual.settle(id, before.get(&id), computed);
        assert_eq!(returned, expected, "{sequence}, {step}, {functions}");
        assert_eq!(
            actual.returns.get(&id).map(|row| row.joined),
            reference.get(&id).map(|(_, joined)| *joined)
        );
        let _ = before.insert(id, returned);
    }
}

fn history_value(which: usize) -> Value {
    match which {
        0 => Value::channel(7),
        1 => Value::channel(9),
        2 => Value::parameter(1, 2),
        _ => Value::unknown_value(),
    }
}

fn seed_search_history(history: &mut ReturnHistory) -> Option<Value> {
    let mut before = None;
    for channel in 0..HISTORY_WIDTH {
        before = Some(history.settle(FUNCTION, before.as_ref(), Value::channel(channel)));
    }
    before
}

#[test]
fn history_search_work_grows_logarithmically() {
    let mut history = ReturnHistory::default();
    let before = seed_search_history(&mut history);
    let computed = Value::channel(HISTORY_WIDTH);
    let expected = computed.clone();
    let (returned, comparisons) =
        super::super::provenance::CloneProbe::measured_value_comparisons(|| {
            history.settle(FUNCTION, before.as_ref(), computed)
        });
    assert_eq!(returned, expected);
    assert!(
        u32::try_from(comparisons)
            .is_ok_and(|count| count <= HISTORY_COMPARISONS_PER_LEVEL * HISTORY_WIDTH.ilog2()),
        "{comparisons} comparisons for {HISTORY_WIDTH} retained returns"
    );
}

#[test]
fn history_seed_clones_once_and_stable_sweeps_clone_nothing() {
    let mut history = ReturnHistory::default();
    let expected = Value::channel(7);
    let (returned, clones) = super::super::provenance::measured_clones(|| {
        history.settle(FUNCTION, Some(&expected), Value::channel(7))
    });
    assert_eq!(returned, expected);
    assert_eq!(clones, 1);
    let ((), clones) = super::super::provenance::measured_clones(|| {
        for _ in 0..STABLE_SWEEPS {
            assert_eq!(
                history.settle(FUNCTION, Some(&expected), Value::channel(7)),
                expected
            );
        }
    });
    assert_eq!(clones, 0);
}

#[test]
fn snapshot_return_preserves_output_history_and_change_without_copying_before() {
    with_snapshot_analyzer(
        Summary::default(),
        Some(snapshot_value()),
        |analyzer, mut state| {
            let body = Expr::Bool(false);
            let mut history = ReturnHistory::default();
            let expected = analyzer.function_return(&snapshot_function(&body));
            let step = measure_snapshot_step(&analyzer, &body, &mut state, &mut history);
            assert_eq!(state.1, [expected]);
            assert_eq!(history.returns.get(&0).map(|row| row.joined), Some(false));
            assert_eq!(step, (true, 1), "old return snapshot must move");
        },
    );
}

#[test]
fn snapshot_row_preserves_fresh_verdict_and_other_metadata_without_copying_before() {
    with_snapshot_analyzer(snapshot_row(), None, |analyzer, mut state| {
        let body = Expr::Block {
            statements: Vec::new(),
            value: None,
            position: None,
        };
        let mut history = ReturnHistory::default();
        let mut expected = snapshot_row();
        expected.unresolved_dynamic_call = false;
        let step = measure_snapshot_step(&analyzer, &body, &mut state, &mut history);
        assert_eq!(state.0, [expected]);
        assert_eq!(state.1.as_slice(), analyzer.returns);
        assert!(history.returns.is_empty());
        assert_eq!(step, (true, 0), "old row snapshot must be borrowed");
    });
}

#[test]
fn returned_projection_preserves_named_and_positional_argument_payloads() {
    let mut payload = super::super::model_values::unknown_child_record();
    let _ = payload
        .performed
        .insert(super::super::Requirement::new("Alarm", "ring", Vec::new()));
    for named in [false, true] {
        assert_returned_payload(&payload, named);
    }
}

fn assert_returned_payload(payload: &Value, named: bool) {
    super::super::identity_tests::with_empty_analyzer(|analyzer| {
        let mut callable = super::super::model_values::returning_unknown(0);
        if let Callable::Known(known) = &mut callable {
            known.returned = Some(Box::new(Value::parameter(0, 0)));
        }
        let mut value = super::super::model_values::unknown_child_record();
        value.callable = Some(callable);
        let arguments = returned_arguments(payload, named);
        let path = [super::super::Projection::Returned(Box::new(arguments))];
        let expected = analyzer.project_path(value.clone(), &path);
        assert_eq!(expected, Some(payload.clone()));
        assert_eq!(analyzer.project_borrowed_path(&value, &path), expected);
    });
}

fn returned_arguments(payload: &Value, named: bool) -> super::super::CallArguments {
    if named {
        super::super::CallArguments {
            positional: Vec::new(),
            named: vec![("argument0".to_owned(), Some(payload.clone()))],
        }
    } else {
        super::super::CallArguments {
            positional: vec![Some(payload.clone())],
            named: Vec::new(),
        }
    }
}

#[test]
fn scalar_lambda_calls_keep_the_complete_original_callable() {
    for ty in ["int", "float", "bool", "string", "Unit", "Ptr"] {
        for callee in oracle_callees() {
            for body in oracle_callback_bodies(ty) {
                let _ = fixture_function_values(
                    &format!("fn fixture()=fn(argument)=>{body}\n"),
                    |_, expression| assert_original_lambda_value(expression, ty, &callee),
                );
            }
        }
    }
}

fn assert_original_lambda_value(expression: &Expr, ty: &str, callee: &Callable) {
    let before = scalar_lambda_instances(expression, ty);
    let index = Index::default();
    let env = oracle_callback_env(callee.clone());
    let _ = compare_lambda_model(&index, expression, &env, &before);
}

fn compare_lambda_model(
    index: &Index<'_>,
    expression: &Expr,
    env: &CallableEnv,
    before: &Instances,
) -> Instances {
    let after = before.clone();
    let expected = original_lambda_value(&analyzer(index, before), expression, env);
    assert!(
        expected.is_some(),
        "the original path must produce a callable"
    );
    assert_eq!(
        analyzer(index, &after).value(expression, &[], env),
        expected
    );
    assert_lambda_metadata(before, &after);
    after
}

fn scalar_lambda_instances(expression: &Expr, ty: &str) -> Instances {
    let mut instances = Instances::default();
    if let Expr::Lambda { body, .. } = expression {
        seed_oracle_metadata(&mut instances, expression_site(body));
        let _ = instances
            .expression_types
            .get_mut()
            .insert(expression_site(body), crate::ty::Type::con(ty, Vec::new()));
        if crate::methods::parts(body).is_some() {
            let _ = instances.methods.insert(
                expression_site(body),
                crate::methods::Target::Field("source".to_owned()),
            );
        }
    }
    instances
}

fn seed_oracle_metadata(instances: &mut Instances, site: usize) {
    let requirements = crate::EffectRequirements {
        operations: vec!["Alarm.ring".to_owned()],
        ..crate::EffectRequirements::default()
    };
    let _ = instances
        .signal_queries
        .get_mut()
        .insert(site, requirements);
    let _ = instances
        .handler_inference
        .get_mut()
        .insert((0, 0), [vec!["Alarm".to_owned()]].into_iter().collect());
}

fn original_lambda_value(
    analyzer: &Analyzer<'_>,
    expression: &Expr,
    env: &CallableEnv,
) -> Option<Value> {
    match expression {
        Expr::Lambda {
            parameters, body, ..
        } => {
            let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
            let local = env.enter_lambda(&names);
            Some(original_lambda_model(analyzer, body, local, names))
        }
        _ => None,
    }
}

fn original_lambda_model(
    analyzer: &Analyzer<'_>,
    body: &Expr,
    mut env: CallableEnv,
    names: Vec<String>,
) -> Value {
    let summary = analyzer.expression(body, &[], &env);
    let returned = analyzer.returned_value(body, &[], &mut env).map(Box::new);
    assert_original_lambda_body(body, &summary, returned.as_deref());
    Value::from_callable(Callable::Known(Box::new(KnownCallable {
        parameters: names,
        summary,
        returned,
    })))
}

fn assert_original_lambda_body(body: &Expr, summary: &Summary, returned: Option<&Value>) {
    assert!(
        !summary.required.is_empty()
            || !summary.parameter_uses.is_empty()
            || summary.unresolved_dynamic_call,
        "the effectful callback must remain effectful"
    );
    if matches!(body, Expr::Block { value: None, .. }) {
        assert_eq!(
            returned, None,
            "a block without a tail has no returned provenance"
        );
    }
}

fn oracle_known_callable() -> Callable {
    let mut callee = crate::effect_rows::model_values::returning_unknown(0);
    if let Callable::Known(known) = &mut callee {
        known.parameters.clear();
        let _ = known
            .summary
            .required
            .insert(super::super::Requirement::new("Alarm", "ring", Vec::new()));
    }
    callee
}

fn oracle_callees() -> [Callable; 4] {
    let known = oracle_known_callable();
    let parameter = Callable::Parameter {
        level: 0,
        index: 0,
        projection: Vec::new(),
    };
    [
        known.clone(),
        Callable::Unknown,
        parameter,
        Callable::Alternatives(vec![known, Callable::Unknown]),
    ]
}

fn oracle_callback_bodies(ty: &str) -> impl Iterator<Item = &'static str> {
    [
        "source()",
        "run(fn()=>source())",
        "(if true {source}else{source})()",
        "subject.source()",
    ]
    .into_iter()
    .chain((ty == "Unit").then_some("{let ignored=source()\n}"))
}

fn oracle_callback_env(callee: Callable) -> CallableEnv {
    let source = Value::from_callable(callee);
    let mut subject = Value::default();
    let _ = subject.fields.insert("source".to_owned(), source.clone());
    let mut env = CallableEnv::default();
    env.values.extend([
        ("source".to_owned(), source),
        ("subject".to_owned(), subject),
    ]);
    install_oracle_run(&mut env);
    env
}

fn install_oracle_run(env: &mut CallableEnv) {
    let values = fixture_function_values("fn fixture()=fn(work)=>work()\n", |_, expression| {
        super::super::identity_tests::with_empty_analyzer(|analyzer| {
            analyzer.value(expression, &[], env)
        })
    });
    assert!(values.iter().all(Option::is_some));
    env.values.extend(
        values
            .into_iter()
            .flatten()
            .map(|value| ("run".to_owned(), value)),
    );
}

#[test]
fn scalar_callback_oracle_checks_actual_metadata_writes() {
    let source = "effect Alarm {ring:fn()->int}\ntype Control={press:fn()->int![]}\nfn fixture()=fn()=>source({\nlet deps=signalDependencies(fn()=>perform Alarm.ring())\nlet bad=Control{press:fn()=>perform Alarm.ring()}\nhandle Alarm {ring=>7}\nperform Alarm.ring()\n})\n";
    let _ = fixture_function_values(source, assert_actual_metadata_writes);
}

fn assert_actual_metadata_writes(index: &Index<'_>, expression: &Expr) {
    let mut before = scalar_lambda_instances(expression, "int");
    before.signal_queries.get_mut().clear();
    before.handler_inference.get_mut().clear();
    let _ = before.argument_types.insert(Vec::new(), Vec::new());
    let env = oracle_callback_env(oracle_known_callable());
    let after = compare_lambda_model(index, expression, &env, &before);
    assert_metadata_written(&before);
    assert_metadata_written(&after);
}
