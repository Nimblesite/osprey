//! Program/function/statement orchestration — the top-level walk over the
//! module: emit each user function (parameter and return types taken from
//! inference), then synthesize `main` from either a user `main` or the trailing
//! top-level statements.

use crate::builder::{Codegen, CodegenOptions};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter, Position, Program, Stmt};
use osprey_debug::DebugSource;

/// Compile a whole program to an LLVM IR module (text), driven by the inferred
/// types in [`osprey_types::ProgramTypes`].
///
/// # Errors
///
/// Returns `Err` if any function body, top-level statement, or `main`
/// expression contains a construct that cannot be lowered to LLVM IR.
pub fn compile_program(program: &Program) -> Result<String> {
    compile_program_with_options(program, CodegenOptions::default())
}

/// Compile a whole program with LLVM/DWARF debug metadata rooted at `source`.
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_program_debug(program: &Program, source: DebugSource) -> Result<String> {
    compile_program_with_options(
        program,
        CodegenOptions {
            debug_source: Some(source),
            ..CodegenOptions::default()
        },
    )
}

/// Compile a whole program with line-coverage instrumentation
/// [TESTING-COVERAGE-CODEGEN].
///
/// # Errors
///
/// Returns `Err` under the same conditions as [`compile_program`].
pub fn compile_program_coverage(program: &Program) -> Result<String> {
    compile_program_with_options(
        program,
        CodegenOptions {
            coverage: true,
            ..CodegenOptions::default()
        },
    )
}

fn compile_program_with_options(program: &Program, options: CodegenOptions) -> Result<String> {
    let prog = osprey_types::infer_program(program);
    let mut cg = Codegen::with_options(prog, options);
    // Seed the coverage denominator from the source, not from what lowering
    // happens to reach [TESTING-COVERAGE-CODEGEN].
    cg.cov_seed(program);

    // Pre-pass: record parameter names so named-argument calls can be ordered,
    // and parse `effect` operation signatures for `handle`/`perform`.
    for stmt in &program.statements {
        match stmt {
            Stmt::Function {
                name,
                parameters,
                body,
                position,
                ..
            } => {
                let _ = cg.fn_params.insert(
                    name.clone(),
                    parameters.iter().map(|p| p.name.clone()).collect(),
                );
                // A polymorphic function is specialised by inlining at each call
                // site, so keep its body reachable — and its definition line
                // coverable through the inline path [TESTING-COVERAGE-CODEGEN].
                if cg.is_generic_fn(name) {
                    let _ = cg
                        .fn_defs
                        .insert(name.clone(), (parameters.clone(), body.clone()));
                    cg.cov_note_inline_fn(name, *position);
                }
            }
            Stmt::Effect {
                name, operations, ..
            } => {
                for op in operations {
                    if let Some(sig) = cg.prog.effects.get(name).and_then(|m| m.get(&op.name)) {
                        cg.register_effect_op(
                            format!("{name}.{}", op.name),
                            crate::effects::op_sig_of(sig),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    let mut top_level: Vec<&Stmt> = Vec::new();
    let mut user_main: Option<(&Expr, Option<Position>)> = None;
    for stmt in &program.statements {
        match stmt {
            Stmt::Function {
                name,
                body,
                position,
                ..
            } if name == "main" => user_main = Some((body, *position)),
            // A generic function is specialised by inlining at each call site
            // (recorded in `fn_defs`), so it is not emitted as a monomorphic def.
            Stmt::Function { name, .. } if cg.fn_defs.contains_key(name) => {}
            Stmt::Function {
                name,
                parameters,
                body,
                position,
                ..
            } => gen_function(&mut cg, name, parameters, body, *position)?,
            Stmt::Let { .. } | Stmt::Assignment { .. } | Stmt::Expr { .. } => {
                top_level.push(stmt);
            }
            _ => {}
        }
    }

    let main_position = user_main
        .and_then(|(_, position)| position)
        .or_else(|| top_level.iter().find_map(|stmt| stmt_position(stmt)));
    cg.begin_function("main", main_position);
    // Anchor the profiler into the link and give it a deterministic activation
    // point: static archives only extract referenced objects, so without this
    // call a fiber-less program would link no profiler at all. A no-op unless
    // OSPREY_PROFILE is set [PROF-ACTIVATE-ENV], docs/specs/0028-Profiler.md.
    cg.add_extern("declare void @osp_prof_boot()");
    cg.emit("call void @osp_prof_boot()");
    // Register every coverable line's counter before user code runs; the
    // init body is rendered after all lowering [TESTING-COVERAGE-CODEGEN].
    cg.cov_emit_boot();
    if let Some((body, _)) = user_main {
        cg.cell_vars = crate::effects::captured_mut_vars(body);
        let _ = gen_expr(&mut cg, body)?;
    } else {
        cg.cell_vars = crate::effects::captured_mut_vars_in_stmts(&top_level);
        for stmt in &top_level {
            gen_local_stmt(&mut cg, stmt)?;
        }
    }
    // A program that used the testing built-ins exits with the TAP epilogue's
    // status (plan + summary printed by the runtime) [TESTING-EXIT].
    crate::arc::epilogue(cg, None);
    if cg.testing_used {
        let code = cg.call("i32", "osp_test_finalize", "", &[]);
        cg.emit(format!("ret i32 {code}"));
    } else {
        cg.emit("ret i32 0");
    }
    cg.finish_function(LType::I32.as_str(), "main", &[]);

    Ok(cg.render())
}

fn gen_function(
    cg: &mut Codegen,
    name: &str,
    parameters: &[Parameter],
    body: &Expr,
    position: Option<Position>,
) -> Result<()> {
    let param_sig = cg
        .fn_param_sig(name)
        .unwrap_or_else(|| vec![(LType::I64, None); parameters.len()]);

    cg.begin_function(name, position);
    // Record any function-typed parameters so a call through one lowers to an
    // indirect call (the higher-order `f(x)` in `fn apply(f, x) = f(x)`).
    let fn_ptr_params: Vec<(String, osprey_types::Type)> = cg
        .prog
        .param_types(name)
        .map(|ptys| {
            parameters
                .iter()
                .zip(ptys)
                .filter(|(_, t)| matches!(t, osprey_types::Type::Fun { .. }))
                .map(|(p, t)| (p.name.clone(), t.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (n, t) in fn_ptr_params {
        cg.bind_fn_local(&n, t);
    }
    let mut params = Vec::new();
    for (p, (pty, owner)) in parameters.iter().zip(param_sig.iter()) {
        let v = Value::new(format!("%{}", p.name), *pty).with_owner(owner.clone());
        cg.emit_debug_param(&p.name, &v);
        cg.bind(p.name.clone(), v);
        params.push((*pty, p.name.clone()));
    }
    cg.cell_vars = crate::effects::captured_mut_vars(body);
    // The definition line counts as covered when the body executes
    // [TESTING-COVERAGE-CODEGEN].
    cg.cov_hit(position);
    let body_val = gen_fn_body(cg, name, body)?;
    let ret = coerce_return(cg, name, body_val)?;
    // Returns transfer +1; everything else the function owned drops here
    // [GC-ARC-PERCEUS].
    crate::arc::epilogue(cg, Some(&ret));
    cg.emit(format!("ret {} {}", ret.llvm_ty(), ret.operand));
    cg.finish_function(&ret.llvm_ty(), name, &params);
    Ok(())
}

/// Lower a function body. A body that IS a lambda (`fn makeAdder(n) = fn(x) =>
/// x + n`) becomes a closure cell typed by the function's declared/inferred
/// return type — the same signature its callers will use — so maker and caller
/// agree on the ABI.
fn gen_fn_body(cg: &mut Codegen, name: &str, body: &Expr) -> Result<Value> {
    if let Expr::Lambda {
        parameters,
        body: lbody,
        ..
    } = body
    {
        if let Some(sig) = cg.prog.return_type(name).and_then(Codegen::fn_value_sig) {
            return crate::closure::emit_closure(cg, parameters, lbody, &sig);
        }
    }
    gen_expr(cg, body)
}

/// Coerce a function body value to its declared return type. A `Result<T, E>`
/// return wraps a bare body into a Success block (or passes an existing Result
/// through); everything else coerces to the inferred scalar return type.
fn coerce_return(cg: &mut Codegen, name: &str, body: Value) -> Result<Value> {
    if let Some(inner) = cg.fn_ret_result_inner(name) {
        // An existing Result is re-laid to the *declared* success-slot type: a
        // body like `Error { message }` types its slot from the message (`i8*`),
        // which must agree with the `i64` the callers read or the block's
        // disc/errmsg offsets shift on 32-bit targets. [WASM-TARGET-WIDTH]
        if body.result_inner.is_some() {
            return crate::result::repack_to_inner(cg, body, inner);
        }
        return crate::result::make_ok(cg, body, inner);
    }
    let ret_ty = cg.fn_ret_ltype(name).unwrap_or(LType::I64);
    crate::cast::coerce_to(cg, body, ret_ty)
}

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
        // An immutable `let` keeps a Result wrapper (so `let v = 21 * 2;
        // toString(v)` shows `Success(42)`); a `mut` reassignment auto-unwraps it
        // (the `mut` auto-unwrap rule: the cell holds the success payload).
        Stmt::Let {
            name,
            value,
            position,
            ..
        } => with_stmt_debug(cg, *position, |cg| gen_bind(cg, name, value, false)),
        Stmt::Assignment {
            name,
            value,
            position,
        } => with_stmt_debug(cg, *position, |cg| gen_bind(cg, name, value, true)),
        Stmt::Expr { value, position } => with_stmt_debug(cg, *position, |cg| {
            let _ = gen_expr(cg, value)?;
            Ok(())
        }),
        _ => Err(CodegenError::unsupported("statement in block/main")),
    }
}

fn stmt_position(stmt: &Stmt) -> Option<Position> {
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

/// Declare a handler-captured `mut` as a heap cell: evaluate + unwrap the
/// initializer, `malloc` a one-slot cell of its type, store the initial value,
/// and record the slot so reads `load` and reassignments `store` it.
fn gen_cell_define(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let raw = gen_expr(cg, value)?;
    let v = crate::result::unwrap(cg, raw);
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
    let _ = cg.cell_slots.insert(
        name.to_string(),
        crate::builder::CellSlot {
            ptr,
            pointee,
            osp_ty: v.osp_ty,
        },
    );
    Ok(())
}

/// Reassign a cell-backed `mut`: evaluate + unwrap, coerce to the cell's type,
/// and `store` into the shared slot.
fn gen_cell_store(cg: &mut Codegen, name: &str, value: &Expr) -> Result<()> {
    let Some(slot) = cg.cell_slots.get(name).cloned() else {
        return Err(CodegenError::unsupported(
            "reassignment of an unpromoted cell",
        ));
    };
    let raw = gen_expr(cg, value)?;
    let v = crate::result::unwrap(cg, raw);
    let v = crate::cast::coerce_to(cg, v, slot.pointee)?;
    let ty = slot.pointee.as_str();
    // Rebind order: dup the incoming value BEFORE dropping the old one, so a
    // self-assignment never frees the value it stores [GC-ARC-PERCEUS].
    crate::arc::dup_store(cg, ty, &v.operand);
    if matches!(slot.pointee, LType::Str | LType::Ptr) {
        let old = cg.emit_reg(format!("load {ty}, {ty}* {}", slot.ptr));
        crate::arc::release_operand(cg, &old);
    }
    cg.emit(format!("store {ty} {}, {ty}* {}", v.operand, slot.ptr));
    Ok(())
}

/// Bind `name` to `value`. A lambda is recorded for inline application at its
/// direct call sites (a beta-reduction fast path) AND materialized as a closure
/// cell so the name is a first-class value. When `unwrap` is set (a mutable
/// assignment), a Result value is unwrapped to its success payload before
/// binding.
fn gen_bind(cg: &mut Codegen, name: &str, value: &Expr, unwrap: bool) -> Result<()> {
    if let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = value
    {
        let _ = cg
            .lambdas
            .insert(name.to_string(), (parameters.clone(), (**body).clone()));
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
    let v = if unwrap {
        crate::result::unwrap(cg, v)
    } else {
        v
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
