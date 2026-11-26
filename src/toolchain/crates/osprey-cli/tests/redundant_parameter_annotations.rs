//! Enforcement gate for the parameter half of "redundant annotations are
//! defects" (#220): `fn add(a, b) = a + b`, never `fn add(a: int, b: int)`.
//!
//! The return gate only ever read the arrow before the body, so a parameter
//! annotation passed every gate however obvious it was. This one reuses the
//! same oracle — strip ONE annotation, then require the program to parse,
//! type-check, lower to self-contained IR and report byte-identical symbols —
//! judging each parameter alone, because deleting one can change what a
//! sibling infers.

#[path = "common/annotation_oracle.rs"]
mod annotation_oracle;
mod common;

use annotation_oracle::{compiles, corpus_offenders, redundant_in};
use std::path::Path;

/// Parameter annotations that carry meaning beyond the type the body infers:
/// `any` is the erasure, not a description of an inferred type.
const LOAD_BEARING: [&str; 1] = ["any"];

/// How far `c` moves the bracket depth. The `>` of an arrow (`->`) closes
/// nothing, so `prev` is the character before `c`.
fn depth_step(prev: char, c: char) -> i32 {
    match c {
        '(' | '[' | '{' | '<' => 1,
        ')' | ']' | '}' => -1,
        '>' if prev != '-' => -1,
        _ => 0,
    }
}

/// The byte ranges of each top-level parameter in a `fn` header line: the text
/// between the parameter list's parentheses, split at depth-0 commas.
fn parameter_spans(line: &str) -> Vec<(usize, usize)> {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("fn ") || trimmed.starts_with("export fn ")) {
        return Vec::new();
    }
    let Some(open) = line.find('(') else {
        return Vec::new();
    };
    let (mut spans, mut start, mut depth, mut prev) = (Vec::new(), open + 1, 0, '(');
    for (offset, c) in line.get(open + 1..).unwrap_or_default().char_indices() {
        let at = open + 1 + offset;
        if depth == 0 && (c == ',' || c == ')') {
            spans.push((start, at));
            if c == ')' {
                return spans;
            }
            start = at + 1;
        } else {
            depth += depth_step(prev, c);
        }
        prev = c;
    }
    Vec::new() // the list does not close on this line: not a header this reads
}

/// Each parameter annotation on a `fn` header line, as (byte range of the
/// `: Type` to delete, spelling).
fn parameter_annotations(line: &str) -> Vec<(usize, usize, String)> {
    parameter_spans(line)
        .into_iter()
        .filter_map(|(start, end)| {
            let segment = line.get(start..end)?;
            let colon = start + segment.find(':')?;
            let spelling = line.get(colon + 1..end)?.trim().to_string();
            let stop = colon + line.get(colon..end)?.trim_end().len();
            (!spelling.is_empty() && !LOAD_BEARING.contains(&spelling.as_str()))
                .then_some((colon, stop, spelling))
        })
        .collect()
}

fn redundant_parameters(path: &Path, source: &str) -> Vec<(usize, String)> {
    redundant_in(path, source, parameter_annotations)
}

/// Both parameters are pinned by their uses: `==` against an `int` literal and
/// `&&`.
const INFERABLE: &str = r#"fn score(n: int, flag: bool) = "${n == 1 && flag}"
print(score(1, true))
"#;

/// `x` is used only through interpolation, so without its annotation the
/// checker infers the wider `printable`: the annotation narrows the reported
/// signature and is load-bearing.
const NARROWING: &str = r#"fn shown(x: string) = "${x}!"
print(shown("hi"))
"#;

/// A higher-order parameter: its own arrow sits inside the parameter list.
const HOF: &str = r#"fn apply(f: fn(int) -> int, n: int) = f(n)
print("${apply(|x| => wrapMul(x, 2), 4)}")
"#;

#[test]
fn the_parameter_reader_reads_every_shape_a_header_can_take() {
    let spellings = |line| {
        parameter_annotations(line)
            .into_iter()
            .map(|(_, _, s)| s)
            .collect::<Vec<_>>()
    };
    assert_eq!(spellings("fn add(a: int, b: int) = a + b"), ["int", "int"]);
    assert_eq!(
        spellings("fn apply(f: fn(int) -> int, n: int) -> int = f(n)"),
        ["fn(int) -> int", "int"],
        "a parameter's own arrow and parentheses stay inside that parameter"
    );
    assert_eq!(
        spellings("export fn keep(m: Map<string, List<int>>, k) = m"),
        ["Map<string, List<int>>"],
        "a comma inside type arguments does not split the parameter"
    );
    assert!(
        spellings("fn bare(a, b) = a").is_empty(),
        "no annotation, nothing read"
    );
    assert!(
        spellings("fn erased(x: any) = x").is_empty(),
        "`any` is the erasure"
    );
    assert!(
        spellings("let f = fn(x: int) => x").is_empty(),
        "only `fn` headers are read"
    );
    let (start, end, _) = parameter_annotations("fn add(a: int, b) = a").remove(0);
    assert_eq!(
        format!(
            "{}{}",
            &"fn add(a: int, b) = a"[..start],
            &"fn add(a: int, b) = a"[end..]
        ),
        "fn add(a, b) = a",
        "deleting the span leaves the bare parameter"
    );
}

#[test]
fn a_parameter_annotation_is_reported_exactly_when_the_checker_proves_it() {
    let probe = Path::new("parameters.osp");
    for program in [INFERABLE, NARROWING, HOF] {
        assert!(
            compiles(probe, program),
            "every probe must compile:\n{program}"
        );
    }
    assert_eq!(
        redundant_parameters(probe, INFERABLE),
        [(1, "int".to_string()), (1, "bool".to_string())],
        "both annotations are proved by their uses, so both are reported"
    );
    assert!(
        redundant_parameters(probe, NARROWING).is_empty(),
        "an annotation that narrows what inference proves must never be reported"
    );
}

#[test]
fn no_corpus_program_carries_a_removable_parameter_annotation() {
    let offenders = corpus_offenders(parameter_annotations);
    assert!(
        offenders.is_empty(),
        "Hindley-Milner infers these parameter types, so the annotations are defects \
         (CLAUDE.md: `fn add(a, b) = a + b`, not `fn add(a: int, b: int)`). {} found:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
