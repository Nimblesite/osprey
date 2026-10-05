//! Effect row solver.
use super::{
    map_parameter_use_values, map_projection_values, Analyzer, Callable, CallableEnv, Expr,
    Function, Index, Instances, Stmt, Summary, Value,
};

/// Advance one function's inferred row and return provenance by a single
/// iteration, reporting whether either moved.
pub(super) fn advance_function(
    analyzer: &Analyzer<'_>,
    id: usize,
    function: &Function<'_>,
    rows: &mut [Summary],
    returns: &mut [Option<Value>],
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
            // unioning across passes only preserves superseded ones.
            *slot = Some(returned.widened().widened());
            changed |= *slot != before;
        }
    }
    changed
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
                advance_function(&analyzer, id, function, &mut next, &mut next_returns) || changed
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
pub(super) fn full_application(body: &Expr, own: &Summary, returned: Option<&Value>) -> Summary {
    let mut summary = own.clone();
    let (mut body, mut returned) = (body, returned);
    while let (Some(inner), Some(Callable::Known(closure))) = (
        curried_body(body),
        returned.and_then(|value| value.callable.as_ref()),
    ) {
        summary.union(closure.summary.clone());
        body = inner;
        returned = closure.returned.as_deref();
    }
    summary
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
    for nested in value.fields.values_mut() {
        clear_value_verdicts(nested);
    }
    for nested in [
        value.element.as_mut(),
        value.result_payload.as_mut(),
        value.fiber_payload.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        clear_value_verdicts(nested);
    }
}
