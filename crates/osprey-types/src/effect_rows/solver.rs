//! Effect row solver.
use super::{
    map_parameter_use_values, map_projection_values, Analyzer, BTreeSet, Callable, CallableEnv,
    Expr, Function, HashMap, Index, Instances, Stmt, Summary, Value,
};

/// Advance one function's inferred row and return provenance by a single
/// iteration, reporting whether either moved.
pub(super) fn advance_function(
    analyzer: &Analyzer<'_>,
    id: usize,
    function: &Function<'_>,
    rows: &mut [Summary],
    returns: &mut [Option<Value>],
    history: &mut ReturnHistory,
) -> bool {
    let actual = normalized_body(analyzer, function);
    let mut changed = rows
        .get_mut(id)
        .is_some_and(|row| row.merge_inferred_row(actual, analyzer.rows.get(id)));
    if let (Some(returned), Some(slot)) = (analyzer.function_return(function), returns.get_mut(id))
    {
        changed |= advance_return(slot, returned, id, history);
    }
    changed
}

fn normalized_body(analyzer: &Analyzer<'_>, function: &Function<'_>) -> Summary {
    let mut actual = analyzer.function_body(function);
    actual.widen();
    actual
}

// Recompute the complete current answer. Fold cuts within-pass feedback;
// ReturnHistory joins only actual cycles. Never union superseded returns.
fn advance_return(
    slot: &mut Option<Value>,
    returned: Value,
    id: usize,
    history: &mut ReturnHistory,
) -> bool {
    let before = slot.take();
    let computed = returned.widened().widened();
    *slot = Some(history.settle(id, before.as_ref(), computed));
    *slot != before
}

/// Every return value each function has had during one fixed point, and
/// which functions' returns have been seen to cycle. Recomputing a return
/// each pass keeps it precise, but nothing makes that sequence monotone: a
/// walk that hands its whole node to a helper falling back to the walk
/// flipped between two returns forever. Widening bounds the values, so a
/// sequence that never settles must revisit one it already left; from then
/// on that function's return is joined with its previous one, which only
/// grows and so converges. Implements [EFFECTS-PROVENANCE].
#[derive(Default)]
pub(super) struct ReturnHistory(HashMap<usize, Returns>);

/// One function's returns so far, and whether they have cycled.
#[derive(Default)]
struct Returns {
    seen: BTreeSet<Value>,
    joined: bool,
}

impl ReturnHistory {
    fn settle(&mut self, id: usize, before: Option<&Value>, computed: Value) -> Value {
        let returns = self.0.entry(id).or_default();
        let revisited = returns.seen.contains(&computed);
        returns.joined |= before != Some(&computed) && revisited;
        if !returns.joined {
            if !revisited {
                let _ = returns.seen.insert(computed.clone());
            }
            return computed;
        }
        let mut joined = before.cloned().unwrap_or_default();
        joined.union(computed);
        joined.widened().widened()
    }
}

/// The provenance environment the file-scope statements establish, threaded in
/// execution order so a later `let` sees an earlier one.
///
/// Built with an empty file scope of its own: the top-level statements are the
/// very thing being summarised, and nothing at file scope can name a binding
/// that has not been walked yet, so one pass is a complete answer for the rows
/// it was given. The enclosing fixed point re-derives it as those rows sharpen.
pub(super) fn file_scope_env(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    rows: &[Summary],
    returns: &[Option<Value>],
) -> CallableEnv {
    let bootstrap = Analyzer {
        index,
        rows,
        returns,
        instances,
        file_scope: CallableEnv::default(),
    };
    let mut env = CallableEnv::default();
    let _ = bootstrap.statements(statements, &[], &mut env);
    env
}

/// Least fixed point over every function's row and return provenance:
/// recursive and forward calls only add requirements.
pub(super) fn converge(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    rows: &mut Vec<Summary>,
    returns: &mut Vec<Option<Value>>,
) {
    let mut history = ReturnHistory::default();
    loop {
        let analyzer = Analyzer {
            index,
            rows,
            returns,
            instances,
            file_scope: file_scope_env(index, instances, statements, rows, returns),
        };
        let mut next = rows.clone();
        let mut next_returns = returns.clone();
        let changed = index
            .functions
            .iter()
            .enumerate()
            .fold(false, |changed, (id, function)| {
                advance_function(
                    &analyzer,
                    id,
                    function,
                    &mut next,
                    &mut next_returns,
                    &mut history,
                ) || changed
            });
        // `analyzer` borrows `rows`/`returns`; its last use is the fold above,
        // so the borrow has ended by the time the new state is written back.
        *rows = next;
        *returns = next_returns;
        if !changed {
            return;
        }
    }
}

/// Erase the provenance verdicts a structural sweep recorded, leaving the
/// requirements, parameter uses and callable shapes that carry them intact so
/// the next sweep can re-derive each verdict from converged provenance.
/// What a declared row governs: the function's own requirements plus, along a
/// curried spine (a body that is a closure literal, as ML `f a b = …` lowers),
/// each nested closure's latent requirements. The written row covers the full
/// application ([FLAVOR-ML-CURRY]).
pub(super) fn full_application(
    body: &Expr,
    own: &Summary,
    returned: Option<&Value>,
    ty: Option<&crate::ty::Type>,
) -> Summary {
    let mut summary = own.clone();
    if curried_body(body).is_some() {
        if let Some(callable) = returned.and_then(|value| value.callable.as_ref()) {
            application_callable(callable, ty, &mut summary);
        }
    }
    summary
}

fn application_callable(callable: &Callable, ty: Option<&crate::ty::Type>, summary: &mut Summary) {
    if ty.is_some_and(|ty| !matches!(ty, crate::ty::Type::Fun { .. } | crate::ty::Type::Var(_))) {
        return;
    }
    match callable {
        Callable::Known(known) => known_application(known, ty, summary),
        Callable::Alternatives(choices) => {
            for choice in choices {
                application_callable(choice, ty, summary);
            }
        }
        Callable::Parameter { .. } | Callable::Unknown => {
            if matches!(ty, Some(crate::ty::Type::Fun { .. })) {
                summary.unresolved_dynamic_call = true;
            }
        }
    }
}

fn known_application(
    known: &super::KnownCallable,
    ty: Option<&crate::ty::Type>,
    summary: &mut Summary,
) {
    summary.union(known.summary.clone());
    let next = ty.and_then(|ty| match ty {
        crate::ty::Type::Fun { ret, .. } => Some(ret.as_ref()),
        _ => None,
    });
    if let Some(callable) = known
        .returned
        .as_deref()
        .and_then(|value| value.callable.as_ref())
    {
        application_callable(callable, next, summary);
    }
}

/// The body of the closure literal a curried spine returns, looking through
/// the parameter-annotation `let`s the ML lowering wraps around it.
pub(super) fn curried_body(body: &Expr) -> Option<&Expr> {
    match body {
        Expr::Lambda { body, .. } => Some(body),
        Expr::Block {
            statements,
            value: Some(value),
            ..
        } if statements.iter().all(is_parameter_annotation) => curried_body(value),
        _ => None,
    }
}

/// `let x: T = x` — an annotation on a parameter, not a computation.
pub(super) fn is_parameter_annotation(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Let {
        name,
        mutable: false,
        ty: Some(_),
        value: Expr::Identifier(bound),
        ..
    } if name == bound)
}

pub(super) fn clear_verdicts(rows: &mut [Summary], returns: &mut [Option<Value>]) {
    for row in rows.iter_mut() {
        clear_summary_verdicts(row);
    }
    for returned in returns.iter_mut().flatten() {
        clear_value_verdicts(returned);
    }
}

pub(super) fn clear_summary_verdicts(summary: &mut Summary) {
    summary.unresolved_dynamic_call = false;
    summary.parameter_uses = std::mem::take(&mut summary.parameter_uses)
        .into_iter()
        .map(|mut use_| {
            map_parameter_use_values(&mut use_, clear_value_verdicts);
            use_
        })
        .collect();
}

pub(super) fn clear_value_verdicts(value: &mut Value) {
    if let Some(Callable::Alternatives(choices)) = &mut value.callable {
        super::provenance::map_alternatives(choices, clear_value_verdicts);
    }
    clear_summary_verdicts(&mut value.deferred);
    // `Callable::Unknown` is deliberately left in place: it is the SHAPE that
    // makes an invocation unresolvable, so the next sweep raises the verdict
    // again wherever the callable is actually called.
    if let Some(Callable::Known(known)) = &mut value.callable {
        clear_summary_verdicts(&mut known.summary);
        if let Some(returned) = &mut known.returned {
            clear_value_verdicts(returned);
        }
    }
    if let Some(Callable::Parameter { projection, .. }) = &mut value.callable {
        map_projection_values(projection, clear_value_verdicts);
    }
    for nested in value.container_children_mut() {
        clear_value_verdicts(nested);
    }
}

#[cfg(test)]
mod tests {
    use super::super::model_values::{
        measure_snapshot_step, snapshot_function, snapshot_row, snapshot_value,
        with_snapshot_analyzer,
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
}
