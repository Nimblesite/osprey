//! Structural conformance for [GPU-BACKEND-DEVICE]: every extracted kernel
//! also has a device form, every launch site asks the device first, and a
//! kernel the device dialect cannot express declines by name — identically in
//! both flavors.

use super::debug::lambda_debug_ir;
use super::Flavor;

/// The program's device kernels, decoded from the module constant the launch
/// sites hand the runtime.
fn kernel_source(ir: &str) -> String {
    let Some(start) = ir.find("@osprey.gpu.kernels = private unnamed_addr constant") else {
        return String::new();
    };
    let body = &ir[start..];
    let Some(open) = body.find("c\"") else {
        return String::new();
    };
    let escaped = &body[open + 2..];
    let mut text = String::new();
    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => {
                let code: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&code, 16) {
                    text.push(char::from(byte));
                }
            }
            other => text.push(other),
        }
    }
    text.trim_end_matches('\0').to_string()
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

/// The reasons every declined launch site hands `osprey_gpu_declined`.
fn declined_reasons(ir: &str) -> Vec<String> {
    let mut reasons = Vec::new();
    for line in ir.lines() {
        let Some(rest) = line.split("@.str.").nth(1) else {
            continue;
        };
        if !line.contains("private unnamed_addr constant") {
            continue;
        }
        let Some(open) = rest.find("c\"") else {
            continue;
        };
        let text = &rest[open + 2..];
        if text.contains("has no device form")
            || text.contains("no device representation")
            || text.contains("device cannot")
            || text.contains("host lowering")
        {
            reasons.push(text.trim_end_matches("\\00\"").to_string());
        }
    }
    reasons
}

/// Both flavors of a kernel workload over every shape the emitter lowers: a
/// capturing lambda with a `fold` over a `range`, a boolean `match`, a
/// `gpuGet ?: default` gather, a named function kernel, a builtin kernel, a
/// zip and a fold.
const DEVICE_KERNELS: [(&str, Flavor); 2] = [
    (
        "let width = 4\nlet widthF = 4.0\nfn at(m, i) = gpuGet(m, i) ?: 0.0\nfn addF(a, x) = a + x\nfn sumRange(x, start, n) = range(start, start + n) |> fold(0.0, fn(a, i) => a + at(x, i))\nfn main() = {\n    handle Arith {\n        overflow _ _ _ wrapped => wrapped\n        divideByZero _ _ => 0.0\n        remainderByZero _ => 0\n    }\n    match toGpu(range(0, 16)) {\n        Success { value } => {\n            let seeds = value\n            match gpuMap(seeds, fn(i) => toFloat(i) / widthF - 1.5) {\n                Success { value } => {\n                    let xs = value\n                    match gpuMap(xs, exp) {\n                        Success { value } => {\n                            let es = value\n                            match gpuZipWith(xs, es, addF) {\n                                Success { value } => {\n                                    let zs = value\n                                    let peak = fn(r) => match at(zs, r * width) > 0.0 { true => at(zs, r * width) * 2.0  false => 0.0 }\n                                    match toGpu(range(0, 4)) {\n                                        Success { value } => {\n                                            let rows = value\n                                            match gpuMap(rows, fn(r) => sumRange(zs, r * width, width) / widthF) {\n                                                Success { value } => {\n                                                    let means = value\n                                                    match gpuMap(rows, peak) {\n                                                        Success(_) => {\n                                                            match toGpu(range(0, 5)) {\n                                                                Success { value } => {\n                                                                    let counts = value\n                                                                    match gpuMap(counts, fn(i) => intDiv(i * 7, 2) % 5 + abs(0 - i)) {\n                                                                        Success(_) => {\n                                                                            gpuFold(means, 0.0, addF)\n                                                                        }\n                                                                        Error { message } => Error { message: message }\n                                                                    }\n                                                                }\n                                                                Error { message } => Error { message: message }\n                                                            }\n                                                        }\n                                                        Error { message } => Error { message: message }\n                                                    }\n                                                }\n                                                Error { message } => Error { message: message }\n                                            }\n                                        }\n                                        Error { message } => Error { message: message }\n                                    }\n                                }\n                                Error { message } => Error { message: message }\n                            }\n                        }\n                        Error { message } => Error { message: message }\n                    }\n                }\n                Error { message } => Error { message: message }\n            }\n        }\n        Error { message } => Error { message: message }\n    }\n}\n",
        Flavor::Default,
    ),
    (
        "width = 4\nwidthF = 4.0\nat (m, i) = gpuGet (m, i) ?: 0.0\naddF (a, x) = a + x\nsumRange (x, start, n) = range (start, start + n) |> fold (0.0, \\(a, i) => a + at (x, i))\nmain () =\n    handle Arith\n        overflow _ _ _ wrapped => wrapped\n        divideByZero _ _ => 0.0\n        remainderByZero _ => 0\n    match toGpu (range (0, 16))\n        Success seeds =>\n            match gpuMap seeds (\\i => toFloat i / widthF - 1.5)\n                Success xs =>\n                    match gpuMap xs exp\n                        Success es =>\n                            match gpuZipWith (xs, es, addF)\n                                Success zs =>\n                                    peak = \\r => match at (zs, r * width) > 0.0\n                                        true => at (zs, r * width) * 2.0\n                                        false => 0.0\n                                    match toGpu (range (0, 4))\n                                        Success rows =>\n                                            match gpuMap rows (\\r => sumRange (zs, r * width, width) / widthF)\n                                                Success means =>\n                                                    match gpuMap rows peak\n                                                        Success _ =>\n                                                            match toGpu (range (0, 5))\n                                                                Success counts =>\n                                                                    match gpuMap counts (\\i => intDiv (i * 7, 2) % 5 + abs (0 - i))\n                                                                        Success _ =>\n                                                                            gpuFold (means, 0.0, addF)\n                                                                        Error message => Error(message = message)\n                                                                Error message => Error(message = message)\n                                                        Error message => Error(message = message)\n                                                Error message => Error(message = message)\n                                        Error message => Error(message = message)\n                                Error message => Error(message = message)\n                        Error message => Error(message = message)\n                Error message => Error(message = message)\n        Error message => Error(message = message)\n",
        Flavor::Ml,
    ),
];

/// [GPU-BACKEND-DEVICE] Every launch site in the workload has a device kernel.
#[test]
fn every_launch_site_has_a_device_kernel_in_both_flavors() -> Result<(), String> {
    let mut sources = Vec::new();
    for (source, flavor) in DEVICE_KERNELS {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_eq!(
            count(&ir, "call i32 @osprey_gpu_launch("),
            7,
            "{flavor:?}: seven launch sites"
        );
        assert_eq!(
            count(&ir, "call i32 @osprey_gpu_declined("),
            0,
            "{flavor:?}: {:?}",
            declined_reasons(&ir)
        );
        let kernels = kernel_source(&ir);
        assert_eq!(
            count(&kernels, "kernel void osprey_kernel_"),
            7,
            "{flavor:?}"
        );
        assert_device_dialect(&kernels, flavor);
        sources.push(kernels);
    }
    assert_eq!(
        sources.first(),
        sources.get(1),
        "twins must emit the same device source"
    );
    Ok(())
}

/// The emitted dialect: checked integer helpers, binary64 library calls, the
/// in-order fold loop, the bounds-checked gather, the two entry shapes.
fn assert_device_dialect(kernels: &str, flavor: Flavor) {
    for needle in [
        "osprey_fdiv(from_long(",
        "osprey_mul(",
        "osprey_div(",
        "osprey_rem(",
        "osprey_abs(",
        "exp64(",
        "add64(",
        "sub64(",
        "multiply64(",
        "lt64(",
        "for (long ",
        ") {\n  ulong gid = GPU_INDEX;\n  if (gid >= count) return;",
        "if (GPU_INDEX != 0) return;",
        "GPU_GLOBAL const ulong *u",
        "_len) {",
        "out[gid] = ",
    ] {
        assert!(
            kernels.contains(needle),
            "{flavor:?}: missing `{needle}` in\n{kernels}"
        );
    }
}

/// [GPU-BACKEND-DEVICE] Shapes the dialect cannot express decline by name,
/// including captured strings inside otherwise supported scan/filter shapes.
#[test]
fn kernels_without_a_device_form_decline_by_name_in_both_flavors() -> Result<(), String> {
    for (source, flavor) in [
        (
            "fn main() = {\n    let label = \"x\"\n    let tag = fn(v) => match label == \"x\" { true => v  false => 0.0 }\n    match toGpu([1.0, 2.0]) {\n        Success { value } => {\n            let xs = value\n            match gpuMap(xs, tag) {\n                Success(_) => {\n                    match gpuScan(xs, 0.0, fn(a, v) => match label == \"x\" { true => a + v false => a }) {\n                        Success(_) => {\n                            gpuFilter(xs, fn(v) => match label == \"x\" { true => v > 1.0 false => false })\n                        }\n                        Error { message } => Error { message: message }\n                    }\n                }\n                Error { message } => Error { message: message }\n            }\n        }\n        Error { message } => Error { message: message }\n    }\n}\n",
            Flavor::Default,
        ),
        (
            "main () =\n    label = \"x\"\n    tag = \\v => match label == \"x\"\n        true => v\n        false => 0.0\n    scan = \\(a, v) => match label == \"x\"\n        true => a + v\n        false => a\n    keep = \\v => match label == \"x\"\n        true => v > 1.0\n        false => false\n    match toGpu [1.0, 2.0]\n        Success xs =>\n            match gpuMap xs tag\n                Success _ =>\n                    match gpuScan (xs, 0.0, scan)\n                        Success _ =>\n                            gpuFilter xs keep\n                        Error message => Error(message = message)\n                Error message => Error(message = message)\n        Error message => Error(message = message)\n",
            Flavor::Ml,
        ),
    ] {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_eq!(count(&ir, "call i32 @osprey_gpu_launch("), 0, "{flavor:?}");
        assert_eq!(count(&ir, "call i32 @osprey_gpu_declined("), 3, "{flavor:?}");
        let reasons = declined_reasons(&ir).join("\n");
        assert!(reasons.contains("gpuMap kernel: the kernel captures `label`"), "{flavor:?}: {reasons}");
        assert!(reasons.contains("Scan kernel: the kernel captures `label`"), "{flavor:?}: {reasons}");
        assert!(reasons.contains("Filter kernel: the kernel captures `label`"), "{flavor:?}: {reasons}");
        assert!(!ir.contains("@osprey.gpu.kernels ="), "{flavor:?}: no kernel source without a device kernel");
    }
    Ok(())
}

/// [GPU-KERNEL-EXTRACT] A kernel that reduces through a higher-order helper,
/// handing it a lambda over the kernel's own index, keeps its device form in
/// every flavor. The Python GPT-2 port spells every row reduction this way
/// (`add_all(WIDTH, lambda c: ...)`), and on Metal/OpenCL each such kernel
/// declined with "a function value has no device form".
#[test]
fn kernels_reducing_through_higher_order_helpers_keep_their_device_form() -> Result<(), String> {
    let mut sources = Vec::new();
    for (source, flavor) in [
        (
            "fn addAll(size, valueAt) = range(0, size) |> fold(0.0, fn(acc, i) => acc + valueAt(i))\nfn main() = match toGpu(range(0, 4)) {\n    Success { value } => gpuMap(value, fn(r) => addAll(3, fn(c) => toFloat(c) * toFloat(r)))\n    Error { message } => Error { message: message }\n}\n",
            Flavor::Default,
        ),
        (
            "addAll (size, valueAt) = range (0, size) |> fold (0.0, \\(acc, i) => acc + valueAt i)\nmain () = match toGpu (range (0, 4))\n    Success rows => gpuMap rows (\\r => addAll (3, \\c => toFloat c * toFloat r))\n    Error message => Error(message = message)\n",
            Flavor::Ml,
        ),
        (
            "def add_all(size, value_at):\n    return fold(range(size), 0.0, lambda acc, i: acc + value_at(i))\n\n\ndef main():\n    match toGpu(range(0, 4)):\n        case Success(value=rows):\n            return gpuMap(rows, lambda r: add_all(3, lambda c: toFloat(c) * toFloat(r)))\n        case Error(message=message):\n            return Error(message=message)\n",
            Flavor::Python,
        ),
    ] {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_eq!(count(&ir, "call i32 @osprey_gpu_declined("), 0, "{flavor:?}: {:?}", declined_reasons(&ir));
        assert_eq!(count(&ir, "call i32 @osprey_gpu_launch("), 1, "{flavor:?}");
        let kernels = kernel_source(&ir);
        assert!(kernels.contains("for (long "), "{flavor:?}: the helper's fold loop\n{kernels}");
        sources.push(kernels);
    }
    assert!(
        sources.windows(2).all(|pair| pair.first() == pair.last()),
        "every flavor must emit the same device source"
    );
    Ok(())
}

/// [GPU-KERNEL-EXTRACT] A kernel passed through a static handler arm's
/// binding (`let f = addF`, or a let-bound lambda) keeps its device form.
#[test]
fn kernels_bound_through_handler_arms_keep_their_device_form() -> Result<(), String> {
    for (source, flavor) in [
        (
            "static effect Tensor<B> {\n    build: fn(int, fn(int) -> float) -> Result<B, Error>\n    reduce: fn(B, float, fn(float, float) -> float) -> Result<float, Error>\n}\nfn onGpu(work) = {\n    handle static Tensor<GpuBuffer<float>> {\n        build n f => match toGpu(range(0, n)) {\n            Success { value } => gpuMap(value, f)\n            Error { message } => Error { message: message }\n        }\n        reduce b seed f => b |> gpuFold(seed, f)\n    }\n    work()\n}\nfn addF(a, x) = a + x\nfn total(b) = perform Tensor.reduce(b, 0.0, addF)\nfn squares(n) = perform Tensor.build(n, fn(i) => toFloat(i * i))\nfn run() = {\n    handle Arith {\n        overflow _ _ _ wrapped => wrapped\n        divideByZero _ _ => 0.0\n        remainderByZero _ => 0\n    }\n    match squares(4) {\n        Success { value } => total(value)\n        Error { message } => Error { message: message }\n    }\n}\nfn main() = onGpu(run)\n",
            Flavor::Default,
        ),
        (
            "static effect Tensor B\n    build : (int, (int) -> float) => Result<B, Error>\n    reduce : (B, float, (float, float) -> float) => Result<float, Error>\n\nonGpu work =\n    handle static Tensor<GpuBuffer<float>>\n        build n f => match toGpu (range (0, n))\n            Success xs => gpuMap xs f\n            Error message => Error(message = message)\n        reduce b seed f => b |> gpuFold seed f\n    work ()\n\naddF (a, x) = a + x\ntotal b = perform Tensor.reduce b 0.0 addF\nsquares n = perform Tensor.build n (\\i => toFloat (i * i))\nrun () =\n    handle Arith\n        overflow _ _ _ wrapped => wrapped\n        divideByZero _ _ => 0.0\n        remainderByZero _ => 0\n    match squares 4\n        Success xs => total xs\n        Error message => Error(message = message)\n\nmain () = onGpu run\n",
            Flavor::Ml,
        ),
    ] {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_eq!(count(&ir, "call i32 @osprey_gpu_declined("), 0, "{flavor:?}: {:?}", declined_reasons(&ir));
        assert_eq!(count(&ir, "call i32 @osprey_gpu_launch("), 2, "{flavor:?}");
        let kernels = kernel_source(&ir);
        assert!(kernels.contains("acc = add64(p0, p1);"), "{flavor:?}: the named fold combine\n{kernels}");
        assert!(kernels.contains("osprey_mul(p0, p0, fault)"), "{flavor:?}: the let-bound map lambda\n{kernels}");
    }
    Ok(())
}

/// A program mapping `[1, 2]` with `kernel`, where `add` reads the global `bias`.
fn shadowing_program(flavor: Flavor, kernel: &str, helpers: &str) -> String {
    match flavor {
        Flavor::Ml => format!(
            "bias = 10\nadd x = wrapAdd x bias\n{helpers}\nrun () = match toGpu [1, 2]\n    Success value => match gpuMap value ({kernel})\n        Success mapped => print \"${{gpuLength mapped}}\"\n        Error message => print message\n    Error message => print message\nrun ()\n"
        ),
        _ => format!(
            "let bias = 10\nfn add(x) = wrapAdd(x, bias)\n{helpers}\nmatch toGpu([1, 2]) {{\n    Success {{ value }} => match gpuMap(value, {kernel}) {{\n        Success {{ value }} => print(\"${{gpuLength(value)}}\")\n        Error {{ message }} => print(message)\n    }}\n    Error {{ message }} => print(message)\n}}\n"
        ),
    }
}

/// [GPU-KERNEL-EXTRACT] A named helper is defined at module scope, so the names in
/// its body mean what they mean there. Inlined into a kernel, `add`'s `bias` stayed
/// the global when the caller also bound a `bias`: as the kernel's parameter, a local
/// or a helper's parameter. Metal answered 2 for element 1 where the host answers 11.
#[test]
fn a_callers_binding_never_shadows_a_name_in_a_helper_it_inlines() -> Result<(), String> {
    let rebound = [
        (Flavor::Default, "fn(x) => add(x)", ""),
        (Flavor::Ml, "\\x => add x", ""),
        (Flavor::Default, "fn(bias) => add(bias)", ""),
        (Flavor::Ml, "\\bias => add bias", ""),
        (Flavor::Default, "fn(x) => { let bias = x\n add(bias) }", ""),
        (
            Flavor::Ml,
            "\\x =>\n            bias = x\n            add bias",
            "",
        ),
        (
            Flavor::Default,
            "fn(x) => outer(x)",
            "fn outer(bias) = add(bias)",
        ),
        (Flavor::Ml, "\\x => outer x", "outer bias = add bias"),
        (
            Flavor::Default,
            "fn(bias) => outer(bias)",
            "fn outer(y) = add(y)",
        ),
        (Flavor::Ml, "\\bias => outer bias", "outer y = add y"),
    ];
    let mut wrong = Vec::new();
    for (flavor, kernel, helpers) in rebound {
        let ir = lambda_debug_ir(&shadowing_program(flavor, kernel, helpers), flavor)?;
        assert_eq!(
            count(&ir, "call i32 @osprey_gpu_declined("),
            0,
            "{kernel}: {:?}",
            declined_reasons(&ir)
        );
        let kernels = kernel_source(&ir);
        let answer = kernels
            .lines()
            .find(|line| line.contains("out[gid] = "))
            .unwrap_or_default();
        if !answer.contains("+ (ulong)(((long)10))") {
            wrong.push(format!("{flavor:?} {kernel}: {answer}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "`add` must add the global 10:\n{}",
        wrong.join("\n")
    );
    Ok(())
}
