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
    let (mut actual, returned) = analyzer.function_body_model(function);
    actual.widen();
    let merge = history.merge_for(id);
    let mut changed = rows.get_mut(id).is_some_and(|row| {
        let before = (merge != Merge::Join).then(|| recomputed_part(row));
        let moved = row.merge_inferred_row(actual, analyzer.rows.get(id), merge);
        if let Some(before) = before {
            history.record_recomputed(id, &before, recomputed_part(row));
        }
        moved
    });
    if let (Some(returned), Some(slot)) = (returned, returns.get_mut(id)) {
        changed |= advance_return(slot, returned, id, history);
    }
    changed
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
/// sequence that revisits a value is joined with its predecessor and widened.
///
/// A function on a reference cycle can feed its own return back, and the
/// node budget makes widening non-monotone: cutting one subtree to unknown
/// frees budget for its siblings, so the proof kernel's mutually recursive
/// returns changed every sweep without ever revisiting. Those functions join
/// every move from the start (Kleene iteration); a function off every cycle
/// is a finite function of its callees and keeps recomputing precisely.
/// Should a joined sequence still revisit, widening has cycled, and the
/// return is pinned to its terminal shape, which only grows. No sweep is
/// counted: counts cannot tell recursion from finite dependency propagation
/// and cutting on one rejects valid acyclic callback chains.
/// Implements [EFFECTS-PROVENANCE].
///
/// A row's recomputed part (its callback uses and dynamic-call verdict)
/// follows the same rule: an acyclic function recomputes both each pass, a
/// recursive one re-derives the verdict over the uses it keeps, and either
/// whose recomputed part revisits an earlier state, or keeps moving for
/// [`RECOMPUTED_MOVES`] passes (functions that read each other's rows through
/// callbacks rather than calls, which recursion analysis cannot see), joins it
/// from then on. Joining only adds requirements and keeps a dynamic call
/// unresolved, so it never accepts what recomputing would reject.
#[derive(Default)]
pub(super) struct ReturnHistory {
    returns: HashMap<usize, Returns>,
    recursive: BTreeSet<usize>,
    recomputed: HashMap<usize, Recomputed>,
}

/// How [`Summary::merge_inferred_row`] treats a row's recomputed part.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Merge {
    /// Acyclic: callback uses and the dynamic verdict are derived afresh.
    Recompute,
    /// Recursive: uses from earlier traversals of the cycle are kept.
    RetainUses,
    /// Seen to cycle: uses are kept and an unresolved dynamic call stays so.
    Join,
}

/// A recomputed row part that moves this many passes has stopped converging.
const RECOMPUTED_MOVES: usize = 16;

/// The part of a row derived afresh each pass: the dynamic verdict, and an acyclic function's uses.
/// Held as a fingerprint, so tracking it never copies the row's values.
type RecomputedPart = (bool, u64);

fn recomputed_part(row: &Summary) -> RecomputedPart {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", row.parameter_uses).hash(&mut hasher);
    (row.unresolved_dynamic_call, hasher.finish())
}

/// One function's recomputed row parts so far, how often they moved, and
/// whether they cycled.
#[derive(Default)]
struct Recomputed {
    seen: BTreeSet<RecomputedPart>,
    moves: usize,
    joined: bool,
}

/// One function's returns so far, whether they have cycled, the joined
/// returns since, and whether those cycled too.
#[derive(Default)]
struct Returns {
    seen: BTreeSet<Value>,
    joined: bool,
    joined_seen: BTreeSet<Value>,
    terminal: bool,
}

impl ReturnHistory {
    /// A history for one fixed point whose `recursive` functions can feed
    /// their own returns back ([`super::recursion::recursive_functions`]).
    pub(super) fn new(recursive: BTreeSet<usize>) -> Self {
        Self {
            returns: HashMap::new(),
            recursive,
            recomputed: HashMap::new(),
        }
    }

    /// How `id`'s row merges its recomputed part this pass.
    fn merge_for(&self, id: usize) -> Merge {
        if self.recomputed.get(&id).is_some_and(|part| part.joined) {
            Merge::Join
        } else if self.recursive.contains(&id) {
            Merge::RetainUses
        } else {
            Merge::Recompute
        }
    }

    /// Note `id`'s recomputed row part; returning to an earlier state has cycled.
    fn record_recomputed(&mut self, id: usize, before: &RecomputedPart, after: RecomputedPart) {
        if *before == after {
            return;
        }
        let part = self.recomputed.entry(id).or_default();
        part.moves += 1;
        part.joined |= !part.seen.insert(after) || part.moves >= RECOMPUTED_MOVES;
    }

    fn settle(&mut self, id: usize, before: Option<&Value>, computed: Value) -> Value {
        let recursive = self.recursive.contains(&id);
        let returns = self.returns.entry(id).or_default();
        let revisited = returns.seen.contains(&computed);
        returns.joined |= before != Some(&computed) && revisited;
        returns.joined |= recursive && before.is_some();
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
        if !self.terminal {
            let widened = joined.clone().widened().widened();
            self.terminal = before != Some(&widened) && !self.joined_seen.insert(widened.clone());
            if !self.terminal {
                return widened;
            }
        }
        super::provenance::terminal_value(joined)
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
    for statement in statements {
        bootstrap.statement_environment(statement, &[], &mut env);
    }
    env
}

/// Least fixed point over every function's row and return provenance:
/// recursive and forward calls only add requirements. Each sweep analyses
/// only the functions whose inputs moved ([`super::worklist`]).
pub(super) fn converge(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    rows: &mut Vec<Summary>,
    returns: &mut Vec<Option<Value>>,
) {
    let solve = Solve {
        index,
        instances,
        statements,
    };
    let mut history = ReturnHistory::new(super::recursion::recursive_functions(index));
    let mut reads = vec![BTreeSet::new(); index.functions.len()];
    let mut dirty: BTreeSet<usize> = (0..index.functions.len()).collect();
    while !dirty.is_empty() {
        dirty = solve.sweep(&dirty, (rows, returns), &mut history, &mut reads);
    }
}

/// The program one fixed point solves.
struct Solve<'a> {
    index: &'a Index<'a>,
    instances: &'a Instances,
    statements: &'a [Stmt],
}

impl Solve<'_> {
    /// Re-analyse `dirty` against the current state, then write the new state
    /// back and return the functions the next sweep must re-analyse.
    fn sweep(
        &self,
        dirty: &BTreeSet<usize>,
        (rows, returns): (&mut Vec<Summary>, &mut Vec<Option<Value>>),
        history: &mut ReturnHistory,
        reads: &mut [BTreeSet<usize>],
    ) -> BTreeSet<usize> {
        let (index, instances) = (self.index, self.instances);
        let (file_scope, file_reads) = super::worklist::recording(|| {
            file_scope_env(index, instances, self.statements, rows, returns)
        });
        let analyzer = Analyzer {
            index,
            rows,
            returns,
            instances,
            file_scope,
        };
        let mut next = (rows.clone(), returns.clone());
        let moved = self.advance(&analyzer, dirty, &mut next, history, reads);
        // `analyzer` borrows `rows`/`returns`; its last use is the advance
        // above, so the borrow has ended by the time the new state is written.
        (*rows, *returns) = next;
        super::worklist::affected(&moved, &file_reads, reads)
    }

    /// Advance each dirty function one iteration, recording what it read;
    /// returns the functions that moved.
    fn advance(
        &self,
        analyzer: &Analyzer<'_>,
        dirty: &BTreeSet<usize>,
        (rows, returns): &mut (Vec<Summary>, Vec<Option<Value>>),
        history: &mut ReturnHistory,
        reads: &mut [BTreeSet<usize>],
    ) -> BTreeSet<usize> {
        let mut moved = BTreeSet::new();
        for &id in dirty {
            let Some(function) = self.index.functions.get(id) else {
                continue;
            };
            let (changed, read) = super::worklist::recording(|| {
                advance_function(analyzer, id, function, rows, returns, history)
            });
            if let Some(slot) = reads.get_mut(id) {
                *slot = read;
            }
            if changed {
                let _ = moved.insert(id);
            }
        }
        moved
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
