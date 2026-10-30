//! Effect row solver return-history tests.
use super::super::model_values::{
    measure_snapshot_step, snapshot_function, snapshot_row, snapshot_value, with_snapshot_analyzer,
};
use super::{ReturnHistory, Value};

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
        .0
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
            actual.0.get(&id).map(|row| row.joined),
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
        super::Summary::default(),
        Some(snapshot_value()),
        |analyzer, mut state| {
            let body = super::Expr::Bool(false);
            let mut history = ReturnHistory::default();
            let expected = analyzer.function_return(&snapshot_function(&body));
            let step = measure_snapshot_step(&analyzer, &body, &mut state, &mut history);
            assert_eq!(state.1, [expected]);
            assert_eq!(history.0.get(&0).map(|row| row.joined), Some(false));
            assert_eq!(step, (true, 1), "old return snapshot must move");
        },
    );
}

#[test]
fn snapshot_row_preserves_fresh_verdict_and_other_metadata_without_copying_before() {
    with_snapshot_analyzer(snapshot_row(), None, |analyzer, mut state| {
        let body = super::Expr::Block {
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
        assert!(history.0.is_empty());
        assert_eq!(step, (true, 0), "old row snapshot must be borrowed");
    });
}
