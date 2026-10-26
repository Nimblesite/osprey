use super::*;

#[test]
fn named_device_callbacks_keep_their_defining_scope() {
    let ir = module("let factor = 2.0\nfn scale(x) = x * factor\nfn apply(f, x) = f(x)\nfn caller(factor, x) = apply(scale, x)\nfn main() = {\n    let out = toGpu([3.0]) |> gpuMap(fn(x) => caller(100.0, x))\n    print(gpuLength(out))\n}\n");
    testkit::contents(&ir, &[], &["call i32 @osprey_gpu_declined("]);
    shows(&ir, &["0x4000000000000000ul"]);
}
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
    shows(
        &ir,
        &["define internal double @__gpu_kernel_0(double %$p0)"],
    );
    assert_eq!(
        ir.matches("define internal double @__gpu_kernel_0(")
            .count(),
        1
    );
    let host = function_body(&ir, "define internal i64 @f()");
    shows(&host, &["call double @__gpu_kernel_0(double "]);
    testkit::contents(&host, &[], &["fmul double"]);
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
            "define internal double @__gpu_kernel_0(double %$p0)",
            "define internal double @__gpu_kernel_1(double %$p0)",
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
    let header = "define internal double @__gpu_kernel_0(double %$p0, double %$p1, double %$p2)";
    assert!(ir.contains(header), "{ir}");
    let kernel = function_body(&ir, header);
    shows(&kernel, &["fmul double %$p1, %$p2", "fadd double"]);
    testkit::contents(&kernel, &[], &["__env"]);
    let host = function_body(&ir, "define internal i64 @f()");
    shows(&host, &["@__gpu_kernel_0(double 2.0, double 5.0, double "]);
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
    shows(
        &ir,
        &["define internal double @__gpu_kernel_0(i8* %$p0, i64 %$p1)"],
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
    let header = "define internal double @__gpu_kernel_0(double %$p0, double %$p1, double %$p2)";
    assert!(ir.contains(header), "{ir}");
    let kernel = function_body(&ir, header);
    shows(&kernel, &["fmul double %$p0, %$p2", "fadd double %$p1,"]);
}

#[test]
fn a_named_kernel_reuses_its_own_definition() {
    // A named function already has an emitted symbol the loop calls, so
    // extraction emits nothing new and copies nothing.
    let ir = module(
        // The annotations are deliberate: an unannotated `twice` is a generic
        // kernel, specialised and lifted at its call site, which is the other
        // test's subject. Only a function with a CONCRETE signature has its own
        // symbol for the loop to call.
        "fn twice(x: float) -> float = x * 2.0\n\
             fn f() = {\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(twice)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    assert!(!ir.contains(KERNEL_PREFIX), "{ir}");
    assert_eq!(ir.matches("define internal double @twice(").count(), 1);
    assert!(function_body(&ir, "define internal i64 @f()").contains("call double @twice(double "));
}

#[test]
fn a_known_let_bound_kernel_is_extracted() {
    let ir = module("fn f() = { let k = fn(v) => v * 2.0\n let out = toGpu([1.0, 2.0]) |> gpuMap(k)\n gpuLength(out) }\nprint(f())\n");
    assert_eq!(
        ir.matches("define internal double @__gpu_kernel_").count(),
        1,
        "{ir}"
    );
    shows(
        &ir,
        &["define internal double @__gpu_kernel_0(double %$p0)"],
    );
    let host = function_body(&ir, "define internal i64 @f()");
    shows(&host, &["call double @__gpu_kernel_0(double "]);
    testkit::contents(&host, &[], &["fmul double"]);
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
    shows(
        &default_ir,
        &["define internal double @__gpu_kernel_0(double %$p0, double %$p1)"],
    );
}

// ---- host operations [GPU-MATMUL] [GPU-BUFFER-LOAD] [GPU-BUFFER-TENSOR] ----

/// The runtime producer is called exactly once inside `f`, with the `io_error`
/// channel cleared immediately before and read immediately after, so the
/// `Error` message is the reason THIS call recorded [BUILTIN-FILE-ERRMSG].
fn assert_one_bracketed_call(ir: &str, symbol: &str) {
    let host = function_body(ir, "define internal i64 @f()");
    let call = format!("call i8* @{symbol}(");
    assert_eq!(host.matches(&call).count(), 1, "{host}");
    let at = host.find(&call).unwrap_or(host.len());
    let clear = host
        .rfind("call void @osp_io_error_clear()")
        .unwrap_or(host.len());
    let take = host.find("call i8* @osp_io_error_take()").unwrap_or(0);
    assert!(clear < at && at < take, "{host}");
}

#[test]
fn gpu_matmul_is_one_bracketed_runtime_call() {
    let ir = module(
        "fn f() = {\n\
               let a = toGpu([1.0, 2.0])\n\
               let m = gpuMatMul(a, a, 1, 2, 1) ?: a\n\
               gpuLength(m)\n\
             }\n\
             print(f())\n",
    );
    assert_one_bracketed_call(&ir, "osprey_gpu_matmul");
}

#[test]
fn gpu_load_is_one_bracketed_runtime_call() {
    let ir = module(
        "fn f() = {\n\
               let w = gpuLoad(\"w.bin\", 8, 2) ?: toGpu([0.0])\n\
               gpuLength(w)\n\
             }\n\
             print(f())\n",
    );
    assert_one_bracketed_call(&ir, "osprey_gpu_load_f32");
}

#[test]
fn gpu_load_tensor_is_one_bracketed_runtime_call() {
    let ir = module(
        "fn f() = {\n\
               let w = gpuLoadTensor(\"m.safetensors\", \"wte\") ?: toGpu([0.0])\n\
               gpuLength(w)\n\
             }\n\
             print(f())\n",
    );
    assert_one_bracketed_call(&ir, "osprey_gpu_load_tensor");
}

// ---- float functions [BUILTIN-MATH] ----

#[test]
fn sqrt_exp_log_are_llvm_intrinsics_and_tanh_is_libm() {
    let ir = module(
        "fn f(x) = sqrt(x) + exp(x) + ln(x) + tanh(x)\n\
             print(\"${f(2.0)}\")\n",
    );
    shows(
        &ir,
        &[
            "call double @llvm.sqrt.f64(double ",
            "call double @llvm.exp.f64(double ",
            "call double @llvm.log.f64(double ",
            "call double @tanh(double ",
            "declare double @tanh(double)",
        ],
    );
}

#[test]
fn a_float_function_by_name_is_an_extracted_kernel_in_both_flavors() {
    // `gpuMap(exp)` lifts the builtin into a scalar kernel whose body is the
    // intrinsic call — no closure cell, no environment [GPU-KERNEL-EXTRACT].
    let default_ir = module(
        "fn f() = {\n\
               let out = toGpu([1.0, 2.0]) |> gpuMap(exp)\n\
               gpuLength(out)\n\
             }\n\
             print(f())\n",
    );
    let ml_ir = ml_module(
        "f () =\n    \
                 out = toGpu [1.0, 2.0] |> gpuMap exp\n    \
                 gpuLength out\n\
             \n\
             print (f ())\n",
    );
    assert_eq!(default_ir, ml_ir);
    let header = "define internal double @__gpu_kernel_0(double %$p0)";
    assert!(default_ir.contains(header), "{default_ir}");
    let kernel = function_body(&default_ir, header);
    shows(&kernel, &["call double @llvm.exp.f64(double %$p0)"]);
    testkit::contents(&kernel, &[], &["__env"]);
}
