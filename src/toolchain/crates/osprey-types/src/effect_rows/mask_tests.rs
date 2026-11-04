//! Effect row mask tests: a request under `n` masks is discharged by the
//! `n+1`th answering activation, and saturation never discharges.
//! Implements [EFFECTS-MASK].
use super::*;

fn get() -> Summary {
    let mut summary = Summary::default();
    let _ = summary
        .required
        .insert(Requirement::new("Store", "get", Vec::new()));
    let _ = summary
        .required
        .insert(Requirement::new("Log", "note", Vec::new()));
    summary
}

fn handled(summary: Summary) -> Summary {
    let operations = HashSet::from(["get".to_owned()]);
    summary.without_operations("Store", &[], &operations, false)
}

fn has_store(summary: &Summary) -> bool {
    summary.required.iter().any(|r| r.effect == "Store")
}

#[test]
fn a_masked_request_passes_one_handler_and_meets_the_next() {
    let masked = get().masked("Store", &[]);
    let once = handled(masked);
    assert!(has_store(&once), "the first handler is skipped");
    assert!(!has_store(&handled(once)), "the second handler answers");
}

#[test]
fn a_mask_leaves_other_effects_alone() {
    let masked = handled(get().masked("Store", &[]));
    assert!(masked.required.iter().any(|r| r.effect == "Log"));
    assert!(masked
        .required
        .iter()
        .all(|r| r.effect != "Log" || r.masked == 0));
}

#[test]
fn a_saturated_request_is_never_discharged() {
    let deep = (0..MASK_LIMIT).fold(get(), |summary, _| summary.masked("Store", &[]));
    let answered = (0..MASK_LIMIT * 2).fold(deep, |summary, _| handled(summary));
    assert!(has_store(&answered));
}

#[test]
fn a_callback_use_replays_its_layers_innermost_first() {
    let mask = Scope::Masked {
        effect: "Store".to_owned(),
        arguments: Vec::new(),
    };
    let answer = Scope::Handled(BTreeMap::from([(
        Requirement::new("Store", "get", Vec::new()),
        1,
    )]));
    assert!(has_store(&get().within(&[mask.clone(), answer.clone()])));
    assert!(!has_store(&get().within(std::slice::from_ref(&answer))));
    let twice = Scope::Handled(BTreeMap::from([(
        Requirement::new("Store", "get", Vec::new()),
        2,
    )]));
    assert!(!has_store(&get().within(&[mask, twice])));
}

#[test]
fn returned_provider_answers_preserve_instance_and_dynamic_scope() {
    for matching in [false, true] {
        let (pending, summary) = provider_request();
        let callback = expr_args::latent_alarm_callback();
        let env = provider_env(&pending, &summary, &callback, matching);
        expr_args::assert_call_outcome("source()", &env, &summary, &pending);
        let returned = if matching { &callback } else { &pending };
        assert_active_provider(&env, &summary, returned);
    }
}

fn provider_request() -> (Value, Summary) {
    let request = Requirement::new("Supply", "make", vec!["int".to_owned()]);
    let pending = Value {
        performed: [request.clone()].into_iter().collect(),
        ..Value::default()
    };
    let summary = Summary {
        required: [request].into_iter().collect(),
        ..Summary::default()
    };
    (pending, summary)
}

fn provider_env(
    pending: &Value,
    summary: &Summary,
    callback: &Value,
    matching: bool,
) -> CallableEnv {
    let answer = Requirement::new(
        "Supply",
        "make",
        vec![if matching { "int" } else { "string" }.to_owned()],
    );
    let source = expr_args::function_outcome(pending.clone(), summary.clone());
    let mut env = CallableEnv::default();
    let _ = env.values.insert("source".to_owned(), source);
    let _ = env.handler_returns.insert(answer, callback.clone());
    env
}

fn assert_active_provider(env: &CallableEnv, summary: &Summary, returned: &Value) {
    let samples =
        field_rows::tests::fixture_function_values("fn fixture()=source()\n", |_, body| {
            identity_tests::with_empty_analyzer(|analyzer| {
                assert_provider_models(&analyzer, body, env, summary, returned);
            });
        });
    assert_eq!(samples.len(), 1);
}

fn assert_provider_models(
    analyzer: &Analyzer<'_>,
    body: &Expr,
    env: &CallableEnv,
    summary: &Summary,
    returned: &Value,
) {
    assert_eq!(analyzer.expression(body, &[], env), *summary);
    assert_eq!(analyzer.value(body, &[], env), Some(returned.clone()));
    assert_eq!(
        analyzer.known_call_model(body, &[], env),
        Some((summary.clone(), returned.clone()))
    );
    let mut active = env.clone();
    assert_eq!(
        analyzer.lambda_body_model(body, &[], &mut active),
        (summary.clone(), Some(returned.clone()))
    );
}
