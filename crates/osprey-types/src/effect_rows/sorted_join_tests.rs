//! Canonical alternative joins: exact semantic and deterministic work guards.
use super::super::value_shapes::project_callable;
use super::*;
use std::cell::Cell;
use std::thread::LocalKey;

std::thread_local! {
    static CLONES: Cell<Option<usize>> = const { Cell::new(None) };
    static COMPARISONS: Cell<Option<usize>> = const { Cell::new(None) };
}

#[derive(Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CloneProbe;

impl Clone for CloneProbe {
    fn clone(&self) -> Self {
        record(&CLONES);
        Self
    }
}

fn record(counter: &'static LocalKey<Cell<Option<usize>>>) {
    counter.with(|slot| {
        if let Some(count) = slot.get() {
            slot.set(Some(count.saturating_add(1)));
        }
    });
}

pub(super) fn record_comparison() {
    record(&COMPARISONS);
}

fn measured<T>(
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
    if let Some(merged) = merged_projection(&existing, &incoming) {
        return merged;
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
        returning_choice(0),
        returning_choice(1),
        canonical([]),
        canonical([parameter(0), parameter(1)]),
        canonical([returning_choice(0), Callable::Unknown]),
        canonical([canonical([parameter(0), parameter(1)]), returning_choice(0)]),
        canonical((0..=MAX_PROVENANCE_NODES).map(returning_choice)),
    ]
}

#[test]
fn contained_choices_still_bound_an_oversized_stored_callable_set() {
    let existing = canonical((0..=MAX_PROVENANCE_NODES).map(returning_choice));
    let expected = reference_join(existing.clone(), returning_choice(0));
    let mut actual = Some(existing);
    merge_callable(&mut actual, returning_choice(0));
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
fn returning_choice(index: usize) -> Callable {
    Callable::Known(Box::new(KnownCallable {
        parameters: vec![format!("parameter{index}")],
        summary: Summary::default(),
        returned: Some(Box::new(Value::unknown_value())),
    }))
}

#[cfg(test)]
#[test]
fn owned_alternative_merges_have_a_linear_value_clone_budget() {
    let width = 24;
    let mut expected: Vec<_> = (0..width).map(returning_choice).collect();
    expected.sort();
    let (joined, clones) = measured_clones(|| {
        (0..width).fold(None, |mut slot, index| {
            merge_callable(&mut slot, returning_choice(index));
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
