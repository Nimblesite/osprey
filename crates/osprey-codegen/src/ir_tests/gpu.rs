use super::*;
// ---- GPU kernel extraction [GPU-KERNEL-EXTRACT] ----

/// The lifted-kernel symbol prefix, spelled once so a rename of the naming
/// scheme fails these tests loudly rather than silently matching nothing.
const KERNEL_PREFIX: &str = "@__gpu_kernel_";

#[test]
fn a_lambda_kernel_is_lifted_to_one_function_the_loop_calls() {
    // gpu_kernel.rs: the kernel body leaves the host loop entirely, and the
    // symbol is defined exactly once however many elements run through it.
    let ir = module(
        "fn f() = {\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(fn(v) => v * 2.0)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    assert!(
        ir.contains("define double @__gpu_kernel_0(double %$p0)"),
        "{ir}"
    );
    assert_eq!(ir.matches("define double @__gpu_kernel_0(").count(), 1);
    let host = function_body(&ir, "define i64 @f()");
    assert!(
        host.contains("call double @__gpu_kernel_0(double "),
        "{host}"
    );
    assert!(
        !host.contains("fmul double"),
        "kernel still inlined: {host}"
    );
}

#[test]
fn each_distinct_kernel_gets_its_own_symbol() {
    let ir = module(
        "fn f() = {\n\
               let a = toGpu([1.0, 2.0]) |> gpuMap(fn(v) => v * 2.0)\n\
               let b = a |> gpuMap(fn(v) => v + 1.0)\n\
               gpuLength(b)\n\
             }\n\
             print(f())\n",
    );
    assert_eq!(ir.matches(KERNEL_PREFIX).count(), 4, "{ir}");
    shows(
        &ir,
        &[
            "define double @__gpu_kernel_0(double %$p0)",
            "define double @__gpu_kernel_1(double %$p0)",
        ],
    );
}

#[test]
fn captures_become_leading_parameters_sorted_by_name() {
    // Uniforms lead the element slot and are ordered by identifier —
    // `alpha` then `beta` — whatever order the body reads them in. There is
    // no environment pointer: the ABI is flat.
    let ir = module(
        "fn f() = {\n\
               let beta = 5.0\n\
               let alpha = 2.0\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(fn(v) => beta * v + alpha)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    let header = "define double @__gpu_kernel_0(double %$p0, double %$p1, double %$p2)";
    assert!(ir.contains(header), "{ir}");
    let kernel = function_body(&ir, header);
    shows(&kernel, &["fmul double %$p1, %$p2", "fadd double"]);
    assert!(!kernel.contains("__env"), "{kernel}");
    let host = function_body(&ir, "define i64 @f()");
    assert!(
        host.contains("@__gpu_kernel_0(double 2.0, double 5.0, double "),
        "{host}"
    );
}

#[test]
fn a_captured_buffer_handle_travels_as_a_uniform() {
    let ir = module(
        "fn f() = {\n\
               let w = toGpu([10.0, 20.0])\n\
               let out = gpuIota(2) |> gpuMap(fn(i) => gpuGet(w, i) ?: 0.0)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    assert!(
        ir.contains("define double @__gpu_kernel_0(i8* %$p0, i64 %$p1)"),
        "{ir}"
    );
}

#[test]
fn a_fold_kernel_takes_uniforms_then_accumulator_then_element() {
    let ir = module(
        "fn f() = {\n\
               let k = 3.0\n\
               toGpu([1.0, 2.0]) |> gpuFold(0.0, fn(acc, v) => acc + k * v)\n\
             }\n\
             print(\"${f()}\")\n",
    );
    let header = "define double @__gpu_kernel_0(double %$p0, double %$p1, double %$p2)";
    assert!(ir.contains(header), "{ir}");
    let kernel = function_body(&ir, header);
    shows(&kernel, &["fmul double %$p0, %$p2", "fadd double %$p1,"]);
}

/// A handler with one RESUMING arm and one SUBSTITUTING arm continues by
/// re-entering its own dispatcher with the same arguments. A plain call
/// there grew one native frame per operation the substituting arm answered:
/// release builds happened to fold it away, debug builds compile at `-O0`
/// where nothing does, and a loop of otherwise constant-space performs
/// exhausted the host stack. The reuse must be a property of the MODULE,
/// not a hope about the optimizer. Implements [EFFECTS-HANDLER-ARMS].
#[test]
fn a_mixed_handler_dispatcher_continues_by_guaranteed_tail_call() {
    let ir = module(
        "effect Log {\n\
               note: fn(int) -> Unit\n\
               control ask: fn(int) -> int\n\
               }\n\
               fn hammer(n) = range(0, n) |> forEach(fn(i) => perform Log.note(i))\n\
               let done = {\n\
                   handle Log {\n\
                       note value => value\n\
                       ask value => resume(1)\n\
                   }\n\
                   hammer(3)\n\
               }\n\
               print(\"done\")\n",
    );
    let body = function_body(
        &ir,
        "define i64 @__resume_drive_Log_0(i8* %__env, i8* %__coro)",
    );
    let reentries: Vec<&str> = body
        .lines()
        .filter(|line| line.contains("call i64 @__resume_drive_Log_0"))
        .collect();
    assert_eq!(reentries.len(), 1, "one re-entry per dispatcher: {body}");
    assert!(
        reentries.iter().all(|line| line.contains("musttail call")),
        "the dispatcher must re-enter itself with a guaranteed tail call, saw: {reentries:?}"
    );
}

#[test]
fn a_named_kernel_reuses_its_own_definition() {
    // A named function already has an emitted symbol the loop calls, so
    // extraction emits nothing new and copies nothing.
    let ir = module(
        "fn twice(x: float) -> float = x * 2.0\n\
             fn f() = {\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(twice)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    assert!(!ir.contains(KERNEL_PREFIX), "{ir}");
    assert_eq!(ir.matches("define double @twice(").count(), 1);
    assert!(function_body(&ir, "define i64 @f()").contains("call double @twice(double "));
}

#[test]
fn a_known_let_bound_kernel_is_extracted() {
    let ir = module("fn f() = { let k = fn(v) => v * 2.0\n let out = toGpu([1.0, 2.0]) |> gpuMap(k)\n gpuLength(out) }\nprint(f())\n");
    assert_eq!(
        ir.matches("define double @__gpu_kernel_").count(),
        1,
        "{ir}"
    );
    assert!(
        ir.contains("define double @__gpu_kernel_0(double %$p0)"),
        "{ir}"
    );
    let host = function_body(&ir, "define i64 @f()");
    assert!(
        host.contains("call double @__gpu_kernel_0(double "),
        "{host}"
    );
    assert!(!host.contains("fmul double"), "{host}");
}

#[test]
fn a_closure_valued_kernel_keeps_its_cell_call() {
    // A factory result is opaque; its environment retains the ordinary cell ABI.
    let ir = module(
        "fn make(scale) = fn(v) => v * scale\n\
             fn f() = {\n\
               let k = make(2.0)\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(k)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    assert!(!ir.contains(KERNEL_PREFIX), "{ir}");
}

#[test]
fn a_result_returning_kernel_keeps_its_scalar_diagnostic() {
    // `checkedMul` makes the kernel return Result<int, Error> (`*` itself
    // is `int` [ARITH-EFFECT]); the host still rejects it where it always
    // did, with the same message.
    let err = compile_err(
        "fn f() = {\n\
               let out = toGpu([1, 2]) |> gpuMap(fn(v) => checkedMul(v, 2))\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    assert!(
        err.to_string()
            .contains("a gpuMap kernel result cannot be an unhandled Result"),
        "{err}"
    );
}

#[test]
fn twin_flavors_number_their_kernels_identically() {
    // Kernel symbols come from a counter advanced in AST walk order, never
    // from a position or an identifier spelling [FLAVOR-IR-EQUIV].
    let default_ir = module(
        "fn f() = {\n\
               let m = 2.0\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(fn(v) => v - m)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    let ml_ir = ml_module(
        "f () =\n    \
                 m = 2.0\n    \
                 out = toGpu [1.0, 2.0] |> gpuMap (\\v => v - m)\n    \
                 gpuLength out\n\
             \n\
             print (f ())\n",
    );
    assert_eq!(default_ir, ml_ir);
    assert!(default_ir.contains("define double @__gpu_kernel_0(double %$p0, double %$p1)"));
}

#[test]
fn codegen_error_display_covers_all_variants() {
    // error.rs Display for every CodegenError variant.
    assert_eq!(
        CodegenError::unsupported("x").to_string(),
        "codegen: unsupported construct: x"
    );
    assert_eq!(
        CodegenError::unknown("n").to_string(),
        "codegen: unknown name `n`"
    );
    assert_eq!(
        CodegenError::invalid("p").to_string(),
        "codegen: invalid program: p"
    );
}
