//! Cross-flavor **LLVM IR** equivalence ([FLAVOR-IR-EQUIV],
//! docs/specs/0023-LanguageFlavors.md). The headline guarantee of the
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
//! Data-driven: every `examples/tested/ml/<stem>.ospml` MUST have a sibling
//! Default twin `examples/tested/ml/<stem>.osp`. The test compiles both through
//! `osprey_codegen::compile_program` (in-process — no built binary required) and
//! asserts the emitted IR text is identical.
//!
//! Exception: a handful of examples are an *intentionally ML-only surface* with
//! no Default-flavor equivalent — the pure three-state `Verdict` testing model
//! ([TESTING-VERDICT], docs/specs/0027) is the case in point: the Default flavor
//! stays imperative (`fn() -> Unit` firing soft assertions), so a Verdict case
//! has no twin to be IR-identical to. Those stems are listed in
//! [`ML_ONLY_STEMS`] and skipped by both tests below.

use std::path::{Path, PathBuf};

use osprey_codegen::compile_program;
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Stems that are an intentionally ML-only surface (no Default twin, exempt from
/// IR equivalence). Keep this list tiny and each entry justified — it is a hole
/// in the headline `[FLAVOR-IR-EQUIV]` guarantee, warranted only when the ML
/// program genuinely has no Default-flavor spelling.
const ML_ONLY_STEMS: &[&str] = &[
    // The pure `Verdict` testing model is ML-only by design [TESTING-VERDICT].
    "verdict.test",
];

/// Whether `path`'s stem is an allowlisted ML-only surface.
fn is_ml_only(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| ML_ONLY_STEMS.contains(&stem))
}

/// `examples/tested`, resolved from the crate manifest dir so the test runs
/// unchanged on a dev box and in CI. Every `.ospml` ANYWHERE under this tree is
/// an in-place twin of its sibling `.osp` and must emit byte-identical IR.
fn ml_examples_dir() -> PathBuf {
    // canonicalize() resolves the `../../`; fall back to the joined path when it
    // is unavailable rather than expect()-panicking outside a `#[test]` (the
    // workspace denies clippy::expect_used in non-test code).
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/tested");
    dir.canonicalize().unwrap_or(dir)
}

/// Parse `source` under `flavor` and emit LLVM IR text, surfacing parse errors
/// loudly so a malformed example fails the test instead of silently lowering a
/// partial AST.
fn ir_for(source: &str, flavor: Flavor, label: &str) -> Result<String, String> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{label}: unexpected {flavor} parse errors: {:?}",
        parsed.errors
    );
    compile_program(&parsed.program).map_err(|e| format!("{label}: codegen failed: {e:?}"))
}

/// Every `.ospml` file anywhere under `dir`, found by a recursive walk and
/// sorted for deterministic output. Twins live in place next to their `.osp`
/// counterparts throughout `examples/tested`, not in one folder.
fn ml_stems(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_ospml(dir, &mut out);
    out.sort();
    out
}

/// Recurse into `dir`, pushing every `.ospml` path into `out`.
fn collect_ospml(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
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
    let dir = ml_examples_dir();
    let missing: Vec<String> = ml_stems(&dir)
        .into_iter()
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
    let dir = ml_examples_dir();
    let mut mismatches: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for ml_path in ml_stems(&dir) {
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

    assert!(
        checked > 0,
        "no flavor pairs found under examples/tested/ml — expected at least one"
    );
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
