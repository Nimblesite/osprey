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
///
/// Waiting for an exact revisit is not enough on its own: mutually recursive
/// descents (the proof kernel's `infer`/`reduce` family) produce a fresh return
/// on every sweep, and widening's per-child budget is not monotone under
/// union, so even a joined return can shrink and regrow. So the history also
/// counts sweeps: past [`SWEEPS_BEFORE_JOIN`], every return that still moves
/// is joined, and a joined return that moves [`JOINED_MOVES_BEFORE_CUT`] more
/// times is cut to its terminal shape — the same over-approximation the depth
/// limit already applies — which bounds the fixed point.
#[derive(Default)]
pub(super) struct ReturnHistory(HashMap<usize, Returns>, usize);

/// Sweeps a fixed point may spend recomputing returns precisely before every
/// return still moving is joined with its predecessor.
const SWEEPS_BEFORE_JOIN: usize = 8;

/// Moves a joined return may still make before it is cut to terminal shape.
const JOINED_MOVES_BEFORE_CUT: usize = 4;

/// One function's returns so far, whether they have cycled, and how often the
/// joined return has moved since.
#[derive(Default)]
struct Returns {
    seen: BTreeSet<Value>,
    joined: bool,
    joined_moves: usize,
}

impl ReturnHistory {
    /// Record that the enclosing fixed point has begun another sweep.
    fn sweep(&mut self) {
        self.1 = self.1.saturating_add(1);
    }

    fn settle(&mut self, id: usize, before: Option<&Value>, computed: Value) -> Value {
        let overran = self.1 > SWEEPS_BEFORE_JOIN;
        let returns = self.0.entry(id).or_default();
        let revisited = returns.seen.contains(&computed);
        returns.joined |= before != Some(&computed) && (revisited || overran);
        if !returns.joined {
            if !revisited {
                let _ = returns.seen.insert(computed.clone());
            }
            return computed;
        }
        returns.join(before, computed)
    }
}

impl Returns {
    fn join(&mut self, before: Option<&Value>, computed: Value) -> Value {
        let mut joined = before.cloned().unwrap_or_default();
        joined.union(computed);
        let joined = joined.widened().widened();
        if before == Some(&joined) {
            return joined;
        }
        self.joined_moves = self.joined_moves.saturating_add(1);
        if self.joined_moves > JOINED_MOVES_BEFORE_CUT {
            return super::provenance::terminal_value(joined);
        }
        joined
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
        history.sweep();
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
#[path = "settle_tests.rs"]
mod settle_tests;

#[cfg(test)]
#[path = "solver_tests.rs"]
mod tests;
