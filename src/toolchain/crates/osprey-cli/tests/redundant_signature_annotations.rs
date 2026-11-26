//! Enforcement gate for "redundant annotations are defects" over the ML flavor
//! (#215). An ML signature is a standalone `describe : JsonValue -> string`
//! line above the definition, which neither the return nor the parameter
//! reader sees, so every `.ospml` source passed both gates unread.
//!
//! The oracle is the shared one, kept whole because the stakes are known:
//! `ec6a5cac` deleted ML signatures by hand and took `verdict`, `workflows` and
//! `type_equality_comprehensive` to "a closure value with a still-generic type".
//! A signature is redundant only when deleting it leaves a program that parses,
//! type-checks, lowers to self-contained IR and reports byte-identical symbols.
//! The line is blanked rather than removed, so no symbol below it moves.

#[path = "common/annotation_oracle.rs"]
mod annotation_oracle;
mod common;

#[path = "common/outlined.rs"]
mod outlined;

use annotation_oracle::{compiles, corpus_offenders, redundant_in};
use outlined::outlined;
use std::path::Path;

/// Signatures that carry meaning beyond the type the body infers: `any` is the
/// erasure, not a description of an inferred type.
const LOAD_BEARING: [&str; 1] = ["any"];

/// `text` without a leading `export `.
fn unexported(text: &str) -> &str {
    text.strip_prefix("export ").unwrap_or(text)
}

/// The name a line starts with, and the rest of the line after it.
fn leading_name(text: &str) -> Option<(&str, &str)> {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\''))
        .unwrap_or(text.len());
    let name = text.get(..end)?;
    let starts_lower = name.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    starts_lower.then(|| (name, text.get(end..).unwrap_or_default()))
}

/// Whether `line` defines `name`: `name args = …`, `name (a, b) = …` or
/// `name = …`, exported or not.
fn defines(line: &str, name: &str) -> bool {
    leading_name(unexported(line.trim_start())).is_some_and(|(found, rest)| {
        found == name
            && (rest.starts_with(' ') || rest.starts_with('('))
            && !rest.trim_start().starts_with(':')
    })
}

/// The signature on line `index`, as (the whole line, its type): a
/// `name : Type` line whose next line defines `name`. A record field has the
/// same shape but is followed by another field, never by a definition. Only an
/// [`outlined`] declaration is judged: a local ascription such as
/// `empty : List<string>` over `empty = []` never reaches the outline.
fn signature_at(lines: &[&str], index: usize) -> Vec<(usize, usize, String)> {
    let Some(line) = lines.get(index) else {
        return Vec::new();
    };
    let Some((name, rest)) = leading_name(unexported(line.trim_start())) else {
        return Vec::new();
    };
    let Some(spelling) = rest.trim_start().strip_prefix(':').map(str::trim) else {
        return Vec::new();
    };
    let defined = lines.get(index + 1).is_some_and(|next| defines(next, name));
    // An effect row rides on the type, and the body alone may not pin it.
    let judged =
        !spelling.is_empty() && !spelling.contains('!') && !LOAD_BEARING.contains(&spelling);
    if defined && judged && outlined(lines, index) {
        vec![(0, line.len(), spelling.to_string())]
    } else {
        Vec::new()
    }
}

fn redundant_signatures(path: &Path, source: &str) -> Vec<(usize, String)> {
    redundant_in(path, source, signature_at)
}

/// `==` against an `int` literal pins the parameter, and the body is a `bool`.
const INFERABLE: &str = "same : int -> bool\nsame n = n == 1\nprint \"${same 1}\"\n";

/// `x` is used only through interpolation, so without the signature the
/// checker infers the wider `printable`: the signature narrows the reported
/// type and is load-bearing.
const NARROWING: &str = "shown : string -> string\nshown x = \"${x}!\"\nprint (shown \"hi\")\n";

#[test]
fn the_signature_reader_tells_a_signature_from_a_record_field() {
    let spellings = |source: &str| {
        let lines: Vec<&str> = source.lines().collect();
        (0..lines.len())
            .flat_map(|index| signature_at(&lines, index))
            .map(|(_, _, spelling)| spelling)
            .collect::<Vec<_>>()
    };
    assert_eq!(spellings(INFERABLE), ["int -> bool"]);
    assert_eq!(
        spellings("module Shapes\n    export area : Shape -> int\n    export area shape = 1\n"),
        ["Shape -> int"],
        "an exported, indented signature in a module is read"
    );
    assert_eq!(
        spellings("pair : (int, int) -> int\npair (a, b) = a\n"),
        ["(int, int) -> int"],
        "a tuple-parameter definition is a definition"
    );
    assert!(
        spellings("type Delta =\n    delta : int\n    minus : int\n").is_empty(),
        "record fields are never signatures"
    );
    assert!(
        spellings("loose : int -> int\n\nother n = n\n").is_empty(),
        "a signature is only read when its own definition follows it"
    );
    assert!(
        spellings("dyn : any\ndyn = 42\n").is_empty(),
        "`any` is the erasure"
    );
    assert!(
        spellings("pick () =\n    chosen : float32\n    chosen = 0.2\n    chosen\n").is_empty(),
        "a local ascription never reaches the outline, so it is never judged"
    );
    assert_eq!(
        spellings("module Shapes\n    area : int -> int\n    area n = n\n"),
        ["int -> int"],
        "a module member is outlined, so it is judged"
    );
    assert!(
        spellings("shout : string -> Unit !Console\nshout m = perform Console.emit m\n").is_empty(),
        "an effect row rides on the type and is not judged"
    );
}

#[test]
fn a_signature_is_reported_exactly_when_the_checker_proves_it() {
    let probe = Path::new("signatures.ospml");
    for program in [INFERABLE, NARROWING] {
        assert!(
            compiles(probe, program),
            "every probe must compile:\n{program}"
        );
    }
    assert_eq!(
        redundant_signatures(probe, INFERABLE),
        [(1, "int -> bool".to_string())],
        "a signature the checker proves on its own is dead weight"
    );
    assert!(
        redundant_signatures(probe, NARROWING).is_empty(),
        "a signature that narrows what inference proves must never be reported"
    );
}

#[test]
fn no_ml_corpus_program_carries_a_removable_signature() {
    let offenders = corpus_offenders("ospml", signature_at);
    assert!(
        offenders.is_empty(),
        "Hindley-Milner infers these signatures, so they are defects (CLAUDE.md: \
         \"Still compiles with identical output without it → remove it\"). {} found:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
