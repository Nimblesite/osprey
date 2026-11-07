//! In-process differential coverage harness.
//!
//! Compiling every tested example through parse → type-check → codegen drives
//! the whole compiler frontend under `cargo llvm-cov`, so the example programs
//! count toward coverage — and each one is asserted to still type-check and
//! lower to LLVM IR. The must-reject corpus (`failscompilation/*.ospo`) is run
//! through the same pipeline to cover the rejection branches. This is the
//! in-process coverage counterpart to the built CLI's `osprey test tests`
//! assertion run, which executes out-of-process and cannot reach this profile.

use std::fs;
use std::path::{Path, PathBuf};

// Only the corpus walk is used here; the IR-symbol helpers `common` also
// carries serve the wellformedness gates, not this one.
#[expect(
    dead_code,
    reason = "the corpus walk is shared; the IR-symbol helpers are not used here"
)]
mod common;
#[path = "examples_compile/support.rs"]
mod support;
use support::{compile, compile_project, rejection_diagnostics, repo_root, sources};

/// The must-reject corpus, `src/osprey/examples/failscompilation`.
fn failscompilation_dir() -> PathBuf {
    repo_root().join("src/osprey/examples/failscompilation")
}

/// Assert the must-reject fixture `name` is rejected; an empty `expected`
/// fragment means rejection alone is pinned.
fn reject(name: &str, expected: &str, why: &str) {
    let path = failscompilation_dir().join(name);
    let source = fs::read_to_string(&path).expect("read ospo");
    let Err(reason) = compile(&path, &source) else {
        panic!("{name} must be rejected ({why})");
    };
    assert!(
        reason.contains(expected),
        "{name}: expected a diagnostic containing {expected:?}, got {reason:?}"
    );
}

/// `path` relative to `dir`, as a display string.
fn rel(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir).unwrap_or(path).display().to_string()
}

/// Compile every `ext` source under `dir`, asserting the corpus is complete and
/// clean; returns the IR bytes it produced.
fn check_corpus(
    dir: &Path,
    exts: &[&str],
    at_least: usize,
    what: &str,
    one: impl Fn(&Path) -> Result<usize, String>,
) -> usize {
    let mut files: Vec<PathBuf> = exts.iter().flat_map(|ext| sources(dir, ext)).collect();
    files.sort();
    assert!(
        files.len() >= at_least,
        "expected the full {what}, found {}",
        files.len()
    );
    let mut total_ir = 0;
    let mut failures = Vec::new();
    for path in &files {
        match one(path) {
            Ok(ir_len) => total_ir += ir_len,
            Err(stage) => failures.push(format!("{}: {stage}", rel(dir, path))),
        }
    }
    assert!(
        failures.is_empty(),
        "{what} must compile cleanly; failures:\n{}",
        failures.join("\n")
    );
    total_ir
}

#[test]
fn unhandled_integer_overflow_in_mutation_is_rejected() {
    let source = r#"
effect Audit
    control step : string => int

pipeline : Unit -> int ! Audit
pipeline () = perform Audit.step "tick"

unhandledOverflow () =
    mut n = 0
    handle Audit
        step label =>
            n := n + 1
            resume n
    pipeline ()
unhandledOverflow ()
"#;
    let result = compile(Path::new("unhandled_integer_overflow.test.ospml"), source);
    assert!(
        matches!(&result, Err(reason) if reason.contains("Arith.overflow")),
        "potentially overflowing `n := n + 1` must be rejected at type checking unless its failure is handled; got {result:?}"
    );
}

#[test]
fn inferred_effects_without_handlers_are_rejected_in_both_flavors() {
    // [EFFECTS-STATIC-DISCHARGE] Omitting a function signature must infer both
    // its value type and its effect row. Neither flavor may turn an omitted
    // `!ArithmeticFailure` annotation into a runtime-only safety check.
    let cases = [
        (
            "unhandled_inferred_effect.test.osp",
            r"
effect ArithmeticFailure {
    raise: fn(string) -> Result<int, Error>
}

fn addPreservingError(a, b) = match checkedAdd(a, b) {
    Success { value } => Success { value: value }
    Error { message } => perform ArithmeticFailure.raise(message)
}

print(toString(addPreservingError(9223372036854775807, 1)))
",
        ),
        (
            "unhandled_inferred_effect.test.ospml",
            r"
effect ArithmeticFailure
    raise : string => Result<int, Error>

addPreservingError a b = match checkedAdd a b
    Success value => Success(value = value)
    Error message => perform ArithmeticFailure.raise message

print (toString (addPreservingError 9223372036854775807 1))
",
        ),
    ];

    let accepted: Vec<_> = cases
        .iter()
        .filter(|(path, source)| compile(Path::new(path), source).is_ok())
        .map(|(path, _)| *path)
        .collect();

    assert!(
        accepted.is_empty(),
        "unhandled inferred effects must fail compilation in both flavors; accepted: {}",
        accepted.join(", ")
    );
}

#[test]
fn every_language_test_and_benchmark_source_compiles_to_ir() {
    let dir = repo_root().join("src/osprey/tests");
    // BOTH flavors. An ML twin is not a translation of its Default twin — it is
    // a second front end producing the same IR ([FLAVOR-IR-EQUIV]) — and it
    // reaches AST forms the Default lowerer never builds (`Expr::MethodCall`
    // among them), so compiling only `.osp` here left the ML half of every
    // shared walker unexercised in-process.
    let total_ir = check_corpus(
        &dir,
        &["osp", "ospml"],
        80,
        "tested corpus in both flavors",
        |path| {
            let ir_len = compile(path, &fs::read_to_string(path).expect("read example"))?;
            assert!(ir_len > 0, "{}: produced empty IR", rel(&dir, path));
            Ok(ir_len)
        },
    );
    assert!(total_ir > 0, "the corpus lowered to non-empty IR");

    // A case with its own osprey.toml shares the benchmark module, so it
    // compiles as the project `run.sh` builds; a lone file compiles alone.
    let dir = repo_root().join("src/toolchain/benchmarks/cases");
    let _ = check_corpus(
        &dir,
        &["osp"],
        22,
        "Osprey benchmark corpus",
        |path| match path.parent().filter(|d| d.join("osprey.toml").is_file()) {
            Some(root) => compile_project(root),
            None => compile(path, &fs::read_to_string(path).expect("read benchmark")),
        },
    );
}

#[test]
fn list_pattern_generics_and_variance_negative_cases_are_rejected() {
    // Implements [TYPE-VARIANCE-POSITIONS], [TYPE-VARIANCE-ASSIGN],
    // [EFFECTS-GENERIC-ROWS]; a rest binder belongs only at the tail, once.
    for name in [
        "list_pattern_middle_rest.ospo",
        "list_pattern_double_rest.ospo",
        "variance_out_in_input_position.ospo",
        "variance_in_in_output_position.ospo",
        "variance_on_fn_type_param.ospo",
        "variance_invariant_arg_mismatch.ospo",
        "variance_covariant_result_payload.ospo",
        "generic_effect_arg_mismatch.ospo",
        "generic_effect_variance_position.ospo",
    ] {
        reject(name, "", "list pattern or variance/row misuse");
    }
}

#[test]
fn static_effect_safety_negative_cases_are_rejected_in_both_flavors() {
    // [EFFECTS-STATIC-DISCHARGE] These paired fixtures pin the end-to-end
    // parse -> inference -> entry-proof boundary, including ML lowering. An
    // arm performing its own operation forwards OUTWARD, so with no outer
    // handler the recursive fixtures surface as unhandled at program entry.
    // Every fixture has an `ml_` twin pinning the same diagnostic.
    let cases = [
        (
            "static_effect_unhandled_inferred_transitive.ospo",
            "Alarm.ring",
        ),
        ("static_effect_escaped_lambda.ospo", "Alarm.ring"),
        (
            "static_effect_partial_missing_operation.ospo",
            "Pair.second",
        ),
        ("static_effect_generic_mismatch.ospo", "Stash<int>.put"),
        (
            "static_effect_recursive_handler.ospo",
            "unhandled effect operations at program entry: Loop.again",
        ),
    ];

    for (name, expected) in cases {
        reject(name, expected, "static effect discharge");
        reject(&format!("ml_{name}"), expected, "static effect discharge");
    }
}

/// The ML-flavor must-reject fixtures, each paired with the ML-specific fragment
/// of the diagnostic it pins: a fixture failing for a *Default*-grammar reason
/// would still be "rejected", and only the message catches that.
/// Implements [FLAVOR-ML-HANDLER], [FLAVOR-ML-LAYOUT], [FLAVOR-ML-COMMENTS],
/// [FLAVOR-ML-MATCH], [FLAVOR-BOUNDARY].
const ML_NEGATIVES: [(&str, &str); 4] = [
    (
        "ml_layout_inconsistent_indent.ospo",
        "inconsistent indentation does not match any enclosing block",
    ),
    (
        "ml_unterminated_doc_comment.ospo",
        "unterminated `(** … *)` doc comment",
    ),
    ("ml_match_arm_thin_arrow.ospo", "expected '=>' in match arm"),
    ("ml_brace_record_and_question_sigil.ospo", "expected '='"),
];

#[test]
fn ml_flavor_negative_cases_are_rejected_by_the_ml_frontend() {
    // ML negatives are `.ospo` (the must-reject extension, which no source
    // harness compiles) plus a leading `// osprey: flavor=ml` marker — the
    // marker alone selects the ML frontend, since `.ospo` implies no flavor.
    for (name, expected) in ML_NEGATIVES {
        reject(name, expected, "the ML frontend");
    }
}

#[test]
fn failscompilation_corpus_drives_rejection_paths() {
    // Every `.ospo` runs through the pipeline to cover the rejection branches,
    // and EVERY ill-formed program must be rejected whatever the corpus size:
    // the count is read from the directory rather than written down, because a
    // number in a comment goes stale the first time a case is added.
    //
    // This used to read `rejected * 2 >= files.len()` — "a healthy majority" —
    // which tolerated half the corpus silently compiling, and deferred the
    // residue to a shell harness (crates/diff_examples.sh) since deleted. A
    // must-reject corpus that permits half its cases is not a gate.
    let dir = failscompilation_dir();
    let files = sources(&dir, "ospo");
    assert!(!files.is_empty(), "expected a must-reject corpus");
    let escaped: Vec<String> = files
        .iter()
        .filter(|p| compile(p.as_path(), &fs::read_to_string(p).unwrap_or_default()).is_ok())
        .map(|p| rel(&dir, p))
        .collect();
    assert!(
        escaped.is_empty(),
        "every ill-formed program must be rejected; {} of {} were accepted:\n{}",
        escaped.len(),
        files.len(),
        escaped.join("\n")
    );
}

#[test]
fn failscompilation_corpus_matches_its_expected_diagnostics() {
    // Rejection alone is a weak gate: a case can keep failing for a reason that
    // has nothing to do with what it was written to pin. Each `.ospo` therefore
    // carries a sibling `.expectedoutput` holding the exact diagnostics, and
    // this asserts them byte-for-byte. The old ones described Go-era messages
    // ("line 6:12: cannot access field 'value' on non-struct type"), nothing
    // read them, and 33 of 82 were stale.
    let dir = failscompilation_dir();
    let mut drift = Vec::new();
    for path in sources(&dir, "ospo") {
        let name = rel(&dir, &path);
        let source = fs::read_to_string(&path).unwrap_or_default();
        let golden_path = path.with_extension("ospo.expectedoutput");
        let Ok(expected) = fs::read_to_string(&golden_path) else {
            drift.push(format!("{name}: no .expectedoutput golden"));
            continue;
        };
        let actual = rejection_diagnostics(&path, &source);
        if actual != expected {
            drift.push(format!(
                "{name}:\n  expected: {expected:?}\n  actual:   {actual:?}"
            ));
        }
    }
    assert!(
        drift.is_empty(),
        "{} rejected program(s) drifted from their expected diagnostics:\n{}",
        drift.len(),
        drift.join("\n")
    );
}

#[test]
fn wasm_hello_example_has_no_unused_bindings() {
    let path = repo_root().join("src/osprey/examples/wasm/hello.osp");
    let source = fs::read_to_string(&path).expect("hello example");
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), &source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(
        osprey_types::check_program(&parsed.program),
        Vec::<osprey_types::TypeError>::new()
    );
    let warnings = osprey_types::unused_symbols(&parsed.program);
    assert!(warnings.is_empty(), "{warnings:?}");
}
