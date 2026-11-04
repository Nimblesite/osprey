use super::*;
use osprey_types::numeric::Numeric;

const DENSE_ROWS: [(&str, &str, i64, &str); 11] = [
    ("uint8", "255", 1, "uchar"),
    ("int8", "-128", 1, "char"),
    ("bool", "true", 1, "bool"),
    ("int16", "-32768", 2, "short"),
    ("uint16", "65535", 2, "ushort"),
    ("float16", "1.5", 2, "half"),
    ("int32", "-2147483648", 4, "int"),
    ("uint32", "4294967295", 4, "uint"),
    ("float32", "1.5", 4, "float"),
    ("int", "42", 8, "long"),
    ("float", "1.5", 8, "ulong"),
];

const NARROW_INTEGER_TARGETS: [(&str, &str); 6] = [
    ("Int8", "_i8"),
    ("Int16", "_i16"),
    ("Int32", "_i32"),
    ("Uint8", "_u8"),
    ("Uint16", "_u16"),
    ("Uint32", "_u32"),
];

fn conversion_program(source: &str, builtin: &str, values: &str) -> String {
    format!("let policy = handler Arith {{ overflow _ _ _ wrapped => wrapped }}\nfn main() = policy(fn() => {{\nlet values: List<{source}> = [{values}]\nlet out = gpuMap(toGpu(values), {builtin})\nprint(gpuLength(out))\n}})\n")
}

fn assert_conversion_pair(source: Numeric, target: Numeric) {
    let name = target.name();
    let builtin = format!("to{}{}", name[..1].to_uppercase(), &name[1..]);
    let ty = source.name();
    let values = if source.is_float() {
        "0.0, 1.0, 2.0"
    } else {
        "0, 1, 2"
    };
    let ir = module(&conversion_program(ty, &builtin, values));
    assert!(!ir.contains("@osprey_gpu_declined"), "{ty} -> {name}: {ir}");
    shows(&ir, &["call i32 @osprey_gpu_launch("]);
}

#[test]
fn every_valid_numeric_conversion_pair_has_a_device_form() {
    for source in Numeric::ALL {
        for target in Numeric::ALL {
            if !(source.is_float() && target.is_integer()) {
                assert_conversion_pair(source, target);
            }
        }
    }
}

#[test]
fn captured_half_uniforms_convert_from_their_binary32_compute_type() {
    // [GPU-DEVICE-NATIVE]: a half uniform is stored as bits but bound as float.
    for target in ["Float", "Float32", "Float16"] {
        let ir = module(&format!("fn main() = {{\nlet seed = toFloat16(1.5)\nlet out = gpuMap(toGpu([0]), fn(_index) => to{target}(seed))\nprint(gpuLength(out))\n}}\n"));
        assert!(!ir.contains("@osprey_gpu_declined"), "to{target}: {ir}");
    }
}

#[test]
fn device_integer_conversions_record_overflow_instead_of_silently_wrapping() {
    // [BUILTIN-CONVERT-INT] [GPU-DEVICE-FAULT]: the source must reach the
    // checked conversion before a narrow cast can discard its high bits.
    for (target, suffix) in NARROW_INTEGER_TARGETS {
        let ir = module(&conversion_program(
            "int",
            &format!("to{target}"),
            "0, 128, 4294967296",
        ));
        shows(
            &ir,
            &[&format!("osprey_convert{suffix}((long)(p0), fault)")],
        );
    }
}

#[test]
fn dense_buffer_allocations_follow_the_inferred_element_width() {
    // [GPU-BUFFER-ELEM]: storage width is independent of the boxed list ABI.
    for (ty, value, width, storage) in DENSE_ROWS {
        let ir = module(&format!("let values: List<{ty}> = [{value}]\nlet buffer = toGpu(values)\nlet gathered = gpuZipWith(gpuIota(1), buffer, fn(index, value) => gpuGet(buffer, index) ?: value)\nprint(fromGpu(gathered))\n"));
        let call = ir
            .lines()
            .find(|line| line.contains("call i8* @osprey_gpu_alloc("))
            .expect("allocation");
        assert!(call.ends_with(&format!(", i64 {width})")), "{ty}: {call}");
        for parameter in [
            format!("GPU_GLOBAL const {storage} *in1"),
            format!("GPU_GLOBAL {storage} *out"),
            format!("GPU_GLOBAL const {storage} *u0"),
        ] {
            shows(&ir, &[&parameter]);
        }
    }
}

#[test]
fn boolean_device_inputs_and_captured_buffers_use_byte_storage() {
    // [GPU-BUFFER-ELEM]: a word pointer would stride eight byte-sized booleans.
    let ir = module("let values: List<bool> = [true, false]\nlet table = toGpu(values)\nlet mapped = gpuMap(table, fn(value) => value && (gpuGet(table, 0) ?: false))\nprint(fromGpu(mapped))\n");
    shows(
        &ir,
        &["GPU_GLOBAL const bool *in0", "GPU_GLOBAL const bool *u0"],
    );
}

#[test]
fn foreign_intrinsics_keep_host_identity_and_refuse_device_emission() {
    let ir = module("extern fn abs(value: int) -> int\nlet answer = abs(-3)\n");
    shows(&ir, &["call i64 @abs(i64"]);
    testkit::contents(&ir, &[], &["osprey_abs(p0, fault)"]);
    let error = compile_err("extern fn abs(value: int) -> int\nlet out = toGpu([-3, 4]) |> gpuMap(fn(x) => abs(x))\nprint(gpuLength(out))\n");
    assert!(error
        .to_string()
        .contains("foreign function `abs` has no device form"));
}

#[test]
fn foreign_calls_fail_before_each_gpu_launch_form() {
    for expression in [
        "gpuMap(toGpu([-3, 4]), abs)",
        "gpuMap(toGpu([-3, 4]), foreign)",
        "gpuMap(toGpu([-3, 4]), fn(value) => helper(value))",
        "gpuZipWith(toGpu([-3]), toGpu([4]), fn(a, b) => abs(wrapAdd(a, b)))",
        "gpuFold(toGpu([-3, 4]), 0, fn(total, value) => abs(wrapAdd(total, value)))",
    ] {
        let error = compile_err(&format!("extern fn abs(value: int) -> int\nfn helper(value) = abs(value)\nlet foreign = abs\nlet answer = {expression}\n"));
        assert!(error
            .to_string()
            .contains("foreign function `abs` has no device form"));
    }
}

#[test]
fn foreign_kernel_aliases_do_not_capture_a_nearer_local_function() {
    let ir = module("extern fn importedWord(value: int) -> int\nfn main() = {\nlet importedWord = fn(value) => wrapMul(value, 2)\nlet selected = importedWord\nlet out = gpuMap(toGpu([3]), selected)\nprint(gpuLength(out))\n}\n");
    shows(&ir, &["mul i64"]);
    testkit::contents(&ir, &[], &["call i64 @importedWord("]);
}

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
