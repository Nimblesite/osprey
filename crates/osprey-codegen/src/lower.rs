//! Program/function/statement orchestration — the top-level walk over the
//! module: emit each user function (parameter and return types taken from
//! inference), then synthesize `main` from either a user `main` or the trailing
//! top-level statements.

use crate::builder::{Codegen, CodegenOptions, ParamSig};
use crate::error::Result;
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

/// `options` with the GPU kernel lowering resolved from the environment. The
/// lowering is an environment switch so the corpus harness can compile the same
/// programs both ways and require identical output [GPU-KERNEL-EXTRACT]; an
/// unrecognised value is an error, never a silent fallback.
fn with_kernel_mode(options: CodegenOptions) -> Result<CodegenOptions> {
    Ok(CodegenOptions {
        gpu_kernels: crate::gpu_kernel::mode_from_env()?,
        ..options
    })
}

fn compile_program_with_options(program: &Program, options: CodegenOptions) -> Result<String> {
    let options = with_kernel_mode(options)?;
    let prog = osprey_types::infer_program(program);
    let mut cg = Codegen::with_options(prog, options);
    // Seed the coverage denominator from the source, not from what lowering
    // happens to reach [TESTING-COVERAGE-CODEGEN].
    cg.cov_seed(program);
    // Seed the erased-`any` candidate row table from the declared records,
    // BEFORE any match or erasure site is emitted ([`crate::anybox`]).
    crate::anybox::seed_rows(&mut cg);

    record_declarations(&mut cg, program);

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
            Stmt::Let { .. } | Stmt::Assignment { .. } | Stmt::Expr { .. } => {
                top_level.push(stmt);
            }
            _ => {}
        }
    }
    // File-scope bindings the functions read need module storage, declared
    // before the first function is emitted so a forward call that inlines a
    // generic body still finds the slot [MODULES-FILE-SCOPE-BINDING].
    let read_by_functions = crate::globals::read_by_functions(program);
    let top_level_cells = crate::globals::cell_names(&top_level, &read_by_functions);
    crate::globals::seed(&mut cg, program, &top_level_cells, &read_by_functions)?;
    // A binding with no runtime value resolves by name instead, so its tables
    // must be populated before the readers are emitted too ([`crate::stmt`]).
    crate::stmt::seed_name_bindings(&mut cg, program, &read_by_functions);
    for stmt in &program.statements {
        match stmt {
            // A generic function is specialised by inlining at each call site
            // (recorded in `fn_defs`), so it is not emitted as a monomorphic def.
            Stmt::Function { name, .. } if name == "main" || cg.fn_defs.contains_key(name) => {}
            Stmt::Function {
                name,
                parameters,
                body,
                position,
                ..
            } => gen_function(&mut cg, name, parameters, body, *position)?,
            _ => {}
        }
    }

    let main_position = user_main.and_then(|(_, position)| position).or_else(|| {
        top_level
            .iter()
            .find_map(|stmt| crate::stmt::stmt_position(stmt))
    });
    cg.begin_function("main", main_position);
    emit_main_boot(&mut cg);
    // The file-scope statements run first either way: they are the entry when
    // there is no user `main`, and its initializers when there is. The checker
    // rejects executable statements beside a `main`, so nothing but bindings
    // reaches this loop in that case [MODULES-ENTRYPOINT].
    cg.cell_vars = top_level_cells;
    for (i, stmt) in top_level.iter().enumerate() {
        crate::stmt::gen_local_stmt(&mut cg, stmt)?;
        crate::stmt::publish_binding(&mut cg, stmt)?;
        let rest = top_level.get(i + 1..).unwrap_or(&[]);
        crate::arc::release_dead_after(&mut cg, rest, None);
    }
    if let Some((body, _)) = user_main {
        cg.cell_vars = crate::effects::captured_mut_vars(body);
        let _ = gen_expr(&mut cg, body)?;
    }
    crate::globals::release_all(&mut cg);
    // A program that used the testing built-ins exits with the TAP epilogue's
    // status (plan + summary printed by the runtime) [TESTING-EXIT].
    crate::arc::epilogue(&mut cg, None);
    if cg.lowered.fibers {
        // A completed fiber keeps one runtime owner so every `await` can return
        // its own retained reference. Main's language owners are gone now, so
        // release those runtime roots before process-exit leak accounting.
        cg.add_extern("declare void @fiber_cleanup_results()");
        cg.emit("call void @fiber_cleanup_results()");
    }
    if cg.lowered.channels {
        // A value sent and never received still holds the reference `send`
        // transferred to the channel, and nothing else can hand it back —
        // `fiber_cleanup_results` walks fibers, not channels [GC-ARC-PERCEUS].
        cg.add_extern("declare void @channel_cleanup()");
        cg.emit("call void @channel_cleanup()");
    }
    if cg.lowered.testing {
        let code = cg.call("i32", "osp_test_finalize", "", &[]);
        cg.emit(format!("ret i32 {code}"));
    } else {
        cg.emit("ret i32 0");
    }
    cg.finish_function(LType::I32.as_str(), "main", &[]);

    Ok(cg.render())
}

/// Pre-pass: record parameter names so named-argument calls can be ordered,
/// parse `effect` operation signatures for `handle`/`perform`, and note the
/// generic bodies that are specialised by inlining rather than emitted.
fn record_declarations(cg: &mut Codegen, program: &Program) {
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
                    // The signature is read out before `register_effect_op`
                    // takes `cg` mutably; `op_sig_of` needs the same
                    // `ProgramTypes` to tag a handle operand's element.
                    let sig = cg
                        .prog
                        .effects
                        .get(name)
                        .and_then(|m| m.get(&op.name))
                        .cloned();
                    if let Some(sig) = sig {
                        let lowered = crate::effects::op_sig_of(&cg.prog, &sig);
                        cg.register_effect_op(format!("{name}.{}", op.name), lowered);
                    }
                }
            }
            // A union an extern claims to return loses its MASK_DIRECT proof
            // (builder.rs `field_meta`); record those before any layout lands.
            Stmt::Extern {
                return_type: Some(t),
                ..
            } => cg.poison_extern_ret(t),
            _ => {}
        }
    }
}

/// The runtime anchors `main` starts with, before any user code runs.
fn emit_main_boot(cg: &mut Codegen) {
    // Anchor the profiler into the link and give it a deterministic activation
    // point: static archives only extract referenced objects, so without this
    // call a fiber-less program would link no profiler at all. A no-op unless
    // OSPREY_PROFILE is set [PROF-ACTIVATE-ENV], docs/specs/0028-Profiler.md.
    cg.add_extern("declare void @osp_prof_boot()");
    cg.emit("call void @osp_prof_boot()");
    // Anchor the MEMORY backend the same way: an allocation-free program
    // would otherwise extract no backend object from the archive, and the ARC
    // leak sentinel (armed by OSPREY_ARC_DEBUG) must print even for a program
    // that never allocates — the differential harness requires the sentinel
    // for every passing arc run, so its absence must mean "broken", never
    // "small program" [MEM-BACKENDS].
    cg.add_extern("declare void @osp_mem_boot()");
    cg.emit("call void @osp_mem_boot()");
    // Register every coverable line's counter before user code runs; the
    // init body is rendered after all lowering [TESTING-COVERAGE-CODEGEN].
    cg.cov_emit_boot();
}

fn gen_function(
    cg: &mut Codegen,
    name: &str,
    parameters: &[Parameter],
    body: &Expr,
    position: Option<Position>,
) -> Result<()> {
    let param_sig = cg.fn_param_sig(name).unwrap_or_else(|| {
        vec![
            (
                ParamSig {
                    ty: LType::I64,
                    result_inner: None,
                    fiber: None,
                },
                None,
            );
            parameters.len()
        ]
    });

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
    for (i, (p, (pty, owner))) in parameters.iter().zip(param_sig.iter()).enumerate() {
        let reg = crate::llty::param_register(i);
        let v = crate::cast::incoming_param(cg, format!("%{reg}"), pty.clone(), owner.clone());
        cg.emit_debug_param(&p.name, &v);
        cg.bind(p.name.clone(), v);
        params.push((pty.ty, reg));
    }
    // A `-> Unit` function discards its body's value, so a body that is a
    // `match` over side-effecting arms needs no common arm type
    // ([`crate::pattern::finish_phi`]).
    cg.value_discarded = cg.fn_ret_is_unit(name);
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
        if let Some(sig) = cg
            .prog
            .return_type(name)
            .and_then(|t| Codegen::fn_value_sig(&cg.prog, t))
        {
            return crate::closure::emit_closure(cg, parameters, lbody, &sig);
        }
    }
    gen_expr(cg, body)
}

/// Coerce a function body value to its declared return type. A `Result<T, E>`
/// return wraps a bare body into a Success block (or passes an existing Result
/// through); everything else coerces to the inferred scalar return type.
fn coerce_return(cg: &mut Codegen, name: &str, body: Value) -> Result<Value> {
    // A returned list literal leaves the only scope that knows it is the flat
    // layout — callers see `List<T>` [`crate::listlit::escaping`].
    let body = crate::listlit::escaping(cg, body);
    if let Some(inner) = cg.fn_ret_result_inner(name) {
        // An existing Result is re-laid to the *declared* success-slot type: a
        // body like `Error { message }` types its slot from the message (`i8*`),
        // which must agree with the `i64` the callers read or the block's
        // disc/errmsg offsets shift on 32-bit targets. [WASM-TARGET-WIDTH]
        return crate::result::fit_to_inner(cg, body, inner);
    }
    let ret_ty = cg.fn_ret_ltype(name).unwrap_or(LType::I64);
    // This cast never RECOVERS anything: a body of type `any` returned through
    // a concrete annotation is rejected by inference before emission
    // ([TYPE-ANY], `unify_assignable`), so the surviving `i64 -> i8*` direction
    // is only the uniform collection/fiber element ABI, whose element type the
    // owner tag still names — and no ownership crosses it. An `any` return is
    // the OTHER direction: `coerce_to` boxes it with its shape descriptor
    // ([`crate::anybox`]), which is where the erasing transfer happens (#208).
    crate::cast::coerce_to(cg, body, ret_ty)
}
