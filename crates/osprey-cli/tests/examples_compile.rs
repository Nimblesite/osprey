//! In-process differential coverage harness.
//!
//! Compiling every tested example through parse → type-check → codegen drives
//! the whole compiler frontend under `cargo llvm-cov`, so the example programs
//! count toward coverage — and each one is asserted to still type-check and
//! lower to LLVM IR. The must-reject corpus (`failscompilation/*.ospo`) is run
//! through the same pipeline to cover the rejection branches. This is the
//! library-boundary twin of `crates/diff_examples.sh` (which runs the built
//! binary out-of-process and therefore never reaches the coverage profile).

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // `crates/osprey-cli` -> repo root. Left un-canonicalized (no fallible call):
    // the `..` segments resolve fine for `read_dir`, and `strip_prefix` below
    // uses this same prefix.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every file with extension `ext` under `dir`, recursively, sorted for stable
/// failure output.
fn sources(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, ext, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Compile one source the way `osprey --run` does: resolve the flavor from the
/// path and any `// osprey: flavor=…` marker, parse, gate on type errors, then
/// lower to IR. `Ok(ir_len)` on success, else the failing stage + reason.
///
/// The flavor must come from the path, not a hardwired `Flavor::Default`:
/// `failscompilation/ml_*.ospo` are ML-flavor negatives selected by a leading
/// marker, and grading them with the brace grammar would pin the wrong
/// rejection path. Implements [FLAVOR-SELECT] (docs/specs/0023).
fn compile(path: &Path, source: &str) -> Result<usize, String> {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if let Some(first) = parsed.errors.first() {
        return Err(format!("parse: {}", first.message));
    }
    let type_errors = osprey_types::check_program(&parsed.program);
    if let Some(first) = type_errors.first() {
        return Err(format!("typecheck: {first:?}"));
    }
    osprey_codegen::compile_program(&parsed.program)
        .map(|ir| ir.len())
        .map_err(|e| format!("codegen: {e:?}"))
}

#[test]
fn every_tested_example_compiles_to_ir() {
    let dir = repo_root().join("examples/tested");
    let files = sources(&dir, "osp");
    assert!(
        files.len() >= 40,
        "expected the full tested corpus, found {}",
        files.len()
    );
    let mut failures = Vec::new();
    let mut total_ir = 0usize;
    for path in &files {
        let source = fs::read_to_string(path).expect("read example");
        match compile(path, &source) {
            Ok(ir_len) => {
                let rel = path.strip_prefix(&dir).unwrap_or(path);
                assert!(ir_len > 0, "{}: produced empty IR", rel.display());
                total_ir += ir_len;
            }
            Err(stage) => {
                let rel = path.strip_prefix(&dir).unwrap_or(path);
                failures.push(format!("{}: {stage}", rel.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "tested examples must compile cleanly; failures:\n{}",
        failures.join("\n")
    );
    assert!(total_ir > 0, "the corpus lowered to non-empty IR");
}

#[test]
fn list_pattern_negative_cases_are_rejected() {
    let dir = repo_root().join("examples/failscompilation");
    for name in [
        "list_pattern_middle_rest.ospo",
        "list_pattern_double_rest.ospo",
    ] {
        let path = dir.join(name);
        let source = fs::read_to_string(&path).expect("read ospo");
        assert!(
            compile(&path, &source).is_err(),
            "{name} must be rejected (rest binder only at the tail; one rest max)"
        );
    }
}

#[test]
fn generics_and_variance_negative_cases_are_rejected() {
    // Implements [TYPE-VARIANCE-POSITIONS], [TYPE-VARIANCE-ASSIGN],
    // [EFFECTS-GENERIC-ROWS].
    let dir = repo_root().join("examples/failscompilation");
    for name in [
        "variance_out_in_input_position.ospo",
        "variance_in_in_output_position.ospo",
        "variance_on_fn_type_param.ospo",
        "variance_invariant_arg_mismatch.ospo",
        "variance_covariant_result_payload.ospo",
        "generic_effect_arg_mismatch.ospo",
        "generic_effect_variance_position.ospo",
    ] {
        let path = dir.join(name);
        let source = fs::read_to_string(&path).expect("read ospo");
        assert!(
            compile(&path, &source).is_err(),
            "{name} must be rejected (variance/generic-effect misuse)"
        );
    }
}

/// The ML-flavor must-reject fixtures, each paired with the ML-specific fragment
/// of the diagnostic it pins. Naming the fragment is what makes the assertion
/// meaningful: a fixture that started failing for a *Default*-grammar reason
/// would still be "rejected", and only the message catches that.
/// Implements [FLAVOR-ML-HANDLER], [FLAVOR-ML-LAYOUT], [FLAVOR-ML-COMMENTS],
/// [FLAVOR-ML-MATCH], [FLAVOR-BOUNDARY].
const ML_NEGATIVES: [(&str, &str); 5] = [
    (
        "ml_handler_value_not_supported.ospo",
        "ML construct 'handler' is not yet supported",
    ),
    (
        "ml_layout_inconsistent_indent.ospo",
        "inconsistent indentation does not match any enclosing block",
    ),
    (
        "ml_unterminated_doc_comment.ospo",
        "unterminated `(** … *)` doc comment",
    ),
    ("ml_match_arm_thin_arrow.ospo", "expected '=>' in match arm"),
    (
        "ml_brace_record_and_question_sigil.ospo",
        "unexpected character '{'",
    ),
];

#[test]
fn ml_flavor_negative_cases_are_rejected_by_the_ml_frontend() {
    // ML negatives are `.ospo` (the must-reject extension, which no source
    // harness compiles) plus a leading `// osprey: flavor=ml` marker — the
    // marker alone selects the ML frontend, since `.ospo` implies no flavor.
    let dir = repo_root().join("examples/failscompilation");
    for (name, expected) in ML_NEGATIVES {
        let path = dir.join(name);
        let source = fs::read_to_string(&path).expect("read ospo");
        let Err(reason) = compile(&path, &source) else {
            panic!("{name} must be rejected by the ML frontend");
        };
        assert!(
            reason.contains(expected),
            "{name}: expected an ML diagnostic containing {expected:?}, got {reason:?}"
        );
    }
}

#[test]
fn failscompilation_corpus_drives_rejection_paths() {
    // Every `.ospo` is run through the pipeline to cover the rejection branches.
    // The compiler does not yet reject all of them (the shell harness tracks the
    // residue via a ratchet), so this asserts only that a healthy majority are
    // already rejected and that the pipeline never panics on ill-formed input.
    let dir = repo_root().join("examples/failscompilation");
    let files = sources(&dir, "ospo");
    assert!(!files.is_empty(), "expected a must-reject corpus");
    let rejected = files
        .iter()
        .filter(|p| compile(p.as_path(), &fs::read_to_string(p).unwrap_or_default()).is_err())
        .count();
    assert!(
        rejected * 2 >= files.len(),
        "most ill-formed programs should be rejected, got {rejected}/{}",
        files.len()
    );
}
