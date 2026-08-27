//! Statement lowering: the `let` / assignment / bare-expression forms that make
//! up a block body, the trailing top-level sequence, and the handler-owned
//! `mut` cells among them. Every one of these appears both inside a function
//! and at file scope, so they lower through exactly one path.

use crate::builder::{Codegen, FnSig};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Position, Program, Stmt};
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
        } => with_stmt_debug(cg, *position, |cg| gen_bind(cg, name, value, *position)),
        Stmt::Assignment {
            name,
            value,
            position,
        } => with_stmt_debug(cg, *position, |cg| gen_bind(cg, name, value, *position)),
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

/// Declare a handler-captured plain-value `mut` as a heap cell. Result-backed
/// cells require a discriminant-bearing slot and are rejected instead of being
/// silently unwrapped.
fn gen_cell_define(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let v = gen_expr(cg, value)?;
    if v.result_inner.is_some() {
        return Err(CodegenError::invalid(
            "mutable Result state must be handled before storage",
        ));
    }
    let fn_ty = fn_result_type(cg, value);
    let pointee = v.ty;
    let ty = pointee.as_str();
    let meta = crate::meta::struct_meta(&[crate::meta::MetaField::of_lty(pointee)]);
    let cell = cg.malloc_struct(&format!("{{ {ty} }}"), meta);
    let ptr = cg.emit_reg(format!(
        "getelementptr {{ {ty} }}, {{ {ty} }}* {cell}, i32 0, i32 0"
    ));
    // The cell holds its own reference to the stored value [GC-ARC-PERCEUS].
    crate::arc::dup_store(cg, ty, &v.operand);
    cg.emit(format!("store {ty} {}, {ty}* {ptr}", v.operand));
    // The cell itself is a heap allocation owned by the region that declared
    // the `mut`. A handler env capturing it only DUPs it (`build_env`), so
    // without this the cell outlives every region and leaks — one per captured
    // `mut`. [GC-ARC-PERCEUS].
    let handle = if ty == "i8*" {
        ptr.clone()
    } else {
        cg.emit_reg(format!("bitcast {ty}* {ptr} to i8*"))
    };
    crate::arc::own_beyond_stmt(cg, &Value::new(handle, LType::Ptr));
    let _ = cg.cell_slots.insert(
        name.to_string(),
        crate::builder::CellSlot {
            ptr,
            pointee,
            osp_ty: v.osp_ty,
        },
    );
    if let Some(ty) = fn_ty {
        cg.bind_fn_local(name, ty);
    }
    Ok(())
}

/// Reassign a cell-backed `mut`: the checker requires the exact plain cell type,
/// then codegen coerces only within that representation and stores it.
fn gen_cell_store(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let Some(slot) = cg.cell_slots.get(name).cloned() else {
        return Err(CodegenError::unsupported(
            "reassignment of an unpromoted cell",
        ));
    };
    let v = gen_expr(cg, value)?;
    let v = crate::cast::coerce_to(cg, v, slot.pointee)?;
    let ty = slot.pointee.as_str();
    // Rebind order: dup the incoming value BEFORE dropping the old one, so a
    // self-assignment never frees the value it stores [GC-ARC-PERCEUS].
    crate::arc::dup_store(cg, ty, &v.operand);
    if slot.pointee.is_managed_ptr() {
        let old = cg.emit_reg(format!("load {ty}, {ty}* {}", slot.ptr));
        crate::arc::release_operand(cg, &old);
    }
    cg.emit(format!("store {ty} {}, {ty}* {}", v.operand, slot.ptr));
    Ok(())
}

/// Reassign a file-scope binding through its module global. The checker allows
/// the write only inside a handler arm, which is exactly where the enclosing
/// frame is out of reach.
fn gen_global_store(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let v = gen_expr(cg, value)?;
    crate::globals::assign(cg, name, v)
}

/// Bind `name` to `value`. A lambda is recorded for inline application at its
/// direct call sites (a beta-reduction fast path) AND materialized as a closure
/// cell so the name is a first-class value.
fn gen_bind(cg: &mut Codegen, name: &str, value: &Expr, position: Option<Position>) -> Result<()> {
    let expected_result_inner = cg
        .prog
        .let_type(position)
        .and_then(crate::types::result_inner)
        .or_else(|| cg.lookup(name).and_then(|bound| bound.result_inner));
    if let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = value
    {
        let _ = cg.lambdas.insert(
            name.to_string(),
            (parameters.clone(), (**body).clone(), *position),
        );
        // Materialize the closure value when its type resolved concretely; a
        // still-generic lambda stays inline-only (its cell ABI would lose the
        // per-instantiation types).
        if let Some((ty, sig)) = lambda_cell(cg, *position) {
            let v = crate::closure::emit_closure(cg, parameters, body, &sig)?;
            cg.emit_debug_local(name, &v);
            crate::arc::bind_owned(cg, name, &v);
            cg.bind(name.to_string(), v);
            cg.bind_fn_local(name, ty);
        }
        return Ok(());
    }
    // A generic function whose body IS a lambda hands back a value with no
    // single cell ABI; record it for inline application instead of rejecting.
    // Never fire for a name that already has module storage: that slot is
    // declared for cross-function readers and this path fills nothing, so the
    // reader would load a zeroed global.
    if let Some((callee_params, parameters, body, lambda_position)) =
        generic_returned_lambda(cg, value).filter(|_| !cg.module_globals.contains_key(name))
    {
        // The callee's arguments are evaluated exactly ONCE, here — the call
        // in the source happens once, so its effects must too, however many
        // instantiations the binding is later used at.
        let mut prefix = Vec::new();
        if let Expr::Call {
            arguments,
            named_arguments,
            ..
        } = value
        {
            for argument in arguments {
                prefix.push(gen_expr(cg, argument)?);
            }
            for argument in named_arguments {
                prefix.push(gen_expr(cg, &argument.value)?);
            }
        }
        if prefix.len() == callee_params.len() {
            let _ = cg
                .lambda_prefix
                .insert(name.to_string(), (callee_params, prefix));
        } else {
            let _ = cg.lambda_prefix.remove(name);
        }
        let _ = cg
            .lambdas
            .insert(name.to_string(), (parameters, body, lambda_position));
        let _ = cg.call_aliases.remove(name);
        return Ok(());
    }
    if let Some(target) = alias_target(cg, value) {
        // `let g = identity` where the target is a GENERIC function: no
        // single concrete cell ABI exists, so bind as a call alias — g's
        // call sites specialise the target exactly as direct calls do,
        // and a value use resolves the alias where a consuming slot fixes
        // the ABI ([TYPE-GENERICS-FN]).
        let _ = cg.call_aliases.insert(name.to_string(), target);
        return Ok(());
    }
    let v = gen_expr(cg, value)?;
    let v = match expected_result_inner {
        Some(inner) => crate::result::fit_to_inner(cg, v, inner)?,
        None => v,
    };
    // A bound `Fiber<T>`/`Channel<T>` carries its element ABI from INFERENCE,
    // not from whatever the right-hand side happened to know. `Channel(2)` has
    // no element yet, an alias copies whatever the original was tagged with,
    // and a handle received before its first `send` was tagged with nothing at
    // all — each of those made `recv` hand back the raw `i64` wire word, so a
    // list element arrived as an integer ([CONCURRENCY-CHANNEL]).
    let v = tag_handle_element(cg, position, v);
    // A non-lambda (re)binding invalidates any stale beta-reduction entry or
    // call alias for the name — `mut f = fn(x) => …; f = makeAdder(10)` must
    // call the new closure, not the old inline body.
    let _ = cg.lambdas.remove(name);
    let _ = cg.call_aliases.remove(name);
    // A function-valued binding (`let add5 = makeAdder(5)`) registers its
    // function type so `add5(3)` lowers as a closure call.
    if let Some(ty) = fn_result_type(cg, value) {
        cg.bind_fn_local(name, ty);
    }
    cg.emit_debug_local(name, &v);
    // The binding outlives the statement region: move the statement's
    // ownership out, or retain a borrow [GC-ARC-PERCEUS].
    crate::arc::bind_owned(cg, name, &v);
    cg.bind(name.to_string(), v);
    Ok(())
}

/// Re-tag a bound handle with the element ABI inference resolved for it. Not a
/// handle, or a handle whose element is still polymorphic: unchanged.
fn tag_handle_element(cg: &Codegen, position: Option<Position>, value: Value) -> Value {
    let Some(ty) = cg.prog.let_type(position) else {
        return value;
    };
    let Some(sig) = crate::builder::FiberSig::of(&cg.prog, ty) else {
        return value;
    };
    let owner = match ty {
        osprey_types::Type::Con { args, .. } => crate::types::elem_tag(&cg.prog, args.first()),
        _ => None,
    };
    let mut tagged = sig.restore(value);
    tagged.fiber_elem_owner = owner;
    tagged
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
type ReturnedLambda = (
    Vec<osprey_ast::Parameter>,
    Vec<osprey_ast::Parameter>,
    Expr,
    Option<Position>,
);

fn generic_returned_lambda(cg: &Codegen, value: &Expr) -> Option<ReturnedLambda> {
    let Expr::Call { function, .. } = value else {
        return None;
    };
    let Expr::Identifier(callee) = &**function else {
        return None;
    };
    // A CONCRETELY-typed result materializes a real cell on the ordinary path,
    // which keeps one-evaluation semantics; this is only for the generic shape
    // that has no single ABI to emit.
    if fn_result_type(cg, value).is_some_and(|t| crate::types::fn_value_concrete(&t)) {
        return None;
    }
    let (params, body) = cg.fn_defs.get(callee)?;
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

/// Whether a generic returned lambda reads the producing call's parameters.
///
/// Those are carried as SSA registers of the function that evaluated the call
/// ([`Codegen::lambda_prefix`]), so such a binding can only serve readers in
/// that same function. A FILE-SCOPE one read from another function cannot be
/// inlined there — the registers do not exist — so it keeps the ordinary path,
/// which rejects it truthfully rather than emitting a call to a symbol no
/// definition produces.
fn captures_callee_params(cg: &Codegen, value: &Expr) -> bool {
    let Some((callee_params, _, body, _)) = generic_returned_lambda(cg, value) else {
        return false;
    };
    let mut free = std::collections::BTreeSet::new();
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
pub(crate) fn seed_name_bindings(cg: &mut Codegen, program: &Program, read: &BTreeSet<String>) {
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
            let _ = cg.call_aliases.insert(name.clone(), target);
        } else if let Some((_, parameters, body, position)) =
            generic_returned_lambda(cg, value).filter(|_| !captures_callee_params(cg, value))
        {
            // A generic function's returned lambda bound at FILE SCOPE and read
            // by a function: seed it here, or that function emits `call @name`
            // to a symbol no definition produces and the module fails to LINK —
            // which neither the type gate nor codegen would have caught.
            let _ = cg
                .file_lambdas
                .insert(name.clone(), (parameters, body, position));
        }
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
        // Capture-free only: a capturing one cannot be seeded for
        // cross-function readers, so it must keep ordinary storage handling.
        Expr::Call { .. } => {
            generic_returned_lambda(cg, value).is_some() && !captures_callee_params(cg, value)
        }
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
