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

/// Parse, type-check AND lower to IR.
///
/// Codegen is NOT optional here, and saying otherwise cost the tree a broken
/// build: `ec6a5cac` deleted ML signatures this gate called redundant, and
/// `verdict`, `workflows` and `type_equality_comprehensive` stopped compiling
/// with "a closure value with a still-generic type". All three still TYPE-CHECK
/// without their signatures — the annotation was what made a closure's type
/// concrete enough to lower. An oracle that stops at inference cannot see that,
/// so it reports a load-bearing annotation as dead weight.
fn compiles(path: &Path, source: &str) -> bool {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    parsed.errors.is_empty()
        && osprey_types::check_program(&parsed.program).is_empty()
        && osprey_codegen::compile_program(&parsed.program).is_ok()
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

/// Every `(line number, spelling)` whose deletion costs the program nothing.
///
/// "Still compiles" is NOT sufficient, and treating it as such gave this gate
/// thirteen false positives. An annotation can be the only thing that pins a
/// free type parameter — `toGpu([])` has no element type, `Error { .. }` never
/// constrains the Success side, a recursive `animate` cannot close its own
/// return, and a generic binder's `T` is a variable by construction. Delete one
/// of those and the program still compiles, because inference is free to leave
/// the slot open; what it loses is the TYPE. The checker then has nothing to
/// report and every outline, hover and breadcrumb falls back to `Unit`.
///
/// So redundancy is judged on the type the checker can prove, not on exit
/// status: an annotation is dead weight only when erasing it leaves the
/// reported signature byte-identical. That is exactly CLAUDE.md's "still
/// compiles with identical output", read strictly.
fn redundant_in(path: &Path, source: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let before = outline(source);
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let (arrow, body, spelling) = return_annotation(line)?;
            let stripped = format!("{}{}", &line[..arrow], &line[body..]);
            let rebuilt = rebuild(&lines, index, &stripped);
            (compiles(path, &rebuilt) && outline(&rebuilt) == before)
                .then(|| (index + 1, spelling))
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

/// A `Result` return that both branches determine COMPLETELY: the Success arm
/// fixes the payload to `int`, the Error arm fixes the reason to `string`.
/// Nothing here is left open, and the annotated twin below proves the type is
/// spellable and that both forms run identically.
const BOTH_ARMS: &str = r#"fn bothArms(f) = if f { Success { value: 1 } } else { Error { message: "e" } }
print("${(bothArms(true)) ?: 0}")
"#;

const BOTH_ARMS_ANNOTATED: &str = r#"fn bothArms(f) -> Result<int, string> = if f { Success { value: 1 } } else { Error { message: "e" } }
print("${(bothArms(true)) ?: 0}")
"#;

#[test]
fn a_result_determined_by_its_constructor_arms_is_reported_not_defaulted_to_unit() {
    // This gate used to assert something FALSE, and the corpus proved it: it
    // read every `-> Unit` downgrade as a tooling defect, when thirteen of them
    // were annotations doing real work. `toGpu([])` has no element type to
    // infer, `Error { .. }` never mentions the Success side, `animate` recurses
    // through its own return, and `identity<T>` binds a variable on purpose.
    // For those, `Unit` is the checker honestly reporting that it proved
    // nothing, and the annotation is load-bearing. `redundant_in` now judges on
    // the proved type for exactly that reason.
    //
    // What survives that correction is a REAL defect, reduced to the narrowest
    // program that shows it. Both arms here are concrete, so `Result<int,
    // string>` is fully determined — the annotated and inferred twins compile
    // and both print `1`. Yet the inferred one is reported `-> Unit`: a
    // `Result` assembled from `Success`/`Error` constructors never reaches the
    // recorded return type, while the same type arriving from a builtin
    // (`parseInt`, `/`) reports fine. So obeying CLAUDE.md's annotation rule
    // here really does cost the outline its type. [LSP-HOVER-INFERRED-SIGNATURE]
    let probe = Path::new("both_arms.osp");
    assert!(
        compiles(probe, BOTH_ARMS_ANNOTATED),
        "the control must compile, or the type is not even spellable"
    );
    assert!(
        compiles(probe, BOTH_ARMS),
        "the inferred twin must compile, or this pins a type error, not a report"
    );

    let control = outline(BOTH_ARMS_ANNOTATED);
    assert!(
        control.contains("Result<int, string>"),
        "the control must report the written type; got {control}"
    );

    let inferred = outline(BOTH_ARMS);
    assert!(
        !inferred.contains("-> Unit"),
        "both arms determine `Result<int, string>` completely and the program \
         runs identically either way, yet erasing the annotation reports the \
         function as returning Unit. A `Result` built from Success/Error \
         constructors never reaches the recorded return type. got {inferred}"
    );
    assert!(
        inferred.contains("Result<int, string>"),
        "the inferred signature must name the type the checker proved; got {inferred}"
    );
    assert_eq!(
        inferred, control,
        "an annotation the checker can prove carries no information, so erasing \
         it must leave the reported symbols byte-identical"
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
            if !compiles(&path, &source) {
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
