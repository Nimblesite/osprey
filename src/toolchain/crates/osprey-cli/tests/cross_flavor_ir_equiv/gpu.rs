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

const EXTRACTION_FLOORS: [(&str, usize); 9] = [
    ("buffers.test", 28),
    ("combinators.test", 16),
    ("gamedev.test", 17),
    ("kernel_frontier.test", 25),
    ("mlkernels.test", 85),
    ("raster.test", 5),
    ("scalar_contracts.test", 6),
    ("stress.test", 18),
    ("transformer.test", 22),
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
    (
        r"fn main() = {
    let enabled = true
    let keep = fn(x) => enabled
    let choose = fn(a, x) => match enabled { true => a false => x }
    let identity = fn(x) => match enabled { true => x false => x }
    let enabled = false
    match toGpu([1.0, 2.0]) {
        Success { value } => {
            let xs = value
            match gpuMap(xs, identity) {
                Success(_) => match gpuFilter(xs, keep) {
                    Success(_) => match gpuZipWith(xs, xs, choose) {
                        Success(_) => match gpuScan(xs, 0.0, choose) {
                            Success(_) => gpuFold(xs, 0.0, choose)
                            Error { message } => Error { message: message }
                        }
                        Error { message } => Error { message: message }
                    }
                    Error { message } => Error { message: message }
                }
                Error { message } => Error { message: message }
            }
        }
        Error { message } => Error { message: message }
    }
}
",
        Flavor::Default,
    ),
    (
        r"main () =
    enabled = true
    keep = \x => enabled
    choose = \(a, x) => match enabled
        true => a
        false => x
    identity = \x => match enabled
        true => x
        false => x
    enabled = false
    match toGpu [1.0, 2.0]
        Success xs => match gpuMap xs identity
            Success _ => match gpuFilter xs keep
                Success _ => match gpuZipWith (xs, xs, choose)
                    Success _ => match gpuScan (xs, 0.0, choose)
                        Success _ => gpuFold (xs, 0.0, choose)
                        Error message => Error(message = message)
                    Error message => Error(message = message)
                Error message => Error(message = message)
            Error message => Error(message = message)
        Error message => Error(message = message)
",
        Flavor::Ml,
    ),
];

#[test]
fn extraction_floors_require_definitions_and_both_fixture_flavors() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../osprey/tests/core/gpu");
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
        (
            r"fn main() = {
    let n = 2
    match toGpu([40]) {
        Success { value } => gpuMap(value, fn(x) => {
            let sum = wrapAdd(x, n)
            sum
        })
        Error { message } => Error { message: message }
    }
}
",
            Flavor::Default,
        ),
        (
            r"main () =
    n = 2
    match toGpu [40]
        Success xs => gpuMap xs (\x =>
            sum = wrapAdd x n
            sum)
        Error message => Error(message = message)
",
            Flavor::Ml,
        ),
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
    for (default, ml, inputs, variables) in KERNEL_COMBINATORS {
        for (source, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
            let ir = kernel_debug_ir(source, flavor, inputs, false)?;
            assert_lambda_variables(&ir, "__gpu_kernel_", variables)?;
            for (index, name) in variables.iter().enumerate() {
                let arg = index + 1;
                assert!(ir.contains(&format!("!DILocalVariable(name: \"{name}\", arg: {arg},")));
            }
        }
    }
    Ok(())
}

fn kernel_debug_ir(
    source: &str,
    flavor: Flavor,
    inputs: &[(&str, &str)],
    uniforms: bool,
) -> Result<String, String> {
    let body = with_uploads(source, flavor, inputs);
    let setup = match (flavor, uniforms) {
        (Flavor::Default, true) => "let zebra = 1.5\nlet alpha = 2\n",
        (Flavor::Ml, true) => "zebra = 1.5\nalpha = 2\n",
        _ => "",
    };
    let wrapped = match flavor {
        Flavor::Default => format!("fn main() = {{\n{setup}{body}\n}}"),
        Flavor::Ml => format!("main () =\n{}", indent(&format!("{setup}{body}"), 4)),
        Flavor::Python => return Err("GPU debug fixture requires Default or ML".to_owned()),
    };
    lambda_debug_ir(&wrapped, flavor)
}

fn indent(source: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn with_uploads(source: &str, flavor: Flavor, inputs: &[(&str, &str)]) -> String {
    inputs.iter().rev().fold(source.to_owned(), |body, (name, values)| {
        match flavor {
            Flavor::Default => format!(
                "match toGpu([{values}]) {{\n    Success {{ value }} => {{\n        let {name} = value\n{}\n    }}\n    Error {{ message }} => Error {{ message: message }}\n}}",
                indent(&body, 8)
            ),
            Flavor::Ml => format!(
                "match toGpu [{values}]\n    Success {name} =>\n{}\n    Error message => Error(message = message)",
                indent(&body, 8)
            ),
            Flavor::Python => body,
        }
    })
}

type FixtureInputs = &'static [(&'static str, &'static str)];

const INT_INPUT: FixtureInputs = &[("left", "1")];
const FLOAT_INPUT: FixtureInputs = &[("left", "1.0")];
const INT_INPUTS: FixtureInputs = &[("left", "1"), ("right", "2")];
const MIXED_INPUTS: FixtureInputs = &[("left", "1"), ("right", "2.0")];

const KERNEL_COMBINATORS: [(&str, &str, FixtureInputs, &[&str]); 5] = [
    (
        "gpuMap(left, fn(x) => wrapAdd(x, 2))",
        "gpuMap left (\\x => wrapAdd x 2)",
        INT_INPUT,
        &["x"],
    ),
    (
        "gpuFilter(left, fn(x) => x > 0)",
        "gpuFilter left (\\x => x > 0)",
        INT_INPUT,
        &["x"],
    ),
    (
        "gpuZipWith(left, right, fn(x, y) => wrapAdd(x, y))",
        "gpuZipWith (left, right, \\(x, y) => wrapAdd x y)",
        INT_INPUTS,
        &["x", "y"],
    ),
    (
        "gpuFold(left, 0, fn(x, y) => wrapAdd(x, y))",
        "gpuFold (left, 0, \\(x, y) => wrapAdd x y)",
        INT_INPUT,
        &["x", "y"],
    ),
    (
        "gpuScan(left, 0, fn(x, y) => wrapAdd(x, y))",
        "gpuScan (left, 0, \\(x, y) => wrapAdd x y)",
        INT_INPUT,
        &["x", "y"],
    ),
];

/// [GPU-KERNEL-EXTRACT] Uniforms are sorted, then element/accumulator slots.
#[test]
fn zip_scan_and_filter_have_exact_scalar_abis() -> Result<(), String> {
    for (default, ml, inputs, signature, variables) in SCALAR_ABIS {
        for (expression, flavor) in [(default, Flavor::Default), (ml, Flavor::Ml)] {
            let ir = kernel_debug_ir(expression, flavor, inputs, true)?;
            assert!(ir.contains(signature), "{flavor}: missing ABI {signature}");
            assert_lambda_variables(&ir, "__gpu_kernel_", variables)?;
            assert!(ir.contains("@__gpu_kernel_0(i64 2, double 0x3FF8000000000000,"));
        }
    }
    Ok(())
}

const SCALAR_ABIS: [(&str, &str, FixtureInputs, &str, &[&str]); 3] = [
    (
        "gpuZipWith(left, right, fn(x, y) => toFloat(wrapAdd(x, alpha)) * y * zebra)",
        "gpuZipWith (left, right, \\(x, y) => toFloat (wrapAdd x alpha) * y * zebra)",
        MIXED_INPUTS,
        "define internal double @__gpu_kernel_0(i64 %$p0, double %$p1, i64 %$p2, double %$p3)",
        &["alpha", "zebra", "x", "y"],
    ),
    (
        "gpuScan(left, 0.0, fn(x, y) => x * zebra + y * toFloat(alpha))",
        "gpuScan (left, 0.0, \\(x, y) => x * zebra + y * toFloat alpha)",
        FLOAT_INPUT,
        "define internal double @__gpu_kernel_0(i64 %$p0, double %$p1, double %$p2, double %$p3)",
        &["alpha", "zebra", "x", "y"],
    ),
    (
        "gpuFilter(left, fn(x) => x * zebra > 0.0 && alpha > 0)",
        "gpuFilter left (\\x => x * zebra > 0.0 && alpha > 0)",
        FLOAT_INPUT,
        "define internal i1 @__gpu_kernel_0(i64 %$p0, double %$p1, double %$p2)",
        &["alpha", "zebra", "x"],
    ),
];

fn buffer_ir(default: &str, ml: &str, inputs: &[(&str, &str)]) -> Result<Vec<String>, String> {
    [(default, Flavor::Default), (ml, Flavor::Ml)]
        .into_iter()
        .map(|(expression, flavor)| {
            let body = with_uploads(expression, flavor, inputs);
            let source = match flavor {
                Flavor::Default => format!("fn main() = {{\n{body}\n}}"),
                Flavor::Ml => format!("main () =\n{}", indent(&body, 4)),
                Flavor::Python => {
                    return Err("GPU buffer fixture requires Default or ML".to_owned())
                }
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
    for ir in buffer_ir("toGpu([10, 20, 30, 40])", "toGpu [10, 20, 30, 40]", &[])? {
        assert_eq!(calls(&ir, "@osprey_gpu_alloc("), 1);
        assert_eq!(calls(&ir, "@osprey_gpu_set("), 4);
        assert_eq!(calls(&ir, "@osprey_list_"), 0);
        assert_eq!(calls(&ir, "@osp_alloc("), 0, "no flat list allocation");
        assert_eq!(calls(&ir, "@osprey_gpu_from_list("), 0);
        // The allocation Result can branch forward, but the literal cannot loop.
        assert_eq!(backward_branches(&ir), 0, "a literal needs no copy loop");
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
        &[],
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
        if line.starts_with("define ") {
            labels.clear();
        } else if let Some(label) = line.strip_suffix(':') {
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
    for (default, ml, inputs, signature, instruction) in BUILTIN_KERNELS {
        for ir in buffer_ir(default, ml, inputs)? {
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

const BUILTIN_KERNELS: [(&str, &str, FixtureInputs, &str, &str); 4] = [
    (
        "gpuMap(left, toFloat)",
        "gpuMap left toFloat",
        INT_INPUT,
        "define internal double @__gpu_kernel_0(i64 %$p0)",
        "sitofp i64",
    ),
    (
        "gpuMap(left, abs)",
        "gpuMap left abs",
        &[("left", "-1.5")],
        "define internal double @__gpu_kernel_0(double %$p0)",
        "@llvm.fabs.f64(",
    ),
    (
        "gpuZipWith(left, right, wrapAdd)",
        "gpuZipWith (left, right, wrapAdd)",
        INT_INPUTS,
        "define internal i64 @__gpu_kernel_0(i64 %$p0, i64 %$p1)",
        "add i64",
    ),
    (
        "gpuScan(left, 0, satMul)",
        "gpuScan (left, 0, satMul)",
        INT_INPUT,
        "define internal i64 @__gpu_kernel_0(i64 %$p0, i64 %$p1)",
        "mul i128",
    ),
];

/// [GPU-IOTA] An index grid is one runtime fill: no per-element store loop in the program.
#[test]
fn gpu_iota_fills_its_buffer_in_one_runtime_call() -> Result<(), String> {
    for ir in buffer_ir("gpuIota(1000)", "gpuIota 1000", &[])? {
        assert_eq!(calls(&ir, "@osprey_gpu_alloc("), 1);
        assert_eq!(calls(&ir, "@osprey_gpu_iota_fill("), 1);
        assert_eq!(calls(&ir, "@osprey_gpu_set("), 0);
        assert_eq!(
            backward_branches(&ir),
            0,
            "the fill needs no loop in the program"
        );
    }
    Ok(())
}
