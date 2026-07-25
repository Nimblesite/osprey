//! CI gate — examples must not wrap a trivial program in a needless `main`.
//!
//! Both flavors synthesize `main` from bare top-level statements and lower them
//! to byte-identical IR ([FLAVOR-IR-EQUIV], docs/specs/0023, 0024), so a
//! zero-argument `fn main()` (Default) or `main ()` / `main :` (ML) is pure
//! boilerplate: the program reads exactly the same written as bare top-level
//! statements. This gate fails if any tested example carries that boilerplate,
//! so the rule is enforced forever instead of by review.
//!
//! The *only* sanctioned exception is a program that genuinely needs `argv` or a
//! non-zero exit code. A `main` that takes parameters is never flagged (it is
//! consuming `argv`); a zero-argument `main` kept for its exit code must opt out
//! explicitly with a `// osprey: keep-main <reason>` marker, which both
//! documents the intent and silences the gate.

use std::path::{Path, PathBuf};

/// `examples/tested`, resolved from the crate manifest so the gate runs the same
/// on a dev box and in CI.
fn tested_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/tested");
    dir.canonicalize().unwrap_or(dir)
}

/// The opt-out marker: a zero-argument `main` kept on purpose (a meaningful
/// non-zero exit code) carries this so the gate records the intent and passes.
const KEEP_MARKER: &str = "osprey: keep-main";

/// Every `.osp`/`.ospml` under `dir`, found by a recursive walk and sorted for
/// deterministic reporting.
fn example_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, &mut out);
    out.sort();
    out
}

/// Recurse into `dir`, pushing every Osprey source file into `out`.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x == "osp" || x == "ospml")
        {
            out.push(path);
        }
    }
}

/// The text between the first `(` and its matching `)` on a `main` header line,
/// or `None` when the line is not a `main` declaration. Used to tell a
/// zero-argument `main ()` (boilerplate) from `main (argv)` (consuming argv).
fn main_param_text(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    // Default `fn main(...)` or ML `main (...)` — the binding head, not a call.
    let after = rest
        .strip_prefix("fn main")
        .or_else(|| rest.strip_prefix("main"))?
        .trim_start();
    let inner = after.strip_prefix('(')?;
    inner.split_once(')').map(|(params, _)| params)
}

/// True when `line` declares a needless zero-argument `main`: `fn main()`,
/// `main ()`, or the ML signature `main :` (which only ever types such a main).
fn declares_needless_main(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("main :") || trimmed.starts_with("main:") {
        return true;
    }
    main_param_text(line).is_some_and(|params| params.trim().is_empty())
}

#[test]
fn no_example_wraps_a_trivial_program_in_main() {
    let dir = tested_dir();
    let mut offenders: Vec<String> = Vec::new();

    for path in example_files(&dir) {
        let src = std::fs::read_to_string(&path).expect("read example source");
        if src.contains(KEEP_MARKER) {
            continue; // explicitly sanctioned (argv / non-zero exit code)
        }
        if src.lines().any(declares_needless_main) {
            let rel = path
                .strip_prefix(&dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            offenders.push(rel);
        }
    }

    assert!(
        offenders.is_empty(),
        "{} example(s) wrap a trivial program in a needless `main` — write bare \
         top-level statements instead (both flavors synthesize `main` with \
         identical IR). If a zero-arg `main` is kept for argv/exit-code, mark it \
         `// {KEEP_MARKER} <reason>`:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn detector_classifies_main_headers_correctly() {
    // Needless: zero-argument mains in both flavors and the ML signature line.
    assert!(declares_needless_main("fn main() = {"));
    assert!(declares_needless_main("fn main () ="));
    assert!(declares_needless_main("main () ="));
    assert!(declares_needless_main("main :"));
    assert!(declares_needless_main("main : Unit -> int"));
    // Allowed: a `main` that consumes argv is never boilerplate.
    assert!(!declares_needless_main("fn main(args) ="));
    assert!(!declares_needless_main("main argv ="));
    // Unrelated lines, and calls to other functions, are never flagged.
    assert!(!declares_needless_main("let mainResult = run()"));
    assert!(!declares_needless_main("print(\"main done\")"));
    assert!(!declares_needless_main("fn mainLoop() ="));
}
