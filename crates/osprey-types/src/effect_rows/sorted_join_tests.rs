//! Canonical alternative joins: exact semantic and deterministic work guards.
use super::super::identity_tests::with_empty_analyzer;
use super::super::model_values::returning_unknown;
use super::super::value_shapes::{project_callable, project_deep};
use super::*;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::thread::LocalKey;

std::thread_local! {
    static CLONES: Cell<Option<usize>> = const { Cell::new(None) };
    static COMPARISONS: Cell<Option<usize>> = const { Cell::new(None) };
    static VALUE_COMPARISONS: Cell<Option<usize>> = const { Cell::new(None) };
}

#[derive(Debug, Default, Eq)]
pub(crate) struct CloneProbe;

impl Clone for CloneProbe {
    fn clone(&self) -> Self {
        record(&CLONES);
        Self
    }
}

impl PartialEq for CloneProbe {
    fn eq(&self, _other: &Self) -> bool {
        record(&VALUE_COMPARISONS);
        true
    }
}

impl Ord for CloneProbe {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        record(&VALUE_COMPARISONS);
        std::cmp::Ordering::Equal
    }
}

impl PartialOrd for CloneProbe {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl CloneProbe {
    pub(crate) fn measured_value_comparisons<T>(run: impl FnOnce() -> T) -> (T, usize) {
        measured(&VALUE_COMPARISONS, run)
    }
}

pub(in crate::effect_rows) fn record(counter: &'static LocalKey<Cell<Option<usize>>>) {
    counter.with(|slot| {
        if let Some(count) = slot.get() {
            slot.set(Some(count.saturating_add(1)));
        }
    });
}

pub(super) fn record_comparison() {
    record(&COMPARISONS);
}

pub(in crate::effect_rows) fn measured<T>(
    counter: &'static LocalKey<Cell<Option<usize>>>,
    run: impl FnOnce() -> T,
) -> (T, usize) {
    counter.with(|slot| slot.set(Some(0)));
    let output = run();
    let count = counter.with(|slot| slot.replace(None));
    assert!(count.is_some(), "measurement counter must remain active");
    let count = count.into_iter().sum();
    (output, count)
}

pub(crate) fn measured_clones<T>(run: impl FnOnce() -> T) -> (T, usize) {
    measured(&CLONES, run)
}

fn parameter(index: usize) -> Callable {
    Callable::Parameter {
        level: 0,
        index,
        projection: Vec::new(),
    }
}

fn projected_parameter(index: usize) -> Callable {
    Callable::Parameter {
        level: 0,
        index,
        projection: vec![Projection::Field("callback".to_owned())],
    }
}

fn canonical(choices: impl IntoIterator<Item = Callable>) -> Callable {
    let mut choices: Vec<_> = choices.into_iter().collect();
    choices.sort();
    choices.dedup();
    Callable::Alternatives(choices)
}

fn flat_choices(callable: Callable) -> Vec<Callable> {
    match callable {
        Callable::Alternatives(choices) => choices,
        other => vec![other],
    }
}

fn reference_join(existing: Callable, incoming: Callable) -> Callable {
    if existing == incoming {
        return existing;
    }
    bounded_alternatives(
        [existing, incoming]
            .into_iter()
            .flat_map(flat_choices)
            .collect(),
    )
}

#[test]
fn projected_alternative_joins_do_not_resort_every_prefix() {
    let width = 64;
    let input = canonical((0..width).map(parameter));
    let expected = Value::from_callable(canonical((0..width).map(projected_parameter)));
    let mut projected = None;
    let ((), comparisons) = measured(&COMPARISONS, || {
        project_callable(
            Some(input),
            Projection::Field("callback".to_owned()),
            &mut projected,
        );
    });
    assert_eq!(projected, Some(expected));
    assert!(
        comparisons > 0,
        "ordering instrumentation must observe work"
    );
    eprintln!("alternative join ordering work: {comparisons} comparisons for {width} branches");
    assert!(
        comparisons <= width * 8,
        "{comparisons} ordering comparisons for {width} projected branches"
    );
}

#[test]
fn sorted_join_matches_the_existing_canonical_reference() {
    let cases = join_cases();
    for left in &cases {
        for right in &cases {
            let expected = reference_join(left.clone(), right.clone());
            let mut actual = Some(left.clone());
            merge_callable(&mut actual, right.clone());
            assert_eq!(actual, Some(expected));
        }
    }
}

fn join_cases() -> Vec<Callable> {
    vec![
        Callable::Unknown,
        parameter(0),
        parameter(1),
        projected_parameter(0),
        returning_unknown(0),
        returning_unknown(1),
        canonical([]),
        canonical([parameter(0), parameter(1)]),
        canonical([returning_unknown(0), Callable::Unknown]),
        canonical([
            canonical([parameter(0), parameter(1)]),
            returning_unknown(0),
        ]),
        canonical((0..=MAX_PROVENANCE_NODES).map(returning_unknown)),
    ]
}

#[test]
fn contained_choices_still_bound_an_oversized_stored_callable_set() {
    let existing = canonical((0..=MAX_PROVENANCE_NODES).map(returning_unknown));
    let expected = reference_join(existing.clone(), returning_unknown(0));
    let mut actual = Some(existing);
    merge_callable(&mut actual, returning_unknown(0));
    assert_eq!(actual, Some(expected));
    assert!(matches!(actual, Some(Callable::Alternatives(choices)) if choices.len() == 2));
}

#[test]
fn projected_joins_preserve_each_overflow_and_later_branches() {
    let base = canonical((0..MAX_PROVENANCE_NODES).map(parameter));
    let expected = (MAX_PROVENANCE_NODES..MAX_PROVENANCE_NODES + 3)
        .map(projected_parameter)
        .fold(base.clone(), reference_join);
    let input = canonical((MAX_PROVENANCE_NODES..MAX_PROVENANCE_NODES + 3).map(parameter));
    let mut projected = Some(Value::from_callable(base));
    project_callable(
        Some(input),
        Projection::Field("callback".to_owned()),
        &mut projected,
    );
    assert_eq!(projected, Some(Value::from_callable(expected)));
    assert!(
        matches!(projected.and_then(|value| value.callable), Some(Callable::Alternatives(choices)) if choices.len() == 4)
    );
}

#[cfg(test)]
#[test]
fn owned_alternative_merges_have_a_linear_value_clone_budget() {
    let width = 24;
    let mut expected: Vec<_> = (0..width).map(returning_unknown).collect();
    expected.sort();
    let (joined, clones) = measured_clones(|| {
        (0..width).fold(None, |mut slot, index| {
            merge_callable(&mut slot, returning_unknown(index));
            slot
        })
    });
    assert_eq!(joined, Some(Callable::Alternatives(expected)));
    assert!(matches!(joined, Some(Callable::Alternatives(choices)) if choices.len() == width));
    assert!(
        clones <= width * 2,
        "{clones} payload clones for {width} owned branches"
    );
}

#[test]
fn owned_transform_clone_probe_positive_control() {
    let value = Value::parameter(0, 1);
    let (copied, clones) = measured_clones(|| value.clone());
    assert_eq!(copied, value);
    assert!(
        clones > 0,
        "the clone probe must observe an actual payload copy"
    );
}

#[test]
fn owned_transform_maps_without_cloning_branch_payloads() {
    let mut choices: Vec<_> = (0..24).map(returning_unknown).collect();
    let mut expected = choices.clone();
    expected.sort();
    let ((), clones) = measured_clones(|| map_alternatives(&mut choices, |_| {}));
    assert_eq!(choices, expected);
    assert_eq!(clones, 0, "owned branch mapping copied {clones} payloads");
}

#[test]
fn owned_transform_mapping_still_resorts_dedups_and_loses_purity() {
    let mut choices: Vec<_> = (0..24).map(returning_unknown).collect();
    map_alternatives(&mut choices, |branch| branch.callable = None);
    assert_eq!(choices, vec![Callable::Unknown]);
}

fn parameter_with_child(child: Value) -> Callable {
    let arguments = super::super::CallArguments {
        positional: vec![Some(child)],
        ..super::super::CallArguments::default()
    };
    Callable::Parameter {
        level: 0,
        index: 0,
        projection: vec![Projection::Returned(Box::new(arguments))],
    }
}

#[test]
fn owned_transform_widening_takes_its_parameter_child() {
    let input = parameter_with_child(Value::parameter(0, 1));
    let expected = input.clone();
    let mut deferred = Summary::default();
    let (actual, clones) =
        measured_clones(|| widen_callable(input, 0, MAX_PROVENANCE_NODES, &mut deferred));
    assert_eq!(actual, expected);
    assert_eq!(deferred, Summary::default());
    assert_eq!(clones, 0, "owned child widening copied {clones} payloads");
}

#[test]
fn owned_transform_widening_preserves_depth_and_budget() {
    for (depth, budget) in [
        (0, MAX_PROVENANCE_NODES),
        (MAX_PROVENANCE_DEPTH - 1, MAX_PROVENANCE_NODES),
        (0, 0),
        (0, 1),
    ] {
        assert_child_widening(depth, budget);
    }
}

fn assert_child_widening(depth: usize, budget: usize) {
    let child = Value::parameter(1, 3);
    let expected_child = widen_value_budget(child.clone(), depth + CALL_NESTING_STEP, budget);
    let expected = parameter_with_child(expected_child);
    let mut deferred = Summary::default();
    let actual = widen_callable(parameter_with_child(child), depth, budget, &mut deferred);
    assert_eq!(actual, expected);
    assert_eq!(deferred, Summary::default());
}

fn deep_parameter(index: usize) -> Callable {
    Callable::Parameter {
        level: 0,
        index,
        projection: vec![Projection::Deep],
    }
}

fn deep_symbol(index: usize) -> Value {
    Value::from_callable(deep_parameter(index))
}

#[test]
fn deep_composition_preserves_symbolic_substitution_marker() {
    with_empty_analyzer(|analyzer| {
        let actual =
            analyzer.substitute_value_at(deep_symbol(0), 0, &[Some(Value::parameter(0, 1))]);
        assert_eq!(actual, deep_symbol(1));
    });
}

fn deep_callback() -> Callable {
    Callable::Known(Box::new(KnownCallable {
        parameters: Vec::new(),
        summary: Summary {
            required: [super::super::Requirement::new("Audit", "write", Vec::new())]
                .into_iter()
                .collect(),
            ..Summary::default()
        },
        returned: None,
    }))
}

fn callback_tree() -> Value {
    Value {
        fields: [(
            "nested".to_owned(),
            Value {
                element: Some(Box::new(Value::from_callable(deep_callback()))),
                ..Value::default()
            },
        )]
        .into_iter()
        .collect(),
        ..Value::default()
    }
}

#[test]
fn deep_composition_recovers_effect_after_two_symbolic_hops() {
    with_empty_analyzer(|analyzer| {
        let first =
            analyzer.substitute_value_at(deep_symbol(0), 0, &[Some(Value::parameter(0, 1))]);
        let second = analyzer.substitute_value_at(first, 0, &[None, Some(Value::parameter(0, 2))]);
        let actual = analyzer.substitute_value_at(second, 0, &[None, None, Some(callback_tree())]);
        assert_eq!(actual.callable, Some(deep_callback()));
    });
}

#[test]
fn deep_composition_retains_mixed_alternative_symbols_and_known_leaf() {
    let choices = canonical([
        parameter(0),
        parameter(1),
        deep_callback(),
        Callable::Unknown,
    ]);
    let input = Value::from_callable(choices);
    let actual = project_deep(&input);
    let expected = canonical([
        deep_parameter(0),
        deep_parameter(1),
        deep_callback(),
        Callable::Unknown,
    ]);
    assert_eq!(actual, Value::from_callable(expected));
}

#[test]
fn deep_projection_does_not_clone_discarded_container_subtrees() {
    let depth = 24;
    let tree = (0..depth).fold(Value::default(), |child, site| Value {
        element: Some(Box::new(child)),
        channel_sites: BTreeSet::from([site]),
        ..Value::default()
    });
    let expected = Value {
        channel_sites: (0..depth).collect(),
        ..Value::default()
    };
    let (actual, clones) = measured_clones(|| project_deep(&tree));
    assert_eq!(actual, expected);
    assert!(
        clones <= depth + 1,
        "{clones} clones for {depth} containers"
    );
}

#[test]
fn symbolic_deep_projection_composes_and_is_idempotent() {
    let expected = Value::from_callable(Callable::Parameter {
        level: 1,
        index: 2,
        projection: vec![Projection::Deep],
    });
    assert_eq!(project_deep(&Value::parameter(1, 2)), expected);
    assert_eq!(project_deep(&expected), expected);
}

#[test]
fn symbolic_deep_projection_keeps_known_closure_effects() {
    let required = super::super::Requirement::new("Alarm", "ring", Vec::new());
    let known = Value {
        callable: Some(known_effects_callable(Summary {
            required: [required].into_iter().collect(),
            ..Summary::default()
        })),
        ..Value::default()
    };
    assert_eq!(project_deep(&known), known);
}

#[test]
fn exact_projection_join_preserves_concrete_fields_beside_deep_summary() {
    with_empty_analyzer(|analyzer| {
        let child = callback_tree();
        let input = Value {
            fields: [("callback".to_owned(), child.clone())]
                .into_iter()
                .collect(),
            ..Value::default()
        };
        let mut expected = project_deep(&input);
        expected.union(child);
        let joined = joined_projection_value(deep_parameter(0), projected_parameter(0));
        let actual = analyzer.substitute_value_at(joined, 0, &[Some(input)]);
        assert_eq!(actual, expected);
    });
}

fn field_deep(field: &str) -> Callable {
    Callable::Parameter {
        level: 0,
        index: 0,
        projection: vec![Projection::Field(field.to_owned()), Projection::Deep],
    }
}

fn selected_callback_input() -> Value {
    let pure = Value::from_callable(returning_unknown(0));
    Value {
        fields: [
            ("left".to_owned(), pure.clone()),
            ("right".to_owned(), pure),
            (
                "unselected".to_owned(),
                Value::from_callable(deep_callback()),
            ),
        ]
        .into_iter()
        .collect(),
        ..Value::default()
    }
}

#[test]
fn exact_projection_join_does_not_collect_unselected_sibling_effect() {
    with_empty_analyzer(|analyzer| {
        let joined = joined_projection_value(field_deep("left"), field_deep("right"));
        let actual = analyzer.substitute_value_at(joined, 0, &[Some(selected_callback_input())]);
        assert_eq!(actual.callable, Some(returning_unknown(0)));
    });
}

fn joined_projection_value(left: Callable, right: Callable) -> Value {
    let mut callable = Some(left);
    merge_callable(&mut callable, right);
    Value {
        callable,
        ..Value::default()
    }
}
