//! Effect row identity tests.
use super::*;
use crate::effect_rows_tests::{assert_accepted, diagnostics};

#[test]
fn opaque_structure_is_provenance_even_without_a_callable_component() {
    assert!(Value {
        unknown_structure: true,
        ..Value::default()
    }
    .carries_provenance());
}

#[test]
fn a_declared_field_row_remains_usable_through_a_typed_opaque_parameter() {
    assert_accepted(
        "type Control = { press: fn(string) -> int ![] }\n\
        extern fn foreignControl() -> Control\n\
        fn through(control: Control) = control.press(\"x\")\n\
        let answer = through(foreignControl())\n",
    );
}

const COMPETING_FIELD_ROWS: &str = "effect Alarm { ring: fn() -> int }\ntype Control = { press: fn(string) -> int ![] }\ntype Other = { press: fn(string) -> int !Alarm }\n";

#[test]
fn an_extracted_field_keeps_its_declared_owners_row() {
    let source = format!("{COMPETING_FIELD_ROWS}let control = Control {{ press: fn(text) => 1 }}\nlet callback = control.press\nlet answer = callback(\"x\")\n");
    assert_accepted(&source);
}

#[test]
fn an_extracted_opaque_field_keeps_its_declared_owners_row() {
    let source = format!("{COMPETING_FIELD_ROWS}extern fn foreignControl() -> Control\nfn through(control: Control) = {{\nlet callback = control.press\ncallback(\"x\")\n}}\nlet answer = through(foreignControl())\n");
    assert_accepted(&source);
}

#[test]
fn an_opaque_anonymous_field_cannot_borrow_an_unrelated_types_row() {
    for invocation in [
        "let answer = other.press(\"x\")\n",
        "let callback = other.press\nlet answer = callback(\"x\")\n",
    ] {
        let source = format!("type Control = {{ press: fn(string) -> int ![] }}\nextern fn foreignCallback() -> fn(string) -> int\nlet other = {{ press: foreignCallback() }}\n{invocation}");
        crate::effect_rows_tests::assert_rejected_with(
            &source,
            &["dynamic callable whose effect provenance cannot be proven"],
        );
    }
}

#[test]
fn a_transform_losing_a_callable_cannot_keep_its_old_purity_proof() {
    let mut choices = vec![requirement_callable(0)];
    provenance::map_alternatives(&mut choices, |branch| branch.callable = None);
    assert_eq!(choices, vec![Callable::Unknown]);
}

#[test]
fn wide_callable_joins_are_bounded_without_losing_required_operations() {
    let choices = (0..=MAX_PROVENANCE_NODES)
        .map(requirement_callable)
        .collect();
    let value = Value::from_callable(Callable::Alternatives(choices)).widened();
    assert!(callable_width(&value) <= MAX_PROVENANCE_NODES);
    let summary = with_empty_analyzer(|analyzer| {
        value.callable.map_or_else(Summary::default, |callee| {
            analyzer.invoke_with_values(callee, &[None])
        })
    });
    assert_eq!(summary.required.len(), MAX_PROVENANCE_NODES + 1);
}

fn requirement_callable(index: usize) -> Callable {
    Callable::Known(Box::new(KnownCallable {
        parameters: vec![format!("parameter{index}")],
        summary: Summary {
            required: [Requirement::new("Alarm", &format!("op{index}"), Vec::new())]
                .into_iter()
                .collect(),
            ..Summary::default()
        },
        returned: None,
    }))
}

fn callable_width(value: &Value) -> usize {
    match &value.callable {
        Some(Callable::Alternatives(choices)) => choices.len(),
        Some(_) => 1,
        None => 0,
    }
}

pub(super) fn with_empty_analyzer<T>(operation: impl FnOnce(Analyzer<'_>) -> T) -> T {
    let index = Index::default();
    let instances = Instances::default();
    operation(Analyzer {
        index: &index,
        rows: &[],
        returns: &[],
        instances: &instances,
        file_scope: CallableEnv::default(),
    })
}

#[test]
fn curried_declared_rows_cover_conditional_returned_functions() {
    for (flavor, source) in [
        (osprey_syntax::Flavor::Default, "effect F { go: fn(int) -> int }\nfn pair(a) ![] = fn(b) => if b > 0 { fn(c) => perform F.go(satAdd(a, c)) } else { fn(c) => c }\n"),
        (osprey_syntax::Flavor::Ml, "effect E\n    op : int => int\neffect F\n    go : int => int\npair : int -> int -> int -> int ! E\npair a b =\n    match b > 0\n        true => \\c => perform F.go (satAdd a c)\n        false => \\c => c\n"),
    ] {
        crate::testutil::rejects_with(flavor, source, "function `pair` performs effects outside its declared row: F.go");
    }
}

#[test]
fn a_curried_pure_factory_does_not_invoke_a_callback_erased_to_data() {
    assert_accepted(
        "effect F { go: fn(int) -> int }\n\
        fn pair(a) ![] = fn(b) => { let value: any = fn(c) => perform F.go(c)\nvalue }\n",
    );
}

#[test]
fn extern_and_function_lookup_share_lexical_precedence() {
    let parsed = osprey_syntax::parse_program("namespace Other { fn read(index:int)=index }\nnamespace Near { extern fn read(index:int)->int }\n");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let index = Index::collect(&parsed.program);
    assert_eq!(index.resolve(&["Near".into()], "read"), None);
    assert_eq!(
        index.extern_parameters(&["Near".into()], "read"),
        Some(&vec!["index".into()])
    );
    assert_eq!(index.resolve(&["Other".into()], "read"), Some(0));
    assert_eq!(index.extern_parameters(&["Other".into()], "read"), None);
}

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

#[test]
fn statically_known_scalar_extern_aliases_preserve_callable_provenance() {
    for call in ["reader(1)", "{let forward=reader\nforward(1)}"] {
        assert_accepted(&format!(
            "extern fn read(index:int)->string\nlet reader=read\nlet answer={call}\n"
        ));
    }
}

#[test]
fn foreign_callbacks_and_shadowed_externs_keep_unknown_provenance() {
    for source in [
        "extern fn read(callback:()->int)->int\nlet reader=read\nlet answer=reader(fn()=>1)\n",
        "extern fn read()->()->int\nlet reader=read\nlet answer=reader()()\n",
        "extern fn read(index:int)->string\nextern fn factory()->(int)->string\nlet read=factory()\nlet answer=read(1)\n",
    ] {
        assert_unknown_callable(source);
    }
}

#[test]
fn foreign_callback_imports_cannot_inherit_builtin_provenance() {
    for name in ["sqrt", "print", "abs"] {
        let prefix = format!(
            "effect Probe {{value:fn()->int}}\nextern fn {name}(callback:fn()->int)->int\n"
        );
        assert_unknown_callable(&format!(
            "{prefix}let callForeign={name}\nlet result=callForeign(fn()=>perform Probe.value())\n"
        ));
        assert_eq!(
            diagnostics(&format!("{prefix}let result={name}(fn()=>perform Probe.value())\n")),
            vec![TypeError::new("unhandled effect operations at program entry: Probe.value; add a matching `handle`")],
        );
    }
}

fn assert_unknown_callable(source: &str) {
    assert!(
        diagnostics(source).iter().any(|error| error
            .message
            .contains("dynamic callable whose effect provenance cannot be proven")),
        "{source}"
    );
}

#[test]
fn structural_patterns_use_the_receivers_field_contract() {
    let prefix = "type Control={press:fn(string)->int ![]}\n";
    assert_unknown_callable(&format!(
        "{prefix}extern fn callback()->fn(string)->int\nlet other={{press:callback()}}\nlet answer=match other {{{{press}}=>press(\"x\")}}"
    ));
    for other in [
        "type Other={press:fn(string)->int}",
        "effect Alarm {ring:fn()->int}\ntype Other={press:fn(string)->int ![Alarm]}",
    ] {
        assert_accepted(&format!(
            "{prefix}{other}\nextern fn control()->Control\nfn run(value:Control)=match value {{{{press}}=>press(\"x\")}}\nlet answer=run(control())"
        ));
    }
}

#[test]
fn generic_record_updates_enforce_each_instantiated_field_row() {
    let prefix = "effect Alarm {ring:fn()->int}\ntype Control={press:fn(string)->int ![]}\ntype Other={press:fn(string)->int ![Alarm]}\nfn change(control)=control {press:fn(_)=>perform Alarm.ring()}\nlet original=Control {press:fn(_)=>1}\nlet other=Other {press:fn(_)=>1}\n";
    for calls in ["let changed=change(original)", "let first=change(other)\nlet changed=change(original)", "let changed=change(original)\nlet last=change(other)", "let edit=change\nlet changed=edit(original)", "let cap=instance Alarm\nlet answer={handle cap {ring=>1}\nperform cap.ring()}\nlet changed=change(original)"] {
        crate::effect_rows_tests::assert_rejected_with(
            &format!("{prefix}{calls}"),
            &["field `press` of `Control` declares effect row ![], but the stored function performs Alarm.ring"],
        );
    }
    assert_accepted(&format!("{prefix}let permitted=change(other)"));
}

#[test]
fn a_field_row_must_match_the_stored_effect_instance() {
    crate::effect_rows_tests::assert_rejected_with(
        "effect Read<T> {get:fn()->T}\ntype Control={press:fn()->string ![Read<int>]}\nlet control=Control {press:fn()=>perform Read<string>.get()}",
        &["the stored function performs Read<string>.get"],
    );
}

#[test]
fn a_pure_field_contract_is_checked_without_effect_declarations() {
    crate::effect_rows_tests::assert_rejected_with(
        "type Control={press:fn()->string ![]}\nextern fn callback()->fn()->string\nlet control=Control {press:callback()}",
        &["field `press` of `Control` declares effect row ![], but the stored function calls a function whose effects cannot be proven"],
    );
}

#[test]
fn generic_field_projection_uses_the_actual_receivers_contract() {
    for body in ["value.press", "fn(text)=>value.press(text)"] {
        assert_accepted(&format!(
            "{COMPETING_FIELD_ROWS}extern fn control()->Control\nfn extract(value)={body}\nlet callback=extract(control())\nlet answer=callback(\"x\")"
        ));
    }
}

#[test]
fn generic_field_rows_are_instantiated_at_constructions_and_updates() {
    let prefix =
        "effect Read<T>{get:fn()->T}\ntype GenericControl<T>={press:fn()->int ![Read<T>]}\n";
    for value in [
        "let control=GenericControl<int> {press:fn()=>perform Read<int>.get()}",
        "let original=GenericControl<int> {press:fn()=>1}\nlet control=original {press:fn()=>perform Read<int>.get()}",
        "fn change(value)=value {press:fn()=>perform Read<int>.get()}\nlet original=GenericControl<int> {press:fn()=>1}\nlet control=change(original)",
    ] {
        assert_accepted(&format!("{prefix}{value}"));
    }
}

#[test]
fn generic_field_contracts_refuse_different_effect_instances() {
    let prefix =
        "effect Read<T>{get:fn()->T}\ntype GenericControl<T>={press:fn()->int ![Read<T>]}\n";
    for value in [
        "let control=GenericControl<string> {press:fn()=>perform Read<int>.get()}",
        "let original=GenericControl<string> {press:fn()=>1}\nlet control=original {press:fn()=>perform Read<int>.get()}",
    ] {
        crate::effect_rows_tests::assert_rejected_with(&format!("{prefix}{value}"), &["the stored function performs Read<int>.get"]);
    }
    crate::effect_rows_tests::assert_rejected_with(&format!(
        "{prefix}extern fn control()->GenericControl<int>\nlet answer={{handle Read<string> {{get=>\"x\"}}\ncontrol().press()}}"
    ), &["unhandled effect operations at program entry: Read<int>.get"]);
}

#[test]
fn generic_field_calls_use_the_actual_receivers_effect_instance() {
    let prefix = "effect Read<T>{get:fn()->T}\ntype GenericControl<T>={press:fn()->int ![Read<T>]}\nextern fn control()->GenericControl<int>\n";
    for invocation in ["control().press()", "extract(control())()"] {
        assert_accepted(&format!(
            "{prefix}fn extract(value)=value.press\nlet answer={{handle Read<int> {{get=>1}}\n{invocation}}}"
        ));
    }
}

#[test]
fn a_generic_field_row_preserves_a_nested_record_instance() {
    assert_accepted(
        "effect Read<T>{get:fn()->T}\ntype Box<T>={value:T}\ntype Control<T>={press:fn()->int ![Read<T>]}\nlet control=Control<Box<int>> {press:fn()=>(perform Read<Box<int>>.get()).value}",
    );
}

#[test]
fn generic_variant_field_rows_follow_the_matched_instance() {
    assert_accepted(
        "effect Read<T>{get:fn()->T}\ntype Event<T>=Empty|Action {press:fn()->int ![Read<T>]}\nextern fn event()->Event<int>\nfn run(value)=match value {Action {press}=>press() Empty=>0}\nlet answer={handle Read<int> {get=>1}\nrun(event())}",
    );
}

#[test]
fn inferred_named_record_arguments_use_the_canonical_effect_identity() {
    assert_accepted(
        "effect Read<T>{get:fn()->T}\ntype Point={value:int}\ntype Control<T>={sample:T,press:fn()->int ![Read<T>]}\nlet control=Control {sample:Point {value:1},press:fn()=>(perform Read<Point>.get()).value}",
    );
}

#[test]
fn generic_updates_preserve_every_instantiated_field_obligation() {
    let prefix = "effect Read<T>{get:fn()->T}\ntype Control<T>={press:fn()->int ![Read<T>]}\nfn change(value)=value {press:fn()=>length(perform Read<string>.get())}\nlet ints=Control<int> {press:fn()=>1}\nlet strings=Control<string> {press:fn()=>1}\n";
    for calls in [
        "let wrong=change(ints)\nlet allowed=change(strings)",
        "let allowed=change(strings)\nlet wrong=change(ints)",
    ] {
        crate::effect_rows_tests::assert_rejected_with(
            &format!("{prefix}{calls}"),
            &["the stored function performs Read<string>.get"],
        );
    }
    assert_accepted(&format!("{prefix}let allowed=change(strings)"));
}

#[test]
fn phantom_field_contract_instances_cannot_be_unified() {
    crate::effect_rows_tests::assert_rejected_with(
        "effect Read<T>{get:fn()->T}\ntype Control<T>={press:fn()->int ![Read<T>]}\nfn requireInt(control:Control<int>)=1\nlet answer=requireInt(Control<string> {press:fn()=>1})",
        &["type mismatch"],
    );
}

#[test]
fn owned_substitution_has_a_linear_value_clone_budget() {
    let depth = 24;
    let value = (0..depth).fold(Value::unknown_value(), |child, _| Value {
        fields: [("child".to_owned(), child)].into_iter().collect(),
        ..Value::default()
    });
    let expected = value.clone();
    let (actual, clones) = provenance::measured_clones(|| {
        with_empty_analyzer(|analyzer| analyzer.substitute_value_at(value, 0, &[]))
    });
    assert_eq!(actual, expected);
    assert!(
        clones <= depth * 2,
        "{clones} value clones for {depth} owned nodes"
    );
}
