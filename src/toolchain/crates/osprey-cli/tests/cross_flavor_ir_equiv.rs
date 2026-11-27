//! Cross-flavor **LLVM IR** equivalence ([FLAVOR-IR-EQUIV],
//! docs/specs/language/0023-LanguageFlavors.md). The headline guarantee of the
//! many-CSTs-one-AST design: a program written in the ML flavor (`.ospml`) and
//! its Default-flavor twin (`.osp`) lower to the SAME canonical AST, therefore
//! the codegen backend emits **byte-identical LLVM IR** for both. Currying is a
//! pure lowering, so `add x y = x + y` (ML) and `fn add(x) = fn(y) => x + y`
//! (Default) are indistinguishable past the `[FLAVOR-BOUNDARY]`.
//!
//! This is the enforcement layer the differential golden harness cannot give:
//! the harness proves the two flavors *run* the same; this proves they *compile*
//! to the same IR, which is a far stronger structural claim.
//!
//! Data-driven: every `.ospml` under the root `tests`
//! corpus MUST have an in-place Default `.osp` twin unless explicitly exempt.
//! The test compiles both through
//! `osprey_codegen::compile_program` (in-process — no built binary required) and
//! asserts the emitted IR text is identical.
//!
//! Exception: a handful of examples are an *intentionally ML-only surface* with
//! no Default-flavor equivalent — the pure three-state `Verdict` testing model
//! ([TESTING-VERDICT], docs/specs/language/0027-TestingFramework.md) is the case in point: the Default flavor
//! stays imperative (`fn() -> Unit` firing soft assertions), so a Verdict case
//! has no twin to be IR-identical to. Those stems are listed in
//! [`ML_ONLY_STEMS`] and skipped by both tests below.

#[path = "cross_flavor_ir_equiv/debug.rs"]
mod debug;

#[path = "cross_flavor_ir_equiv/gpu.rs"]
mod gpu;

#[path = "cross_flavor_ir_equiv/device.rs"]
mod device;

#[path = "cross_flavor_ir_equiv/use_binding.rs"]
mod use_binding;

use std::path::{Path, PathBuf};

use osprey_ast::Program;
use osprey_codegen::compile_program;
use osprey_project::SourceFile;
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Stems that are an intentionally ML-only surface (no Default twin, exempt from
/// IR equivalence). Keep this list tiny and each entry justified — it is a hole
/// in the headline `[FLAVOR-IR-EQUIV]` guarantee, warranted only when the ML
/// program genuinely has no Default-flavor spelling.
const ML_ONLY_STEMS: &[&str] = &[
    // Reproductions of bugs reported in ML syntax, kept as reported.
    "signal_refresh_call_argument.test",
    "signal_refresh_field_on_call.test",
    "shared_field_name_generic_records.test",
    "static_handler_callback_parameter.test",
    "closures_in_recursive_union.test",
    "mutual_recursion_generalized.test",
    "generic_recursion_through_fold.test",
    "record_update_list_literal.test",
    "reperform_reaches_outer_handler.test",
    "handler_state_from_nested_function.test",
    "four_argument_curried_effects.test",
    // The pure `Verdict` testing model is ML-only by design [TESTING-VERDICT].
    "verdict.test",
    // A program that declares its own `Verdict` [TESTING-VERDICT-DECLARED].
    "verdict_declared.test",
    // Reserved words as operation names: Default reserves them in every
    // position, ML frees them in operation positions [FLAVOR-ML-EFFECT-OP-NAME].
    "ml_name_positions.test",
];

/// Whether `path`'s stem is an allowlisted ML-only surface.
fn is_ml_only(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| ML_ONLY_STEMS.contains(&stem))
}

/// Flavor-pair roots, resolved from the crate manifest directory so the test
/// runs unchanged locally and in CI. Moved assertion suites retain the same IR
/// equivalence guarantee as golden examples.
///
/// `benchmarks` is a root because its twins were otherwise unguarded and rotted:
/// nothing type-checked a `.ospml` under it — this test walked only `tests/`, the
/// formatter corpus test only *parses*, and `src/toolchain/benchmarks/run.sh` compiles the
/// `.osp` side alone. `binarytrees.ospml` sat with four
/// `cannot unify int with Result<int, MathError>` errors under the old
/// ML elegance plan advertised it as the
/// proof that ML reaches 6 lines at IR parity with its Default twin.
fn flavor_roots() -> Vec<PathBuf> {
    // canonicalize() resolves the `../../../../`; fall back to the joined path when it
    // is unavailable rather than expect()-panicking outside a `#[test]` (the
    // workspace denies clippy::expect_used in non-test code).
    ["../../../osprey/tests", "../../benchmarks"]
        .iter()
        .map(|relative| {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            dir.canonicalize().unwrap_or(dir)
        })
        .collect()
}

/// Parse `source` under `flavor` and emit LLVM IR text, surfacing parse errors
/// loudly so a malformed example fails the test instead of silently lowering a
/// partial AST.
fn ir_for(source: &str, flavor: Flavor, label: &str) -> Result<String, String> {
    let source_file = parsed_source(source, flavor, label)?;
    let program = assembled_if_module_aware(source_file)?;
    let ir = compile_program(&program).map_err(|e| format!("{label}: codegen failed: {e:?}"))?;
    gpu::check_extraction_floor(label, &ir)?;
    Ok(ir)
}

fn parsed_source(source: &str, flavor: Flavor, label: &str) -> Result<SourceFile, String> {
    let parsed = parse_program_with_flavor(source, flavor);
    if !parsed.errors.is_empty() {
        return Err(format!(
            "{label}: unexpected {flavor} parse errors: {:?}",
            parsed.errors
        ));
    }
    Ok(SourceFile {
        path: PathBuf::from(label),
        flavor,
        source: source.to_owned(),
        program: parsed.program,
        discipline: None,
    })
}

/// A module-bearing source is not a program until the project layer resolves its
/// paths and flattens the graph — `Tax::add` and an import alias like `U::pack`
/// have no meaning before that. Lowering the raw parse would compare IR neither
/// flavor ever runs, so this reproduces the CLI's own single-source path
/// ([MODULES-MODEL]) for both flavors alike, leaving ordinary scripts untouched.
/// The program answered is the one the backend lowers, with every opaque alias
/// expanded ([MODULES-OPAQUE-TYPES]).
fn assembled_if_module_aware(source: SourceFile) -> Result<Program, String> {
    if !osprey_project::needs_assembly(&source.program) {
        return Ok(source.program);
    }
    osprey_project::assemble_one(source)
        .map(|assembled| assembled.backend_program().clone())
        .map_err(|errors| format!("project assembly failed: {errors:?}"))
}

/// Every `.ospml` file anywhere under `dir`, found by a recursive walk and
/// sorted for deterministic output. Twins live in place next to their `.osp`
/// counterparts throughout `tests`, not in one folder.
fn ml_stems(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_ospml(dir, &mut out);
    out.sort();
    out
}

/// Recurse into `dir`, pushing every `.ospml` path into `out`. A symlinked
/// directory (`src/toolchain/benchmarks/proof/<case>/kernel` pointing at an example
/// project) is not entered: its files are governed by the root they live in.
fn collect_ospml(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let linked = entry.file_type().is_ok_and(|kind| kind.is_symlink());
        if path.is_dir() && !linked {
            collect_ospml(&path, out);
        } else if path.extension().is_some_and(|x| x == "ospml") {
            out.push(path);
        }
    }
}

/// Every ML example must have a Default twin (same stem, `.osp`). A missing twin
/// is a hard failure — the pairing is the whole point of the flavor system.
#[test]
fn every_ml_example_has_a_default_twin() {
    let missing: Vec<String> = flavor_roots()
        .iter()
        .flat_map(|dir| ml_stems(dir))
        .filter(|p| !is_ml_only(p) && !p.with_extension("osp").exists())
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string()
        })
        .collect();
    assert!(
        missing.is_empty(),
        "every <stem>.ospml needs a Default twin <stem>.osp for IR equivalence; \
         missing twins for: {missing:?}"
    );
}

/// The headline guarantee: ML and Default twins emit byte-identical IR. Collects
/// every mismatch before failing so one run reports all drift, not just the first.
#[test]
fn ml_and_default_twins_emit_identical_ir() {
    let mut mismatches: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for ml_path in flavor_roots().iter().flat_map(|dir| ml_stems(dir)) {
        let def_path = ml_path.with_extension("osp");
        if !def_path.exists() {
            continue; // covered by `every_ml_example_has_a_default_twin`
        }
        let stem = ml_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();

        let ml_src = std::fs::read_to_string(&ml_path).expect("read .ospml");
        let def_src = std::fs::read_to_string(&def_path).expect("read .osp");

        let ml_ir = ir_for(&ml_src, Flavor::Ml, &format!("{stem}.ospml")).expect("ml codegen");
        let def_ir =
            ir_for(&def_src, Flavor::Default, &format!("{stem}.osp")).expect("default codegen");

        checked += 1;
        if ml_ir != def_ir {
            mismatches.push(format!(
                "  {stem}: IR differs\n{}",
                first_diff(&def_ir, &ml_ir)
            ));
        }
    }

    assert!(checked > 0, "no flavor pairs found under tests");
    assert!(
        mismatches.is_empty(),
        "ML and Default twins MUST emit identical LLVM IR; {} pair(s) drifted:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// First differing line between two IR texts, with a little context, so a
/// failure points at the exact divergence instead of dumping two full modules.
fn first_diff(default_ir: &str, ml_ir: &str) -> String {
    for (i, (d, m)) in default_ir.lines().zip(ml_ir.lines()).enumerate() {
        if d != m {
            return format!(
                "    line {}:\n      default: {d}\n      ml:      {m}",
                i + 1
            );
        }
    }
    format!(
        "    one IR is a prefix of the other (default {} lines, ml {} lines)",
        default_ir.lines().count(),
        ml_ir.lines().count()
    )
}

#[path = "cross_flavor_ir_equiv/project.rs"]
mod project;

/// [FLAVOR-PYTHON-BOUNDARY] Check source-independent ASTs and the full LLVM module.
fn flat_triple(label: &str, sources: [&str; 3]) -> Result<String, String> {
    let mut expected = None;
    for (source, flavor) in sources
        .into_iter()
        .zip([Flavor::Default, Flavor::Ml, Flavor::Python])
    {
        let program = parsed_source(source, flavor, label)?.program;
        let canonical = osprey_ast::canonical::without_positions(&program);
        let errors = osprey_types::check_program(&program);
        assert!(errors.is_empty(), "{label} {flavor}: {errors:?}");
        let ir = compile_program(&program).map_err(|e| format!("{label} {flavor}: {e:?}"))?;
        if let Some((ast, module)) = &expected {
            assert_eq!(ast, &canonical, "{label} {flavor} AST");
            assert_eq!(module, &ir, "{label} {flavor} LLVM");
        } else {
            expected = Some((canonical, ir));
        }
    }
    expected
        .map(|(_, ir)| ir)
        .ok_or_else(|| format!("{label}: missing input"))
}

const RESULT_RECORD_ABI: [&str; 3] = [
    "type Packet = { number: Result<float, string>, flag: Result<bool, string> }\nfn make(n, b) = Packet { number: Success { value: n }, flag: Success { value: b } }\nlet packet = make(2.5, true)\nprint(\"${packet.number},${packet.flag}\")",
    "type Packet =\n    number: Result<float, string>\n    flag: Result<bool, string>\nmake (n, b) = Packet(number = Success(value = n), flag = Success(value = b))\npacket = make (2.5, true)\nprint \"${packet.number},${packet.flag}\"",
    "class Packet:\n    number: Result[float, str]\n    flag: Result[bool, str]\ndef make(n, b):\n    return Packet(number=Success(value=n), flag=Success(value=b))\npacket = make(2.5, True)\nprint(f\"{packet.number},{packet.flag}\")",
];

/// [TYPE-RECORD-RESULT] Payload bits must not move the tag or error-message slots.
#[test]
fn result_record_fields_use_complete_pointer_slots_and_bit_preserving_payloads(
) -> Result<(), String> {
    let ir = flat_triple("result-record", RESULT_RECORD_ABI)?;
    assert_result_record_storage(&ir);
    Ok(())
}

fn assert_result_record_storage(ir: &str) {
    for required in [
        "getelementptr { i64, i8*, i8* }", // both fields point to whole Results
        "getelementptr { i64, i8, i8* }",  // canonical Result block
        "bitcast double",
        "to i64",
        "bitcast i64",
        "to double",
        "zext i1",
        "trunc i64",
        "to i1", // booleans use payload bits too
    ] {
        assert!(ir.contains(required), "missing {required}: {ir}");
    }
    for forbidden in ["{ double, i8, i8* }", "{ i1, i8, i8* }", "{ i8*, i8, i8* }"] {
        assert!(
            !ir.contains(forbidden),
            "payload-dependent Result offsets: {forbidden}"
        );
    }
}
