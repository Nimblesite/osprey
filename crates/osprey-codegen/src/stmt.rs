//! Statement lowering: the `let` / assignment / bare-expression forms that make
//! up a block body, the trailing top-level sequence, and the handler-owned
//! `mut` cells among them. Every one of these appears both inside a function
//! and at file scope, so they lower through exactly one path.

mod bindings;
mod cells;
use bindings::gen_bind;
use cells::{gen_cell_define, gen_cell_store, gen_global_store};

use crate::builder::{Codegen, FnSig};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter, Position, Program, Stmt};
use std::collections::BTreeSet;

/// Lower a statement inside its own ARC region: temporaries the statement
/// produced and did not bind drop at its end [GC-ARC-PERCEUS].
pub(crate) fn gen_local_stmt(cg: &mut Codegen, stmt: &Stmt) -> Result<()> {
    crate::arc::push_frame(cg);
    let lowered = gen_stmt_kind(cg, stmt);
    crate::arc::pop_frame(cg);
    lowered
}

fn gen_stmt_kind(cg: &mut Codegen, stmt: &Stmt) -> Result<()> {
    match stmt {
        // A `mut` an effect handler captures is promoted to a shared heap cell so
        // the handler owns it; its declaration allocates the cell and a
        // reassignment stores through it (reads `load` it, see `gen_expr`).
        Stmt::Let {
            name,
            value,
            mutable: true,
            position,
            ..
        } if cg.cell_vars.contains(name) => {
            with_stmt_debug(cg, *position, |cg| gen_cell_define(cg, name, value))
        }
        Stmt::Assignment {
            name,
            value,
            position,
        } if cg.cell_slots.contains_key(name) => {
            with_stmt_debug(cg, *position, |cg| gen_cell_store(cg, name, value))
        }
        // A handler arm is a lifted function, so a file-scope `mut` it writes is
        // neither in its scope nor in its cell table — the write must reach the
        // module global, not bind a fresh local nobody can see
        // [MODULES-FILE-SCOPE-BINDING].
        Stmt::Assignment {
            name,
            value,
            position,
        } if cg.module_globals.contains_key(name) && cg.lookup(name).is_none() => {
            with_stmt_debug(cg, *position, |cg| gen_global_store(cg, name, value))
        }
        // Bindings preserve their inferred representation. A Result can never
        // be silently reduced to its payload at an assignment boundary.
        Stmt::Let {
            name,
            value,
            position,
            ..
        } => with_stmt_debug(cg, *position, |cg| {
            gen_bind(cg, name, value, *position, true)
        }),
        Stmt::Assignment {
            name,
            value,
            position,
        } => with_stmt_debug(cg, *position, |cg| {
            gen_bind(cg, name, value, *position, false)
        }),
        // A statement's value is discarded, so a `match` used purely for its
        // side effects is allowed arms of differing LLVM type — there is no
        // `phi` to type. Everywhere else that disagreement is a hard error
        // ([`crate::pattern::finish_phi`]).
        Stmt::Expr {
            value, position, ..
        } => with_stmt_debug(cg, *position, |cg| {
            let outer = std::mem::replace(&mut cg.value_discarded, true);
            let generated = gen_expr(cg, value);
            cg.value_discarded = outer;
            generated.map(|_| ())
        }),
        _ => Err(CodegenError::unsupported("statement in block/main")),
    }
}

/// Copy a just-lowered file-scope `let` into its module global, so functions
/// see the bound value. A handler-owned `mut` publishes its CELL, keeping the
/// arms' writes and the functions' reads on one location
/// [EFFECTS-HANDLER-STATE] [MODULES-FILE-SCOPE-BINDING].
pub(crate) fn publish_binding(cg: &mut Codegen, stmt: &Stmt) -> Result<()> {
    let Stmt::Let { name, .. } = stmt else {
        return Ok(());
    };
    if !cg.module_globals.contains_key(name) {
        return Ok(());
    }
    match cg.cell_slots.get(name).cloned() {
        Some(cell) => crate::globals::publish_cell(cg, name, &cell),
        None => match cg.lookup(name) {
            Some(value) => crate::globals::publish(cg, name, value),
            None => Err(CodegenError::unknown(name)),
        },
    }
}

pub(crate) fn stmt_position(stmt: &Stmt) -> Option<Position> {
    match stmt {
        Stmt::Let { position, .. }
        | Stmt::Assignment { position, .. }
        | Stmt::Expr { position, .. } => *position,
        _ => None,
    }
}

/// Preserve a block's final expression through its function's return sequence.
/// Synthetic binding blocks inherit the real body's position when available.
pub(crate) fn tail_position(body: &Expr) -> Option<Position> {
    match body {
        Expr::Block {
            position: Some(position),
            ..
        } => Some(*position),
        Expr::Block {
            value: Some(value), ..
        } => tail_position(value),
        _ => None,
    }
}

fn with_stmt_debug(
    cg: &mut Codegen,
    position: Option<Position>,
    f: impl FnOnce(&mut Codegen) -> Result<()>,
) -> Result<()> {
    let previous = cg.set_debug_position(position);
    // Every positioned statement is a coverable line, bumped where control
    // flow reaches it [TESTING-COVERAGE-CODEGEN].
    cg.cov_hit(position);
    let result = f(cg);
    cg.restore_debug_position(previous);
    result
}

/// The lambda a call to a GENERIC function hands back, when that lambda can be
/// applied inline at each of the binding's call sites instead of materialized
/// as one closure cell.
///
/// `let f = pick()` for `fn pick() = |x| => x` used to be rejected outright —
/// `a closure value with a still-generic type` — because one cell has one ABI
/// and `f` may be used at several. The lambda's own source position serves
/// every instantiation, so its recorded type stays generic and
/// [`crate::closure::lambda_value`] had nothing concrete to emit against.
///
/// Beta-reduction is what already makes a directly-bound `let f = |x| => x`
/// work at two instantiations, and this reuses it: the lambda is recorded for
/// inline application, so each call site specialises it at that site's real
/// types ([TYPE-GENERICS-FN]).
///
/// Two conditions keep this SOUND rather than merely permissive:
///
/// 1. The callee's body must be syntactically the lambda, so calling it
///    performs no work of its own that inlining could duplicate or drop.
/// 2. What the lambda reads from the callee's parameters is evaluated ONCE,
///    here, and carried as values in [`Codegen::lambda_prefix`]. A body
///    inlined later would otherwise read those names from whatever scope it
///    landed in — a silently wrong answer — and re-evaluating the argument
///    expression per call site would duplicate its effects. `fn constly(v) =
///    |x| => v` therefore evaluates `"hi"` at the binding and every `c(7)`
///    reuses that value.
type ReturnedLambda = (Vec<Parameter>, Vec<Parameter>, Expr, Option<Position>);

fn generic_returned_lambda(cg: &Codegen, value: &Expr) -> Option<ReturnedLambda> {
    let Expr::Call { function, .. } = value else {
        return None;
    };
    let Expr::Identifier(callee) = crate::expr::unapplied(function) else {
        return None;
    };
    // A CONCRETELY-typed result materializes a real cell on the ordinary path,
    // which keeps one-evaluation semantics; this is only for the generic shape
    // that has no single ABI to emit.
    if fn_result_type(cg, value).is_some_and(|t| crate::types::fn_value_concrete(&t)) {
        return None;
    }
    let (params, body, _) = cg.fn_defs.get(callee)?;
    let Expr::Lambda {
        parameters,
        body: lambda_body,
        position,
        ..
    } = body
    else {
        return None;
    };
    Some((
        params.clone(),
        parameters.clone(),
        (**lambda_body).clone(),
        *position,
    ))
}

fn factory_lambda_abi(
    cg: &Codegen,
    value: &Expr,
    position: Option<Position>,
) -> Option<(Vec<Position>, osprey_types::Type)> {
    let Expr::Call { function, .. } = value else {
        return None;
    };
    let Expr::Identifier(callee) = crate::expr::unapplied(function) else {
        return None;
    };
    let (_, body, _) = cg.fn_defs.get(callee)?;
    let mut lambdas = Vec::new();
    returned_lambda_positions(body, &mut lambdas);
    if lambdas.is_empty() {
        return None;
    }
    let ty = cg.prog.let_type(position)?.clone();
    crate::types::fn_value_concrete(&ty).then_some((lambdas, ty))
}

fn returned_lambda_positions(body: &Expr, positions: &mut Vec<Position>) {
    match body {
        Expr::Lambda {
            position: Some(position),
            ..
        } => positions.push(*position),
        Expr::Block {
            value: Some(value), ..
        } => returned_lambda_positions(value, positions),
        Expr::Match { arms, .. } => {
            for arm in arms {
                returned_lambda_positions(&arm.body, positions);
            }
        }
        _ => {}
    }
}

/// Whether a generic returned lambda reads the producing call's parameters.
///
/// Local bindings keep the values as SSA registers in [`Codegen::lambda_prefix`].
/// A file-scope binding read from another function instead stores the captured
/// arguments in module globals, evaluated once at the factory call.
fn captures_callee_params(cg: &Codegen, value: &Expr) -> bool {
    let Some((callee_params, _, body, _)) = generic_returned_lambda(cg, value) else {
        return false;
    };
    let mut free = BTreeSet::new();
    osprey_ast::freevars::free_idents(&body, &mut free);
    callee_params.iter().any(|p| free.contains(&p.name))
}

/// The concrete function type and closure ABI a file-scope lambda binding
/// materialises, or `None` when the lambda is still GENERIC — no single cell
/// ABI exists for it, so it stays an inline body its call sites specialise
/// ([TYPE-GENERICS-FN]).
fn lambda_cell(cg: &Codegen, position: Option<Position>) -> Option<(osprey_types::Type, FnSig)> {
    let ty = cg
        .prog
        .lambda_type(position)
        .filter(|t| crate::types::fn_value_concrete(t))?;
    Some((ty.clone(), Codegen::fn_value_sig(&cg.prog, ty)?))
}

/// The definition `value` is a bare ALIAS for, when binding it materialises no
/// value of its own — `let g = identity` with a generic `identity`.
fn alias_target(cg: &Codegen, value: &Expr) -> Option<String> {
    let Expr::Identifier(n) = value else {
        return None;
    };
    let target = cg.call_aliases.get(n).cloned().unwrap_or_else(|| n.clone());
    (cg.lookup(&target).is_none() && cg.fn_defs.contains_key(&target)).then_some(target)
}

/// Register the file-scope bindings that resolve by NAME rather than by value,
/// BEFORE any function body is emitted.
///
/// [`gen_bind`] records a generic lambda in `cg.lambdas` and a generic-function
/// alias in `cg.call_aliases`, but it does not run until `main`'s statements
/// are lowered — which is AFTER every function. A function reading such a
/// binding therefore found both tables empty and emitted a direct call to
/// `@alias`, a symbol no definition ever produces, so the module failed to
/// link ([TYPE-GENERICS-FN], [MODULES-FILE-SCOPE-BINDING]).
pub(crate) fn seed_name_bindings(
    cg: &mut Codegen,
    program: &Program,
    read: &BTreeSet<String>,
) -> Result<()> {
    for statement in &program.statements {
        let Stmt::Let { name, value, .. } = statement else {
            continue;
        };
        if !read.contains(name) || !binds_no_value(cg, value) {
            continue;
        }
        if let Expr::Lambda {
            parameters,
            body,
            position,
            ..
        } = value
        {
            let _ = cg.file_lambdas.insert(
                name.clone(),
                (parameters.clone(), (**body).clone(), *position),
            );
        } else if let Some(target) = alias_target(cg, value) {
            let _ = cg.call_aliases.insert(name.clone(), target.clone());
            let _ = cg.file_aliases.insert(name.clone(), target);
        } else if let Some((callee_params, parameters, body, position)) =
            generic_returned_lambda(cg, value)
        {
            if captures_callee_params(cg, value) {
                seed_file_prefix(cg, name, value, &callee_params)?;
            }
            let _ = cg
                .file_lambdas
                .insert(name.clone(), (parameters, body, position));
        }
    }
    Ok(())
}

fn seed_file_prefix(
    cg: &mut Codegen,
    name: &str,
    value: &Expr,
    parameters: &[Parameter],
) -> Result<()> {
    let types = factory_capture_types(cg, value)?;
    if parameters.len() != types.len() {
        return Err(CodegenError::invalid("factory capture arity changed"));
    }
    let mut slots = Vec::with_capacity(parameters.len());
    for (parameter, ty) in parameters.iter().zip(types) {
        let slot = format!("$capture.{name}.{}", parameter.name);
        crate::globals::seed_capture(cg, &slot, &ty)?;
        slots.push(slot);
    }
    let _ = cg
        .file_lambda_prefix
        .insert(name.to_string(), (parameters.to_vec(), slots));
    Ok(())
}

fn factory_capture_types(cg: &Codegen, value: &Expr) -> Result<Vec<osprey_types::Type>> {
    let Expr::Call { function, .. } = value else {
        return Err(CodegenError::invalid("factory capture is not a call"));
    };
    match cg.callee_fn_type(function) {
        Some(osprey_types::Type::Fun { params, .. }) => Ok(params),
        _ => Err(CodegenError::invalid(
            "factory capture has no function type",
        )),
    }
}

/// Whether `let name = value` materialises NO runtime value, so nothing could
/// ever be stored into a module global for it.
///
/// [`gen_bind`] leaves exactly two shapes name-resolved instead of
/// value-bound: a still-generic lambda (inline body) and an alias for a
/// generic definition (call alias). [`crate::globals::seed`] asks this before
/// declaring storage, because a global that is declared and never filled wins
/// over the alias in identifier resolution — the reader then loaded a zeroed
/// slot, and publication failed outright with `unknown name` on a program that
/// is otherwise valid ([MODULES-FILE-SCOPE-BINDING], [TYPE-GENERICS-FN]).
pub(crate) fn binds_no_value(cg: &Codegen, value: &Expr) -> bool {
    match value {
        Expr::Lambda { position, .. } => lambda_cell(cg, *position).is_none(),
        Expr::Identifier(_) => alias_target(cg, value).is_some(),
        // Factory arguments captured by the returned lambda get their own
        // module globals; the polymorphic callable itself has no cell ABI.
        Expr::Call { .. } => generic_returned_lambda(cg, value).is_some(),
        _ => false,
    }
}

/// The function type of an expression that produces a function value: a
/// lambda with a concretely-inferred type, a call whose callee returns a
/// function, an alias of another function-typed local or a top-level function,
/// or a function-typed record field. Shared with `genfn::try_inline`, which
/// uses it to keep inlined function-typed parameters callable.
pub(crate) fn fn_result_type(cg: &Codegen, value: &Expr) -> Option<osprey_types::Type> {
    match value {
        Expr::TypeApply { .. } => cg
            .callee_fn_type(value)
            .filter(crate::types::fn_value_concrete),
        Expr::Lambda { position, .. } => cg
            .prog
            .lambda_type(*position)
            .filter(|t| crate::types::fn_value_concrete(t))
            .cloned(),
        Expr::Call { function, .. } => match &**function {
            Expr::Identifier(f) => cg.call_result_fn_type(f),
            // A curried spine (`let p3 = sum6 1 2 3`): every application peels
            // one arrow off the head's type, so the partial application's own
            // type is what is left of the chain [FLAVOR-ML-CURRY]. Without
            // this the binding stayed unregistered and `p3 4 5 6` lowered to a
            // direct call to a symbol no definition emits.
            _ => cg
                .callee_fn_type(value)
                .filter(|t| matches!(t, osprey_types::Type::Fun { .. })),
        },
        Expr::Identifier(n) => cg.fn_value_types.get(n).cloned().or_else(|| {
            // `let d = double` — alias of a named user function.
            if cg.fn_params.contains_key(n) {
                cg.prog
                    .functions
                    .get(n)
                    .map(|(p, r)| osprey_types::Type::fun(p.clone(), r.clone()))
            } else {
                None
            }
        }),
        Expr::FieldAccess { field, .. } => field_fn_type(cg, field),
        _ => None,
    }
}

/// The type of a function-typed record field, found by field name across the
/// known constructor layouts (same fallback discipline as
/// `Codegen::find_field_owner`).
fn field_fn_type(cg: &Codegen, field: &str) -> Option<osprey_types::Type> {
    let mut tys: Vec<(&String, &osprey_types::Type)> = cg
        .prog
        .ctors
        .iter()
        .filter_map(|(owner, c)| {
            c.fields
                .iter()
                .find(|(f, t)| f == field && matches!(t, osprey_types::Type::Fun { .. }))
                .map(|(_, t)| (owner, t))
        })
        .collect();
    tys.sort_by(|a, b| a.0.cmp(b.0));
    tys.into_iter().next().map(|(_, t)| t.clone())
}
