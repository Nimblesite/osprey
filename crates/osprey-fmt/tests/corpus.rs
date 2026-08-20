//! Whole-corpus formatter invariants.
//!
//! Every hand-written example, benchmark, and language test — both `.osp`
//! (Default) and `.ospml` (ML) — is run through the formatter and held to the
//! two guarantees the formatter promises: formatting is **idempotent** (a
//! second pass changes nothing) and **meaning-preserving** (the formatted text
//! reparses to the very same AST). Every corpus file must parse: silently
//! skipping one would leave a formatter regression outside this audit.
//!
//! Both halves are real only because a failed meaning-preservation guard is an
//! error (`osprey_fmt::DECLINED`) rather than the input returned verbatim —
//! until plan 0019 closed that, "declined to format" and "already formatted"
//! were the same observation here.

use std::fs;
use std::path::{Path, PathBuf};

use osprey_fmt::format_for_path;

fn repo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Plan 0019 measured the hand-written corpus under `examples` and
/// `benchmarks`; `tests` extends the same audit to the current language corpus.
fn corpus_roots() -> [PathBuf; 3] {
    let repo = repo_dir();
    [
        repo.join("examples"),
        repo.join("benchmarks"),
        repo.join("tests"),
    ]
}

/// Every hand-written file with extension `ext`, sorted for stable failures.
fn sources(ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in corpus_roots() {
        collect(&root, ext, &mut out);
    }
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
            if path.file_name().is_none_or(|name| name != "build") {
                collect(&path, ext, out);
            }
        } else if path.extension().is_some_and(|e| e == ext) && !is_generated(&path) {
            out.push(path);
        }
    }
}

/// The checked-in web bundle is generated JavaScript embedded in an Osprey
/// string, explicitly excluded from Plan 0019's hand-written corpus baseline.
fn is_generated(path: &Path) -> bool {
    path.ends_with("examples/projects/modules/src/web/bundle.ospml")
}

/// Format every corpus source of one extension, collecting **every** file that
/// fails rather than stopping at the first: one run reports the whole blast
/// radius of a formatter regression. Returns `(processed, failures)`.
fn check_extension(ext: &str) -> (usize, Vec<String>) {
    let mut processed = 0;
    let mut failures = Vec::new();
    for path in sources(ext) {
        let display = path.display();
        let Ok(src) = fs::read_to_string(&path) else {
            failures.push(format!("{display}: unreadable"));
            continue;
        };
        processed += 1;
        let key = path.to_string_lossy();
        match format_for_path(&key, &src) {
            Err(errors) => failures.push(format!("{display}: format: {errors:?}")),
            Ok(once) => match format_for_path(&key, &once) {
                Err(errors) => failures.push(format!("{display}: re-format: {errors:?}")),
                Ok(twice) if twice != once => {
                    failures.push(format!("{display}: not idempotent"));
                }
                Ok(_) => {}
            },
        }
    }
    (processed, failures)
}

#[test]
fn default_corpus_formats_idempotently() {
    let (processed, failures) = check_extension("osp");
    assert!(processed > 0, "no .osp sources were processed");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ml_corpus_formats_idempotently() {
    let paths = sources("ospml");
    for required in [
        "benchmarks/cases/binarytrees/binarytrees.ospml",
        "examples/projects/modules/src/main.ospml",
        "examples/wasm/studio.ospml",
        "tests/core/collections/list_basics.test.ospml",
        "tests/core/feature_composition/feature_omnibus.test.ospml",
    ] {
        assert!(
            paths.iter().any(|path| path.ends_with(required)),
            "ML formatter corpus omitted {required}"
        );
    }
    let (processed, failures) = check_extension("ospml");
    assert!(processed > 0, "no .ospml sources were processed");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ml_clause_and_match_surface_forms_are_preserved() {
    let src = concat!(
        "type Choice = None | Some int\n",
        "value (Some n) = n\n",
        "value None = 0\n",
        "build n = Some (n + 1)\n",
        "ignore _ = 0\n",
        "recover n = n % 2 ?: -1\n",
        "explicit x = match x\n",
        "  Some n => n\n",
        "  None => 0\n",
    );
    let out = format_for_path("forms.ospml", src).unwrap_or_default();
    assert!(!out.is_empty(), "forms.ospml did not format");
    assert_ne!(out, src, "the irregular layout must be reformatted");
    assert!(
        out.contains("type Choice = None | Some int"),
        "union changed: {out}"
    );
    assert!(out.contains("value (Some n) = n"), "clause changed: {out}");
    assert!(
        out.contains("build n = Some (n + 1)"),
        "constructor changed: {out}"
    );
    assert!(out.contains("ignore _ = 0"), "wildcard changed: {out}");
    assert!(
        out.contains("recover n = n % 2 ?: -1"),
        "fallback changed: {out}"
    );
    assert!(out.contains("explicit x = match x"), "match changed: {out}");
    assert!(
        out.contains("\n    Some n => n\n"),
        "match layout changed: {out}"
    );
    let twice = format_for_path("forms.ospml", &out).unwrap_or_default();
    assert_eq!(out, twice, "Plan 0019 forms are not idempotent");
}
