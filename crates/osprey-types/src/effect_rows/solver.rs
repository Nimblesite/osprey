//! Effect row solver.
use super::{
    map_parameter_use_values, map_projection_values, Analyzer, Callable, CallableEnv, Expr,
    Function, HashMap, Index, Instances, Stmt, Summary, Value,
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
    let mut changed = false;
    let mut actual = analyzer.function_body(function);
    actual.widen();
    if let Some(row) = rows.get_mut(id) {
        let before = row.clone();
        // Requirements and parameter uses only grow toward the least fixed
        // point, so unioning them is what makes this converge. The provenance
        // verdict is not of that kind — see `check` — so take the freshly
        // computed one. `widen` still re-raises it when it drops a use.
        let resolved = actual.unresolved_dynamic_call;
        row.union(actual);
        row.unresolved_dynamic_call = resolved;
        row.widen();
        changed |= *row != before;
    }
    if let Some(returned) = analyzer.function_return(function) {
        if let Some(slot) = returns.get_mut(id) {
            let before = slot.clone();
            // Recompute rather than merge with the previous iteration.
            // `function_return` already merges every return path of the body,
            // so each pass yields a COMPLETE answer for the rows it was given;
            // unioning across passes only preserves superseded ones. The one
            // feedback a pass cannot complete on its own — `fold`'s accumulator
            // carrying its callback's return — is cut in `fold_value`, which
            // stabilises that contribution instead of unrolling it. A return
            // that cycles instead of settling is joined; see `ReturnHistory`.
            let computed = returned.widened().widened();
            *slot = Some(history.settle(id, before.as_ref(), computed));
            changed |= *slot != before;
        }
    }
    changed
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
    seen: Vec<Value>,
    joined: bool,
}

impl ReturnHistory {
    fn settle(&mut self, id: usize, before: Option<&Value>, computed: Value) -> Value {
        let returns = self.0.entry(id).or_default();
        returns.joined |= before != Some(&computed) && returns.seen.contains(&computed);
        if !returns.joined {
            if returns.seen.last() != Some(&computed) {
                returns.seen.push(computed.clone());
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
    use super::{ReturnHistory, Value};

    const FUNCTION: usize = 0;
    const STABLE_SWEEPS: usize = 512;

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
}
