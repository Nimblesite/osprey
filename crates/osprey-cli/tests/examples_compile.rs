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
    let program = assemble_if_needed(path, source, parsed.program)?;
    let type_errors = osprey_types::check_program(&program);
    if let Some(first) = type_errors.first() {
        return Err(format!("typecheck: {first:?}"));
    }
    osprey_codegen::compile_program(&program)
        .map(|ir| ir.len())
        .map_err(|e| format!("codegen: {e:?}"))
}

/// Resolve a module-bearing source through the project layer before grading it.
///
/// A namespace, module, import or signature is not a program until
/// `osprey_project` resolves its paths and flattens the graph ([MODULES-MODEL]);
/// `Tax::add` is an unknown identifier until then. Grading the raw parse would
/// report every module program as broken while the CLI runs it perfectly — so
/// this reproduces the CLI's own single-source path. Ordinary scripts skip it
/// and keep exactly the IR and symbol names they had.
fn assemble_if_needed(
    path: &Path,
    source: &str,
    program: osprey_ast::Program,
) -> Result<osprey_ast::Program, String> {
    if !osprey_project::needs_assembly(&program) {
        return Ok(program);
    }
    let source_file = osprey_project::SourceFile {
        path: path.to_path_buf(),
        flavor: osprey_syntax::resolve_flavor(None, &path.to_string_lossy(), source)
            .unwrap_or(osprey_syntax::Flavor::Default),
        source: source.to_string(),
        program,
    };
    osprey_project::assemble_one(source_file)
        .map(|assembled| assembled.program)
        .map_err(|errors| match errors.first() {
            Some(first) => format!("project: {}", first.message),
            None => "project: assembly failed with no diagnostic".to_string(),
        })
}

#[test]
fn unhandled_integer_overflow_in_mutation_is_rejected() {
    let source = r#"
effect Audit
    step : string => int

pipeline : Unit -> int ! Audit
pipeline () = perform Audit.step "tick"

unhandledOverflow () =
    mut n = 0
    handle Audit
        step label =>
            n := n + 1
            resume n
    in pipeline ()
"#;
    let result = compile(Path::new("unhandled_integer_overflow.test.ospml"), source);
    assert!(
        matches!(&result, Err(reason) if reason.starts_with("typecheck:")),
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
    raise: fn(string) -> Result<int, MathError>
}

fn addPreservingError(a, b) = match a + b {
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
    raise : string => Result<int, MathError>

addPreservingError a b = match a + b
    Success value => Success(value = value)
    Error message => perform ArithmeticFailure.raise message

print (toString (addPreservingError 9223372036854775807 1))
",
        ),
    ];

    let mut accepted = Vec::new();
    for (path, source) in cases {
        if compile(Path::new(path), source).is_ok() {
            accepted.push(path);
        }
    }

    assert!(
        accepted.is_empty(),
        "unhandled inferred effects must fail compilation in both flavors; accepted: {}",
        accepted.join(", ")
    );
}

#[test]
fn every_language_test_compiles_to_ir() {
    let dir = repo_root().join("tests");
    // BOTH flavors. An ML twin is not a translation of its Default twin — it is
    // a second front end producing the same IR ([FLAVOR-IR-EQUIV]) — and it
    // reaches AST forms the Default lowerer never builds (`Expr::MethodCall`
    // among them), so compiling only `.osp` here left the ML half of every
    // shared walker unexercised in-process.
    let mut files = sources(&dir, "osp");
    files.extend(sources(&dir, "ospml"));
    files.sort();
    assert!(
        files.len() >= 80,
        "expected the full tested corpus in both flavors, found {}",
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
fn every_benchmark_source_compiles_after_checked_arithmetic_change() {
    let dir = repo_root().join("benchmarks/cases");
    let files = sources(&dir, "osp");
    assert!(
        files.len() >= 22,
        "expected the full Osprey benchmark corpus, found {}",
        files.len()
    );

    let mut failures = Vec::new();
    for path in &files {
        let source = fs::read_to_string(path).expect("read benchmark");
        if let Err(stage) = compile(path, &source) {
            let rel = path.strip_prefix(&dir).unwrap_or(path);
            failures.push(format!("{}: {stage}", rel.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "benchmark sources must compile cleanly; failures:\n{}",
        failures.join("\n")
    );
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

#[test]
fn static_effect_safety_negative_cases_are_rejected_in_both_flavors() {
    // [EFFECTS-STATIC-DISCHARGE] These paired fixtures pin the end-to-end
    // parse -> inference -> entry-proof boundary, including ML lowering.
    let dir = repo_root().join("examples/failscompilation");
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
            "recursively re-enter",
        ),
        (
            "ml_static_effect_unhandled_inferred_transitive.ospo",
            "Alarm.ring",
        ),
        ("ml_static_effect_escaped_lambda.ospo", "Alarm.ring"),
        (
            "ml_static_effect_partial_missing_operation.ospo",
            "Pair.second",
        ),
        ("ml_static_effect_generic_mismatch.ospo", "Stash<int>.put"),
        (
            "ml_static_effect_recursive_handler.ospo",
            "recursively re-enter",
        ),
    ];

    for (name, expected) in cases {
        let path = dir.join(name);
        let source = fs::read_to_string(&path).expect("read static effect fixture");
        let Err(reason) = compile(&path, &source) else {
            panic!("{name} must be rejected by static effect discharge");
        };
        assert!(
            reason.contains(expected),
            "{name}: expected a diagnostic containing {expected:?}, got {reason:?}"
        );
    }
}

#[test]
fn unsupported_select_is_rejected_before_codegen() {
    // The parser reserves `select`, but the checker must stop it before the
    // backend's incomplete arm lowering can silently choose a value.
    // Implements [CONCURRENCY-SELECT-REJECT].
    let dir = repo_root().join("examples/failscompilation");
    let path = dir.join("select_not_supported.ospo");
    let source = fs::read_to_string(&path).expect("read ospo");
    let Err(reason) = compile(&path, &source) else {
        panic!("select_not_supported.ospo must be rejected")
    };
    assert!(
        reason.contains("`select` is not supported"),
        "unexpected rejection: {reason}"
    );
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
        "unexpected token LBrace in expression",
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
    // EVERY ill-formed program must be rejected, whatever the corpus size — the
    // count is read from the directory rather than written down here, because a
    // number in a comment goes stale the first time a case is added and then
    // describes a corpus that no longer exists.
    //
    // This assertion used to read `rejected * 2 >= files.len()`, i.e. "a healthy
    // majority", which tolerated half the corpus silently starting to compile. Its
    // comment deferred the exact residue to "the shell harness ratchet", but
    // that harness (crates/diff_examples.sh, FC_EXPECTED_ESCAPES=0) was deleted,
    // so the strict count it pointed at no longer existed. A must-reject corpus
    // that permits half its cases to be accepted is not a gate.
    let dir = repo_root().join("examples/failscompilation");
    let files = sources(&dir, "ospo");
    assert!(!files.is_empty(), "expected a must-reject corpus");
    let escaped: Vec<String> = files
        .iter()
        .filter(|p| compile(p.as_path(), &fs::read_to_string(p).unwrap_or_default()).is_ok())
        .map(|p| p.strip_prefix(&dir).unwrap_or(p).display().to_string())
        .collect();
    assert!(
        escaped.is_empty(),
        "every ill-formed program must be rejected; {} of {} were accepted:\n{}",
        escaped.len(),
        files.len(),
        escaped.join("\n")
    );
}

/// Every diagnostic the CLI would print for one rejected source, path prefix
/// stripped so the golden is location-independent. Mirrors the two shapes
/// `main.rs` emits: `{path}:{line}:{col}: {msg}` for a located error and
/// `{path}: {msg}` for one with no position.
fn rejection_diagnostics(path: &Path, source: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if !parsed.errors.is_empty() {
        for e in &parsed.errors {
            // A formatting failure into a String cannot happen; ignoring the
            // Result keeps this panic-free without an unwrap.
            let _ = writeln!(
                out,
                "{}:{}: {}",
                e.position.line, e.position.column, e.message
            );
        }
        return out;
    }
    for e in &osprey_types::check_program(&parsed.program) {
        let _ = match e.position {
            Some(p) => writeln!(out, "{}:{}: {}", p.line, p.column, e.message),
            None => writeln!(out, "{}", e.message),
        };
    }
    // A program the frontend accepts can still be rejected at lowering (the
    // CLI prints these as `{path}: {msg}` too) — e.g. a recursive function
    // whose signature never became concrete enough to emit.
    if out.is_empty() {
        if let Err(e) = osprey_codegen::compile_program(&parsed.program) {
            let _ = writeln!(out, "{e}");
        }
    }
    out
}

#[test]
fn failscompilation_corpus_matches_its_expected_diagnostics() {
    // Rejection alone is a weak gate: a case can keep failing for a reason that
    // has nothing to do with what it was written to pin. Each `.ospo` therefore
    // carries a sibling `.expectedoutput` holding the exact diagnostics, and
    // this asserts them byte-for-byte.
    //
    // These goldens previously described Go-era messages ("line 6:12: cannot
    // access field 'value' on non-struct type") that the Rust compiler stopped
    // emitting years ago, and nothing read them — 33 of 82 were stale.
    let dir = repo_root().join("examples/failscompilation");
    let mut drift = Vec::new();
    for path in sources(&dir, "ospo") {
        let name = path
            .strip_prefix(&dir)
            .unwrap_or(&path)
            .display()
            .to_string();
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
