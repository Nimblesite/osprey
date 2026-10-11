//! Structural conformance for [GPU-KERNEL-EXTRACT].

use super::debug::{assert_lambda_variables, lambda_debug_ir};
use super::Flavor;

/// Ratchets [GPU-KERNEL-EXTRACT] independently of runtime output equivalence.
pub(super) fn check_extraction_floor(label: &str, ir: &str) -> Result<(), String> {
    let stem = std::path::Path::new(label)
        .file_stem()
        .and_then(|s| s.to_str());
    let Some((_, floor)) = EXTRACTION_FLOORS
        .iter()
        .find(|(name, _)| Some(*name) == stem)
    else {
        return Ok(());
    };
    let count = ir
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("@__gpu_kernel_"))
        .count();
    if count < *floor {
        return Err(format!(
            "{label}: extracted {count} kernels, below the required {floor}"
        ));
    }
    Ok(())
}

const EXTRACTION_FLOORS: [(&str, usize); 8] = [
    ("buffers.test", 28),
    ("combinators.test", 16),
    ("gamedev.test", 17),
    ("kernel_frontier.test", 23),
    ("mlkernels.test", 85),
    ("raster.test", 5),
    ("scalar_contracts.test", 4),
    ("stress.test", 18),
];

/// [GPU-KERNEL-EXTRACT] Immutable lexical captures become explicit uniforms.
#[test]
fn closed_generic_kernels_extract_with_their_definition_environment() -> Result<(), String> {
    assert_closed_kernel(CLOSED_KERNELS[0])
}

#[test]
fn ml_closed_generic_kernels_extract_with_their_definition_environment() -> Result<(), String> {
    assert_closed_kernel(CLOSED_KERNELS[1])
}

fn assert_closed_kernel((source, flavor): (&str, Flavor)) -> Result<(), String> {
    let ir = lambda_debug_ir(source, flavor)?;
    let definitions: Vec<_> = ir
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("@__gpu_kernel_"))
        .collect();
    assert_eq!(definitions.len(), 5, "{flavor:?}: {definitions:?}");
    assert_closed_abi(&definitions);
    assert!(ir.contains("!DILocalVariable(name: \"enabled\""));
    assert_lambda_variables(&ir, "__gpu_kernel_", &["enabled"])?;
    Ok(())
}

fn assert_closed_abi(definitions: &[&str]) {
    assert!(
        definitions.iter().all(|line| line.contains("i1 %$p0")),
        "lexical boolean must be the leading uniform: {definitions:?}"
    );
    assert!(
        definitions.iter().any(|line| line.contains("double %$p1")),
        "float slots must remain floats: {definitions:?}"
    );
}

const CLOSED_KERNELS: [(&str, Flavor); 2] = [
    ("fn main() = {\n let enabled = true\n let keep = fn(x) => enabled\n let choose = fn(a, x) => match enabled { true => a false => x }\n let identity = fn(x) => match enabled { true => x false => x }\n let enabled = false\n let xs = toGpu([1.0, 2.0])\n let mapped = gpuMap(xs, identity)\n let filtered = gpuFilter(xs, keep)\n let zipped = gpuZipWith(xs, xs, choose)\n let folded = gpuFold(xs, 0.0, choose)\n let scanned = gpuScan(xs, 0.0, choose)\n print(folded)\n}\n", Flavor::Default),
    ("main () =\n    enabled = true\n    keep = \\x => enabled\n    choose = \\(a, x) => match enabled\n        true => a\n        false => x\n    identity = \\x => match enabled\n        true => x\n        false => x\n    enabled = false\n    xs = toGpu [1.0, 2.0]\n    mapped = gpuMap xs identity\n    filtered = gpuFilter xs keep\n    zipped = gpuZipWith (xs, xs, choose)\n    folded = gpuFold (xs, 0.0, choose)\n    scanned = gpuScan (xs, 0.0, choose)\n    print folded\n", Flavor::Ml),
];

#[test]
fn extraction_floors_require_definitions_and_both_fixture_flavors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/core/gpu");
    for (stem, _) in EXTRACTION_FLOORS {
        for extension in ["osp", "ospml"] {
            let label = format!("{stem}.{extension}");
            assert!(
                root.join(&label).is_file(),
                "missing extraction fixture {label}"
            );
            assert!(check_extraction_floor(
                &label,
                "declare i64 @__gpu_kernel_0()\n  %v = call i64 @__gpu_kernel_0()\n"
            )
            .is_err());
        }
    }
}

/// [GPU-KERNEL-EXTRACT] Lifted kernels preserve source scopes and uniform values.
#[test]
fn extracted_kernel_debug_scopes_keep_uniforms_and_source_parameters() -> Result<(), String> {
    for (source, flavor) in [
        ("fn main() = {\n    let n = 2\n    let result = gpuMap(toGpu([40]), fn(x) => {\n        let sum = wrapAdd(x, n)\n        sum\n    })\n    print(gpuGet(result, 0) ?: -1)\n}\n", Flavor::Default),
        ("main () =\n    n = 2\n    result = gpuMap (toGpu [40]) (\\x =>\n        sum = wrapAdd x n\n        sum)\n    print (gpuGet (result, 0) ?: -1)\n", Flavor::Ml),
    ] {
        let ir = lambda_debug_ir(source, flavor)?;
        assert_lambda_variables(&ir, "__gpu_kernel_", &["x", "n", "sum"])?;
        assert!(ir.contains("!DILocalVariable(name: \"x\", arg: 2,"));
        assert!(ir.contains("!DILocation(line: 5,"));
    }
    Ok(())
}

/// [DEBUGGER-LAMBDA-SCOPES] Kernel combinators share the parameter/location contract.
#[test]
fn every_extracting_combinator_preserves_its_source_arguments() -> Result<(), String> {
    for (default, ml, variables) in KERNEL_COMBINATORS {
        for (source, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
            let ir = kernel_debug_ir(source, flavor)?;
            assert_lambda_variables(&ir, "__gpu_kernel_", variables)?;
            for (index, name) in variables.iter().enumerate() {
                let arg = index + 1;
                assert!(ir.contains(&format!("!DILocalVariable(name: \"{name}\", arg: {arg},")));
            }
        }
    }
    Ok(())
}

fn kernel_debug_ir(source: &str, flavor: Flavor) -> Result<String, String> {
    let wrapped = match flavor {
        Flavor::Default => format!("fn main() = {{\n let result = {source}\n print(0)\n}}"),
        Flavor::Ml => format!("main () =\n    result = {source}\n    print 0"),
    };
    lambda_debug_ir(&wrapped, flavor)
}

const KERNEL_COMBINATORS: [(&str, &str, &[&str]); 5] = [
    (
        "gpuMap(toGpu([1]), fn(x) => wrapAdd(x, 2))",
        "gpuMap (toGpu [1]) (\\x => wrapAdd x 2)",
        &["x"],
    ),
    (
        "gpuFilter(toGpu([1]), fn(x) => x > 0)",
        "gpuFilter (toGpu [1]) (\\x => x > 0)",
        &["x"],
    ),
    (
        "gpuZipWith(toGpu([1]), toGpu([2]), fn(x, y) => wrapAdd(x, y))",
        "gpuZipWith (toGpu [1], toGpu [2], \\(x, y) => wrapAdd x y)",
        &["x", "y"],
    ),
    (
        "gpuFold(toGpu([1]), 0, fn(x, y) => wrapAdd(x, y))",
        "gpuFold (toGpu [1], 0, \\(x, y) => wrapAdd x y)",
        &["x", "y"],
    ),
    (
        "gpuScan(toGpu([1]), 0, fn(x, y) => wrapAdd(x, y))",
        "gpuScan (toGpu [1], 0, \\(x, y) => wrapAdd x y)",
        &["x", "y"],
    ),
];

/// [GPU-KERNEL-EXTRACT] Uniforms are sorted, then element/accumulator slots.
#[test]
fn zip_scan_and_filter_have_exact_scalar_abis() -> Result<(), String> {
    for (default, ml, signature, variables) in SCALAR_ABIS {
        for (expression, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
            let source = match flavor {
                Flavor::Default => format!("fn main() = {{\n let zebra = 1.5\n let alpha = 2\n let result = {expression}\n print(0)\n}}"),
                Flavor::Ml => format!("main () =\n    zebra = 1.5\n    alpha = 2\n    result = {expression}\n    print 0"),
            };
            let ir = lambda_debug_ir(&source, flavor)?;
            assert!(ir.contains(signature), "{flavor}: missing ABI {signature}");
            assert_lambda_variables(&ir, "__gpu_kernel_", variables)?;
            assert!(ir.contains("@__gpu_kernel_0(i64 2, double 0x3FF8000000000000,"));
        }
    }
    Ok(())
}

const SCALAR_ABIS: [(&str, &str, &str, &[&str]); 3] = [
    (
        "gpuZipWith(toGpu([1]), toGpu([2.0]), fn(x, y) => toFloat(wrapAdd(x, alpha)) * y * zebra)",
        "gpuZipWith (toGpu [1], toGpu [2.0], \\(x, y) => toFloat (wrapAdd x alpha) * y * zebra)",
        "define double @__gpu_kernel_0(i64 %$p0, double %$p1, i64 %$p2, double %$p3)",
        &["alpha", "zebra", "x", "y"],
    ),
    (
        "gpuScan(toGpu([1.0]), 0.0, fn(x, y) => x * zebra + y * toFloat(alpha))",
        "gpuScan (toGpu [1.0], 0.0, \\(x, y) => x * zebra + y * toFloat alpha)",
        "define double @__gpu_kernel_0(i64 %$p0, double %$p1, double %$p2, double %$p3)",
        &["alpha", "zebra", "x", "y"],
    ),
    (
        "gpuFilter(toGpu([1.0]), fn(x) => x * zebra > 0.0 && alpha > 0)",
        "gpuFilter (toGpu [1.0]) (\\x => x * zebra > 0.0 && alpha > 0)",
        "define i1 @__gpu_kernel_0(i64 %$p0, double %$p1, double %$p2)",
        &["alpha", "zebra", "x"],
    ),
];

fn buffer_ir(default: &str, ml: &str) -> Result<Vec<String>, String> {
    [(default, Flavor::Default), (ml, Flavor::Ml)]
        .into_iter()
        .map(|(expression, flavor)| {
            let source = match flavor {
                Flavor::Default => format!("fn main() = gpuLength({expression})"),
                Flavor::Ml => format!("main () = gpuLength ({expression})"),
            };
            super::ir_for(&source, flavor, "buffer.osp")
        })
        .collect()
}

fn calls(ir: &str, name: &str) -> usize {
    ir.lines()
        .filter(|line| line.contains("call ") && line.contains(name))
        .count()
}

/// [GPU-BUFFER-LITERAL] Dense construction has no intermediate list or copy loop.
#[test]
fn gpu_literals_make_one_buffer_with_constant_stores() -> Result<(), String> {
    for ir in buffer_ir("toGpu([10, 20, 30, 40])", "toGpu [10, 20, 30, 40]")? {
        assert_eq!(calls(&ir, "@osprey_gpu_alloc("), 1);
        assert_eq!(calls(&ir, "@osprey_gpu_set("), 4);
        assert_eq!(calls(&ir, "@osprey_list_"), 0);
        assert_eq!(calls(&ir, "@osp_alloc("), 0, "no flat list allocation");
        assert_eq!(calls(&ir, "@osprey_gpu_from_list("), 0);
        assert!(!ir.contains("br label"), "a literal needs no copy loop");
        for (index, value) in [10, 20, 30, 40].into_iter().enumerate() {
            assert!(ir.contains(&format!(", i64 {index}, i64 {value})")));
        }
    }
    Ok(())
}

/// [GPU-BUFFER-FUSE] Range, filter and map feed one buffer in one counted loop.
#[test]
fn gpu_iterator_pipelines_do_not_materialize_intermediate_collections() -> Result<(), String> {
    for ir in buffer_ir(
        "range(0, 10) |> filter(fn(x) => x % 2 == 0) |> map(fn(x) => wrapMul(x, x)) |> toGpu()",
        "range (0, 10) |> filter (\\x => x % 2 == 0) |> map (\\x => wrapMul x x) |> toGpu ()",
    )? {
        assert_eq!(calls(&ir, "@osprey_gpu_alloc("), 1);
        assert_eq!(calls(&ir, "@osprey_gpu_set("), 1);
        assert_eq!(calls(&ir, "@osprey_gpu_take("), 1);
        assert_eq!(calls(&ir, "@osprey_list_"), 0);
        assert_eq!(calls(&ir, "@osp_alloc("), 1, "only the range descriptor");
        assert!(!ir.contains("{ i64, i8* }"), "no flat list header");
        assert_eq!(calls(&ir, "@osprey_gpu_from_list("), 0);
        assert_eq!(backward_branches(&ir), 1, "one range loop");
        assert!(ir.contains(" = srem i64 "), "filter is fused");
        assert!(ir.contains(" = mul i64 "), "map is fused");
    }
    Ok(())
}

fn backward_branches(ir: &str) -> usize {
    let mut labels = std::collections::BTreeSet::new();
    let mut count = 0;
    for line in ir.lines().map(str::trim) {
        if let Some(label) = line.strip_suffix(':') {
            let _ = labels.insert(label);
        } else if line.starts_with("br ") {
            count += line
                .split("label %")
                .skip(1)
                .filter(|target| labels.contains(target.trim_end_matches(',')))
                .count();
        }
    }
    count
}

/// [GPU-KERNEL-EXTRACT] Intrinsics passed as kernels need real scalar definitions.
#[test]
fn builtin_kernels_are_extracted_with_specialized_scalar_abis() -> Result<(), String> {
    for (default, ml, signature, instruction) in BUILTIN_KERNELS {
        for ir in buffer_ir(default, ml)? {
            assert!(
                ir.contains(signature),
                "missing builtin kernel {signature}: {ir}"
            );
            assert!(
                ir.contains(instruction),
                "missing intrinsic operation {instruction}"
            );
            assert_eq!(calls(&ir, "@__gpu_kernel_0("), 1);
            assert!(
                !ir.contains("@__closure_"),
                "scalar builtins need no closure"
            );
        }
    }
    Ok(())
}

const BUILTIN_KERNELS: [(&str, &str, &str, &str); 4] = [
    (
        "gpuMap(toGpu([1]), toFloat)",
        "gpuMap (toGpu [1]) toFloat",
        "define double @__gpu_kernel_0(i64 %$p0)",
        "sitofp i64",
    ),
    (
        "gpuMap(toGpu([-1.5]), abs)",
        "gpuMap (toGpu [-1.5]) abs",
        "define double @__gpu_kernel_0(double %$p0)",
        "@llvm.fabs.f64(",
    ),
    (
        "gpuZipWith(toGpu([1]), toGpu([2]), wrapAdd)",
        "gpuZipWith (toGpu [1], toGpu [2], wrapAdd)",
        "define i64 @__gpu_kernel_0(i64 %$p0, i64 %$p1)",
        "add i64",
    ),
    (
        "gpuScan(toGpu([1]), 0, satMul)",
        "gpuScan (toGpu [1], 0, satMul)",
        "define i64 @__gpu_kernel_0(i64 %$p0, i64 %$p1)",
        "mul i128",
    ),
];
