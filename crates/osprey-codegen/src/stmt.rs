//! Statement lowering: the `let` / assignment / bare-expression forms that make
//! up a block body, the trailing top-level sequence, and the handler-owned
//! `mut` cells among them. Every one of these appears both inside a function
//! and at file scope, so they lower through exactly one path.

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Position, Stmt};

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
        if let Some(ty) = cg.prog.lambda_type(*position).cloned() {
            if crate::types::fn_value_concrete(&ty) {
                if let Some(sig) = Codegen::fn_value_sig(&ty) {
                    let v = crate::closure::emit_closure(cg, parameters, body, &sig)?;
                    cg.emit_debug_local(name, &v);
                    crate::arc::bind_owned(cg, name, &v);
                    cg.bind(name.to_string(), v);
                    cg.bind_fn_local(name, ty);
                }
            }
        }
        return Ok(());
    }
    if let Expr::Identifier(n) = value {
        let target = cg.call_aliases.get(n).cloned().unwrap_or_else(|| n.clone());
        if cg.lookup(&target).is_none() && cg.fn_defs.contains_key(&target) {
            // `let g = identity` where the target is a GENERIC function: no
            // single concrete cell ABI exists, so bind as a call alias — g's
            // call sites specialise the target exactly as direct calls do,
            // and a value use resolves the alias where a consuming slot fixes
            // the ABI ([TYPE-GENERICS-FN]).
            let _ = cg.call_aliases.insert(name.to_string(), target);
            return Ok(());
        }
    }
    let v = gen_expr(cg, value)?;
    let v = match expected_result_inner {
        Some(inner) => crate::result::fit_to_inner(cg, v, inner)?,
        None => v,
    };
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
            _ => None,
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
