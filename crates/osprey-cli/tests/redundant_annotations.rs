//! Enforcement gate for the "redundant annotations are defects" rule.
//!
//! Osprey is Hindley-Milner, so every type the compiler can infer must be left
//! off the source. That rule lived only in prose, and nothing in the tree
//! checked it: `any_type_comprehensive` carried EIGHT removable `-> string`
//! annotations through every gate on the way to green. A style rule no gate
//! asserts is a suggestion.
//!
//! An annotation is redundant when deleting it leaves a program that still
//! type-checks with the same meaning. `-> any` is excluded by construction: it
//! is the ERASURE, not a description of an inferred type — dropping it from
//! `fn getDynamicValue() -> any = 42` still compiles but infers `int`, which is
//! a different program. Effectful returns are excluded for the same reason:
//! the row is not always recoverable from the body alone.

use std::fs;
use std::path::{Path, PathBuf};

/// Return annotations that carry meaning beyond the type the body infers.
const LOAD_BEARING: [&str; 2] = ["any", "Unit"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

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

/// Parse + type-check only. Codegen is irrelevant to whether an annotation was
/// needed, and skipping it keeps a per-annotation sweep affordable.
fn type_checks(path: &Path, source: &str) -> bool {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    parsed.errors.is_empty() && osprey_types::check_program(&parsed.program).is_empty()
}

/// The concrete return annotation on a `fn` line, as (byte range, spelling).
///
/// `rfind` rather than `find`: a higher-order parameter carries its own arrow
/// (`fn apply(f: fn(int) -> int) -> string`), and the LAST arrow before the `=`
/// is the one that belongs to the declaration.
fn return_annotation(line: &str) -> Option<(usize, usize, String)> {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("fn ") || trimmed.starts_with("export fn ")) {
        return None;
    }
    let body = line
        .find(" = ")
        .or_else(|| line.find(" =").filter(|i| line[*i..].trim_end() == "="))?;
    let arrow = line[..body].rfind(" -> ")?;
    let spelling = line[arrow + 4..body].trim().to_string();
    // An effect row rides on the return type; the body alone may not pin it.
    if spelling.is_empty() || spelling.contains('!') || LOAD_BEARING.contains(&spelling.as_str()) {
        return None;
    }
    Some((arrow, body, spelling))
}

/// Every `(line number, spelling)` whose deletion still type-checks.
fn redundant_in(path: &Path, source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (arrow, body, spelling) = return_annotation(line)?;
            let stripped = format!("{}{}", &line[..arrow], &line[body..]);
            let variant: Vec<&str> = source.lines().collect();
            let rebuilt = rebuild(&variant, index, &stripped);
            type_checks(path, &rebuilt).then(|| (index + 1, spelling))
        })
        .collect()
}

fn rebuild(lines: &[&str], index: usize, replacement: &str) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| if i == index { replacement } else { line })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What `--symbols` reports for `source`, via the exact path the CLI uses.
fn outline(source: &str) -> String {
    let parsed = osprey_syntax::parse_program(source);
    osprey_lsp::symbols_json(&parsed.program)
}

#[test]
fn deleting_an_inferable_annotation_does_not_change_what_symbols_reports() {
    // ENFORCEMENT for the `-> Unit` defect. CLAUDE.md REQUIRES deleting an
    // inferable annotation, and `symbols_json` renders the DECLARED return type
    // with a `Unit` fallback — so obeying the style rule silently downgrades
    // every outline, hover-by-outline and editor breadcrumb to `-> Unit`.
    //
    // The invariant is exact and needs no inference of its own: an annotation
    // the checker can infer carries no information, so erasing it must leave
    // the reported symbols byte-identical. Anything else means tooling is
    // reading the source text rather than the type.
    //
    // Asserted over the real corpus rather than one hand-written probe, because
    // the single-case version in osprey-lsp cannot show the blast radius.
    let root = repo_root();
    let mut downgraded = Vec::new();
    for dir in ["tests", "examples", "benchmarks"] {
        for path in sources(&root.join(dir), "osp") {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            if !type_checks(&path, &source) {
                continue;
            }
            let display = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let lines: Vec<&str> = source.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                let Some((arrow, body, spelling)) = return_annotation(line) else {
                    continue;
                };
                let stripped = format!("{}{}", &line[..arrow], &line[body..]);
                let rebuilt = rebuild(&lines, index, &stripped);
                if !type_checks(&path, &rebuilt) {
                    continue;
                }
                if outline(&rebuilt) != outline(&source) {
                    downgraded.push(format!("{display}:{}  -> {spelling}", index + 1));
                }
            }
        }
    }
    assert!(
        downgraded.is_empty(),
        "deleting an inferable return annotation changed the reported symbols — \
         tooling is rendering the declared type with a `Unit` fallback instead of \
         the inferred type, so following CLAUDE.md's annotation rule downgrades \
         every one of these to `-> Unit`. {} affected:\n{}",
        downgraded.len(),
        downgraded.join("\n")
    );
}

#[test]
fn no_corpus_program_carries_a_removable_return_annotation() {
    let root = repo_root();
    let mut offenders = Vec::new();
    for dir in ["tests", "examples", "benchmarks"] {
        for path in sources(&root.join(dir), "osp") {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            // Only judge a program that is valid to begin with; the
            // must-reject corpus is graded by its own harness.
            if !type_checks(&path, &source) {
                continue;
            }
            let display = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            for (line, spelling) in redundant_in(&path, &source) {
                offenders.push(format!("{display}:{line}  -> {spelling}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "Hindley-Milner infers these return types, so the annotations are defects \
         (CLAUDE.md: \"If removing an annotation still compiles with identical output, \
         it was redundant — remove it\"). {} found:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
