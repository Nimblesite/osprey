//! Positional arguments borrowed from their original inference sites.
use osprey_ast::Expr;

#[derive(Clone, Copy)]
pub(crate) struct ExprArgs<'a> {
    receiver: Option<&'a Expr>,
    arguments: &'a [Expr],
}

#[cfg(test)]
#[test]
fn untyped_function_returning_cps_models_each_leaf_once() {
    use super::field_rows::tests::{fixture_function_values, nested_callback_body};
    for depth in [2, 4, 6, 8] {
        let body = nested_callback_body(depth, "fn()=>leaf");
        let source = format!("let leaf=7\nfn main()={body}\nlet answer=main()()\n");
        crate::effect_rows_tests::assert_accepted(&source);
        let samples = fixture_function_values(&source, |_, body| untyped_function_visits(body));
        assert_eq!(samples.len(), 1);
        let visits: usize = samples.into_iter().sum();
        eprintln!("untyped function-returning depth {depth}: leaf visits={visits}");
        assert!(
            visits <= depth + 1,
            "depth {depth} revisited leaf {visits} times"
        );
    }
}

#[cfg(test)]
fn untyped_function_visits(body: &Expr) -> usize {
    use super::field_rows::tests::{measured_value_visits, nested_callback_leaf};
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let scalar = Value {
            field_names: Some(std::collections::BTreeSet::new()),
            ..Value::default()
        };
        let mut env = CallableEnv::default();
        let _ = env.values.insert("leaf".to_owned(), scalar.clone());
        let expected = returned_function_shape(scalar);
        let (actual, visits) = measured_value_visits(nested_callback_leaf(body), || {
            analyzer.value(body, &[], &env)
        });
        assert_eq!(actual, Some(expected));
        assert_eq!(
            analyzer.expression(body, &[], &env),
            super::Summary::default()
        );
        visits
    })
}

#[cfg(test)]
fn returned_function_shape(returned: Value) -> Value {
    function_outcome(returned, super::Summary::default())
}

#[cfg(test)]
pub(in crate::effect_rows) fn function_outcome(returned: Value, summary: super::Summary) -> Value {
    Value::from_callable(super::Callable::Known(Box::new(super::KnownCallable {
        parameters: Vec::new(),
        summary,
        returned: Some(Box::new(returned)),
    })))
}

impl<'a> ExprArgs<'a> {
    pub(crate) fn new(arguments: &'a [Expr]) -> Self {
        Self {
            receiver: None,
            arguments,
        }
    }

    pub(crate) fn with_receiver(receiver: &'a Expr, arguments: &'a [Expr]) -> Self {
        Self {
            receiver: Some(receiver),
            arguments,
        }
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = &'a Expr> {
        self.receiver.into_iter().chain(self.arguments.iter())
    }

    pub(crate) fn get(self, index: usize) -> Option<&'a Expr> {
        match self.receiver {
            Some(receiver) if index == 0 => Some(receiver),
            Some(_) => self.arguments.get(index - 1),
            None => self.arguments.get(index),
        }
    }

    pub(crate) fn first(self) -> Option<&'a Expr> {
        self.get(0)
    }
}

#[cfg(test)]
use super::{Analyzer, CallableEnv, Value};

#[cfg(test)]
fn source_channel() -> Expr {
    Expr::Call {
        function: Box::new(Expr::Identifier("Channel".to_owned())),
        arguments: Vec::new(),
        named_arguments: Vec::new(),
    }
}

#[cfg(test)]
#[test]
fn receiver_arguments_preserve_original_channel_site_identity() {
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let receiver = Expr::Integer(0);
        let arguments = [source_channel()];
        let env = CallableEnv::default();
        let [argument] = &arguments;
        let expected = Some(Value::channel(super::expression_site(argument)));
        assert_eq!(analyzer.value(argument, &[], &env), expected);
        let first = Analyzer::receiver_first(&receiver, &arguments);
        let second = Analyzer::receiver_first(&receiver, &arguments);
        for arguments in [&first, &second] {
            let actual = arguments
                .get(1)
                .and_then(|arg| analyzer.value(arg, &[], &env));
            assert_eq!(actual, expected);
        }
    });
}

#[cfg(test)]
fn assert_argument_indices(view: ExprArgs<'_>, expected: &[&Expr]) {
    for (index, expression) in expected.iter().enumerate() {
        assert!(view.get(index).is_some_and(
            |actual| super::expression_site(actual) == super::expression_site(expression)
        ));
    }
}

#[cfg(test)]
fn assert_argument_view(view: ExprArgs<'_>, expected: &[&Expr]) {
    let sites: Vec<_> = view.iter().map(super::expression_site).collect();
    let originals: Vec<_> = expected
        .iter()
        .map(|expr| super::expression_site(expr))
        .collect();
    assert_eq!(sites, originals);
    assert_argument_indices(view, expected);
    assert_eq!(
        view.first().map(super::expression_site),
        originals.first().copied()
    );
    assert_eq!(view.get(expected.len()), None);
    assert_eq!(view.get(usize::MAX), None);
}

#[cfg(test)]
#[test]
fn positional_view_keeps_argument_order_and_identity() {
    let arguments = [Expr::Integer(7), source_channel(), Expr::Bool(false)];
    assert_argument_view(
        ExprArgs::new(&arguments),
        &arguments.iter().collect::<Vec<_>>(),
    );
}

#[cfg(test)]
#[test]
fn receiver_view_is_a_copyable_prefix_without_copying_ast_nodes() {
    let receiver = source_channel();
    let arguments = [Expr::Bool(false), Expr::Integer(9)];
    let expected: Vec<_> = std::iter::once(&receiver).chain(arguments.iter()).collect();
    let view = ExprArgs::with_receiver(&receiver, &arguments);
    let copied = view;
    for value in [view, copied] {
        assert_argument_view(value, &expected);
    }
}

#[cfg(test)]
#[test]
fn empty_and_receiver_only_views_keep_absence_explicit() {
    assert_argument_view(ExprArgs::new(&[]), &[]);
    let receiver = source_channel();
    assert_argument_view(ExprArgs::with_receiver(&receiver, &[]), &[&receiver]);
}

#[cfg(test)]
fn typed_sites(sites: [usize; 2]) -> impl Iterator<Item = (usize, crate::ty::Type)> {
    sites
        .into_iter()
        .zip([crate::ty::Type::int(), crate::ty::Type::string()])
}

#[cfg(test)]
fn seed_named_metadata(metadata: &mut super::Instances, site: usize, label: &str) {
    let _ = metadata.instantiations.insert(
        site,
        [("T".to_owned(), label.to_owned())].into_iter().collect(),
    );
    let _ = metadata
        .methods
        .insert(site, crate::methods::Target::Field(label.to_owned()));
    let requirements = crate::EffectRequirements {
        operations: vec![label.to_owned()],
        ..crate::EffectRequirements::default()
    };
    let _ = metadata
        .signal_queries
        .borrow_mut()
        .insert(site, requirements);
}

#[cfg(test)]
fn inference_metadata(sites: [usize; 2]) -> super::Instances {
    let mut metadata = super::Instances {
        expression_types: std::cell::RefCell::new(typed_sites(sites).collect()),
        instantiation_types: typed_sites(sites)
            .map(|(site, ty)| (site, [(0, ty)].into_iter().collect()))
            .collect(),
        ..super::Instances::default()
    };
    for (site, label) in sites.into_iter().zip(["receiver", "argument"]) {
        seed_named_metadata(&mut metadata, site, label);
    }
    metadata
}

#[cfg(test)]
fn assert_site_map<T: std::fmt::Debug + PartialEq>(
    map: &std::collections::HashMap<usize, T>,
    sites: (usize, usize),
) {
    let (expected, found) = sites;
    assert_eq!(found, expected);
    assert!(map.contains_key(&found));
    assert_eq!(map.get(&found), map.get(&expected));
}

#[cfg(test)]
fn assert_inference_metadata(metadata: &super::Instances, sites: (usize, usize)) {
    assert_site_map(&metadata.expression_types.borrow(), sites);
    assert_site_map(&metadata.instantiations, sites);
    assert_site_map(&metadata.instantiation_types, sites);
    assert_site_map(&metadata.methods, sites);
    assert_site_map(&metadata.signal_queries.borrow(), sites);
}

#[cfg(test)]
#[test]
fn borrowed_arguments_retain_all_pointer_keyed_inference_metadata() {
    let receiver = source_channel();
    let arguments = [Expr::Bool(false)];
    let [argument] = &arguments;
    let originals = [&receiver, argument];
    let sites = originals.map(super::expression_site);
    let metadata = inference_metadata(sites);
    let original_types = metadata.expression_types.borrow().clone();
    let view = Analyzer::receiver_first(&receiver, &arguments);
    for (original, actual) in originals.into_iter().zip(view.iter()) {
        let sites = (
            super::expression_site(original),
            super::expression_site(actual),
        );
        assert_inference_metadata(&metadata, sites);
    }
    assert_eq!(*metadata.expression_types.borrow(), original_types);
}

#[cfg(test)]
fn pipe_field_setup(inline: bool) -> String {
    let member = if inline {
        "fn() => perform Alarm.ring()"
    } else {
        "noisy"
    };
    format!(
        "effect Alarm {{ ring: fn() -> int }}\n\
             fn noisy() !Alarm = perform Alarm.ring()\n\
             type Action = Action {{ m: fn() -> int }}\n\
             fn m<T>(x: T) = 22\nfn consume(value, unused) = value\n\
             let action = Action {{ m: {member} }}\n"
    )
}

#[cfg(test)]
fn assert_nested_field_dispatch(inline: bool) {
    let setup = pipe_field_setup(inline);
    crate::effect_rows_tests::assert_accepted(&setup);
    let source = format!("{setup}let result = action.m() |> consume(0)\n");
    crate::effect_rows_tests::assert_rejected_with(&source, &["Alarm.ring"]);
    let handled = format!(
        "{setup}let result = {{ handle Alarm {{ ring => 7 }}\n\
                           action.m() |> consume(0) }}\n"
    );
    crate::effect_rows_tests::assert_accepted(&handled);
}

#[cfg(test)]
#[test]
fn nested_pipe_argument_retains_effectful_field_dispatch() {
    assert_nested_field_dispatch(false);
}

#[cfg(test)]
#[test]
fn nested_pipe_inline_field_cannot_fall_back_to_generic_free_function() {
    assert_nested_field_dispatch(true);
}

#[cfg(test)]
#[test]
fn forwarding_data_preserves_the_complete_latent_alarm_outcome() {
    for payload in latent_payloads() {
        for body in [
            "source(payload)",
            "source<int>(payload)",
            "source(argument0:payload)",
        ] {
            assert_call_outcome(
                body,
                &forwarding_env(&payload),
                &super::Summary::default(),
                &payload,
            );
        }
    }
}

#[cfg(test)]
fn forwarding_env(payload: &Value) -> CallableEnv {
    let mut source = returned_function_shape(Value::parameter(0, 0));
    if let Some(super::Callable::Known(known)) = &mut source.callable {
        known.parameters.push("argument0".to_owned());
    }
    let mut env = CallableEnv::default();
    env.values.extend([
        ("source".to_owned(), source),
        ("payload".to_owned(), payload.clone()),
    ]);
    env
}

#[cfg(test)]
fn latent_alarm_summary() -> super::Summary {
    let mut summary = super::Summary::default();
    let _ = summary
        .required
        .insert(super::Requirement::new("Alarm", "ring", Vec::new()));
    summary
}

#[cfg(test)]
pub(in crate::effect_rows) fn latent_alarm_callback() -> Value {
    let scalar = Value {
        field_names: Some(std::collections::BTreeSet::new()),
        ..Value::default()
    };
    function_outcome(scalar, latent_alarm_summary())
}

#[cfg(test)]
fn latent_payloads() -> [Value; 4] {
    let callback = latent_alarm_callback();
    let packet = packet_payload(&callback);
    let result = Value {
        result_payload: Some(Box::new(packet.clone())),
        ..Value::default()
    };
    let iterator = Value {
        element: Some(Box::new(Value::default())),
        deferred: latent_alarm_summary(),
        ..Value::default()
    };
    [callback, packet, result, iterator]
}

#[cfg(test)]
fn packet_payload(callback: &Value) -> Value {
    let mut packet = super::model_values::unknown_child_record();
    let _ = packet
        .fields
        .insert("callback".to_owned(), callback.clone());
    packet.field_names = Some(packet.fields.keys().cloned().collect());
    packet.field_owner = Some(super::field_rows::FieldInstance {
        name: "Packet".to_owned(),
        arguments: vec!["int".to_owned()],
    });
    packet
}

#[cfg(test)]
pub(in crate::effect_rows) fn assert_call_outcome(
    body: &str,
    env: &CallableEnv,
    summary: &super::Summary,
    returned: &Value,
) {
    let source = format!("fn fixture()=fn()=>{body}\n");
    let samples = super::field_rows::tests::fixture_function_values(&source, |_, expression| {
        assert_lambda_outcome(expression, env, summary, returned);
    });
    assert_eq!(samples.len(), 1);
}

#[cfg(test)]
fn assert_lambda_outcome(
    expression: &Expr,
    env: &CallableEnv,
    summary: &super::Summary,
    returned: &Value,
) {
    assert!(matches!(expression, Expr::Lambda { .. }));
    if let Expr::Lambda {
        parameters, body, ..
    } = expression
    {
        assert_eq!(parameters.as_slice(), []);
        super::identity_tests::with_empty_analyzer(|analyzer| {
            assert_original_outcome(&analyzer, expression, body, env, summary, returned);
        });
    }
}

#[cfg(test)]
fn assert_original_outcome(
    analyzer: &Analyzer<'_>,
    expression: &Expr,
    body: &Expr,
    env: &CallableEnv,
    summary: &super::Summary,
    returned: &Value,
) {
    let mut local = env.enter_lambda(&[]);
    assert_eq!(analyzer.expression(body, &[], &local), *summary);
    assert_eq!(
        analyzer.returned_value(body, &[], &mut local),
        Some(returned.clone())
    );
    let expected = function_outcome(returned.clone(), summary.clone());
    assert_eq!(analyzer.value(expression, &[], env), Some(expected));
}

#[cfg(test)]
#[test]
fn type_applied_named_forwarding_keeps_the_alarm_latent() {
    let prefix = "effect Alarm {ring:fn()->int}\ntype Packet={payload:List<int>,callback:fn()->int}\nfn source<T>(item:T,tag)=item\nfn wrapper()=fn()=>source<Packet>(tag:9,item:Packet{payload:[1,2],callback:fn()=>perform Alarm.ring()})\nlet saved=wrapper()\nlet packet=saved()\n";
    crate::effect_rows_tests::assert_accepted(&format!(
        "{prefix}let h=handler Alarm {{ring=>7}}\nlet answer=h(packet.callback)\n"
    ));
    crate::effect_rows_tests::assert_rejected_with(
        &format!("{prefix}let answer=packet.callback()\n"),
        &["unhandled effect operations", "Alarm.ring"],
    );
}
