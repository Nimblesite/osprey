//! The oracle the redundant-annotation gates share: an annotation is dead weight
//! when deleting it leaves a program that parses, type-checks, lowers to
//! self-contained IR and reports byte-identical symbols.

use crate::common::{repo_root, sources, undefined_symbols};
use std::fs;
use std::path::Path;

/// A reader: every annotation on one source line, as (byte range to delete,
/// spelling). Each is judged on its own against the unstripped baseline.
pub(crate) type Reader = fn(&str) -> Vec<(usize, usize, String)>;

/// Parse, type-check AND lower to IR.
///
/// Codegen is NOT optional here, and saying otherwise cost the tree a broken
/// build: `ec6a5cac` deleted ML signatures this gate called redundant, and
/// `verdict`, `workflows` and `type_equality_comprehensive` stopped compiling
/// with "a closure value with a still-generic type". All three still TYPE-CHECK
/// without their signatures — the annotation was what made a closure's type
/// concrete enough to lower. An oracle that stops at inference cannot see that,
/// so it reports a load-bearing annotation as dead weight.
/// And `is_ok()` is not the end of it either. The sibling gate exists because
/// `compile_program` can return `Ok` while handing back a module that
/// references a body it never emitted — IR that only clang rejects. A gate that
/// stopped at `is_ok()` could therefore demand a removal that BREAKS the build,
/// which is precisely how `ec6a5cac` happened. So the stripped candidate must
/// also be self-contained ([`undefined_symbols`]).
pub(crate) fn compiles(path: &Path, source: &str) -> bool {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    if !parsed.errors.is_empty() || !osprey_types::check_program(&parsed.program).is_empty() {
        return false;
    }
    osprey_codegen::compile_program(&parsed.program)
        .is_ok_and(|ir| undefined_symbols(&ir).is_empty())
}

/// What `--symbols` reports for `source`, via the exact path the CLI uses.
pub(crate) fn outline(source: &str) -> String {
    let parsed = osprey_syntax::parse_program(source);
    osprey_lsp::symbols_json(&parsed.program)
}

fn rebuild(lines: &[&str], index: usize, replacement: &str) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| if i == index { replacement } else { line })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `(line number, spelling)` `read` finds whose deletion costs the
/// program nothing.
///
/// "Still compiles" is NOT sufficient, and treating it as such gave this gate
/// thirteen false positives. An annotation can be the only thing that pins a
/// free type parameter — `toGpu([])` has no element type, `Error { .. }` never
/// constrains the Success side, a recursive `animate` cannot close its own
/// return, and a generic binder's `T` is a variable by construction. Delete one
/// of those and the program still compiles, because inference is free to leave
/// the slot open; what it loses is the TYPE. So redundancy is judged on the
/// type the checker can prove: an annotation is dead weight only when erasing
/// it leaves the reported signature byte-identical. That is exactly CLAUDE.md's
/// "still compiles with identical output", read strictly. Candidates are judged
/// one at a time, never stripped in a batch: deleting one parameter's
/// annotation can change what a sibling infers (#220).
pub(crate) fn redundant_in(path: &Path, source: &str, read: Reader) -> Vec<(usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let before = outline(source);
    lines
        .iter()
        .enumerate()
        .flat_map(|(index, line)| {
            read(line)
                .into_iter()
                .filter_map(|(start, end, spelling)| {
                    let stripped = format!("{}{}", line.get(..start)?, line.get(end..)?);
                    let rebuilt = rebuild(&lines, index, &stripped);
                    (compiles(path, &rebuilt) && outline(&rebuilt) == before)
                        .then(|| (index + 1, spelling))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every removable annotation `read` finds across the Default-flavor corpus,
/// as `path:line  spelling`. Only programs valid to begin with are judged; the
/// must-reject corpus is graded by its own harness.
pub(crate) fn corpus_offenders(read: Reader) -> Vec<String> {
    let root = repo_root();
    let mut offenders = Vec::new();
    for dir in [
        "src/osprey/tests",
        "src/osprey/examples",
        "src/toolchain/benchmarks",
    ] {
        for path in sources(&root.join(dir), "osp") {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            if !compiles(&path, &source) {
                continue;
            }
            let display = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            for (line, spelling) in redundant_in(&path, &source, read) {
                offenders.push(format!("{display}:{line}  {spelling}"));
            }
        }
    }
    offenders
}
