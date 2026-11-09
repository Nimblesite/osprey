//! Tests for [TYPE-ANNOTATION-REDUNDANT] over whole programs: silent corpora,
//! group narrowing, curried ML headers and exact source metadata.

use crate::redundant::redundant_annotations;
use crate::redundant_tests::{messages, reports, silent, warnings};
use osprey_syntax::{parse_program_with_flavor, Flavor};
use std::fmt::Write as _;

#[test]
fn an_annotation_free_corpus_is_completely_silent() {
    silent(
        Flavor::Default,
        "type Point = { x: int, y: int }\n\
         fn px(p) = p.x\n\
         fn shout(s) = s + \"!\"\n\
         let p = Point { x: 1, y: 2 }\n\
         let out = shout(toString(px(p)))\n",
    );
}

/// Eight redundantly annotated functions around one whose parameter annotation
/// is doing real work — erasing `p: Point` would widen it to a structural row.
/// This is the shape the group-narrowing search has to get right.
fn mixed_corpus() -> String {
    let redundant = (0..8).fold(String::new(), |mut out, i| {
        let _ = writeln!(out, "fn f{i}(s: string) -> string = s + \"{i}\"");
        out
    });
    format!(
        "type Point = {{ x: int, y: int }}\n\
         {redundant}\
         fn keeper(p: Point) -> int = p.x\n\
         let v = keeper(Point {{ x: 1, y: 2 }})\n"
    )
}

#[test]
fn narrowing_does_not_lose_warnings_around_a_load_bearing_annotation() {
    // Erasing everything at once fails because of `keeper`, so the search
    // splits. Every redundant annotation must still be reported, and
    // `keeper`'s load-bearing parameter must not be.
    let raised = messages(Flavor::Default, &mixed_corpus());
    assert_eq!(raised.len(), 17, "{raised:?}");
    for index in 0..8 {
        assert!(
            raised.contains(&format!(
                "redundant type annotation on parameter `s` of `f{index}`: inference derives `string` without it"
            )),
            "lost the parameter of f{index}: {raised:?}"
        );
        assert!(
            raised.contains(&format!(
                "redundant return type annotation on `f{index}`: inference derives `string` without it"
            )),
            "lost the return type of f{index}: {raised:?}"
        );
    }
    assert!(
        raised.contains(
            &"redundant return type annotation on `keeper`: inference derives `int` without it"
                .to_string()
        ),
        "{raised:?}"
    );
    assert!(
        !raised
            .iter()
            .any(|m| m.contains("parameter `p` of `keeper`")),
        "reported a load-bearing parameter: {raised:?}"
    );
}

#[test]
fn narrowing_reports_in_source_order_regardless_of_how_it_searched() {
    // The search halves the annotation list and so visits it out of order.
    // What comes back must still read down the file.
    let raised = warnings(Flavor::Default, &mixed_corpus());
    let lines: Vec<u32> = raised
        .iter()
        .filter_map(|w| w.position.map(|p| p.line))
        .collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "warnings came out of order: {raised:?}");
    assert_eq!(lines.len(), raised.len(), "a warning lost its position");
}

#[test]
fn the_same_program_reports_the_same_warnings_every_run() {
    let source = mixed_corpus();
    let first = messages(Flavor::Default, &source);
    let second = messages(Flavor::Default, &source);
    assert_eq!(first, second, "detection is not deterministic");
}

#[test]
fn an_effect_operation_signature_is_never_reported() {
    // An `effect` declares its operations' types; there is no body to infer
    // them from, so they are declarations and not constraints.
    silent(
        Flavor::Default,
        "effect Log { line: fn(string) -> Unit }\n\
         fn main() = {\n\
             handle Log {\n\
                 line message => {}\n\
             }\n\
             perform Log.line(\"hi\")\n\
         }\n",
    );
}

/// The ML header `smaller : int -> int -> int` with `smaller a b = ...`.
/// One written line; the lowering turns it into a parameter, a curried
/// function-typed return, and a nested lambda carrying the rest.
const CURRIED_ML: &str = "smaller : int -> int -> int\n\
                          smaller a b = match a <= b\n    true => a\n    false => b\n";

#[test]
fn a_curried_ml_header_that_monomorphises_a_polymorphic_body_is_kept() {
    // `smaller a b = match a <= b ...` generalises to a comparison over any
    // one type. The header pins it to `int`, so it is doing real work and no
    // part of it is redundant. Judging the lowering's fragments one at a time
    // said the opposite — each slot looked derivable because the others were
    // still holding the type down — and reported four warnings for a line
    // that cannot be deleted.
    silent(Flavor::Ml, CURRIED_ML);
}

#[test]
fn a_curried_ml_header_never_blames_a_lambda_for_a_named_parameter() {
    // `b` is a parameter the reader named on the `smaller a b = ...` line.
    // Attributing it to `<lambda>` points at code nobody wrote.
    let raised = messages(Flavor::Ml, CURRIED_ML);
    assert!(
        !raised.iter().any(|m| m.contains("<lambda>")),
        "blamed a lambda for a written parameter: {raised:?}"
    );
}

#[test]
fn a_genuinely_redundant_curried_header_is_one_warning_naming_the_function() {
    // Here the body pins both parameters to `int` on its own, so the header
    // really is deletable — and it is ONE line, so it is ONE warning naming
    // the function and the whole signature. Reporting `(int) -> int` as the
    // "return type" would be the curried remainder, which appears nowhere in
    // the source.
    reports(
        Flavor::Ml,
        "combine : int -> int -> int\ncombine a b = (a - 1) * (b - 1)\n",
        &["redundant type signature on `combine`: inference derives `(int) -> (int) -> int` without it"],
    );
}

#[test]
fn a_default_signature_is_unaffected_by_the_curried_collapse() {
    // Nothing curries in Default flavor: each annotation is its own written
    // thing and keeps its own warning. The return type stays unreported
    // because the body only compares — something has to keep `smaller` at
    // `int`, and with both parameters gone that is the return.
    reports(
        Flavor::Default,
        "fn smaller(a: int, b: int) -> int = match a <= b {\n  true => a\n  false => b\n}\n",
        &[
            "redundant type annotation on parameter `a` of `smaller`: inference derives `int` without it",
            "redundant type annotation on parameter `b` of `smaller`: inference derives `int` without it",
        ],
    );
}

#[test]
fn a_hand_written_lambda_is_still_reported_against_the_lambda() {
    // The collapse is specific to a curry chain the lowering produced. A
    // lambda somebody actually wrote still names itself.
    reports(
        Flavor::Ml,
        "exclaim = \\(s: string) => s + \"!\"\nr = exclaim \"hi\"\n",
        &["redundant type annotation on parameter `s` of `<lambda>`: inference derives `string` without it"],
    );
}

#[test]
fn a_three_argument_curried_header_preserves_every_arrow() {
    // The whole written signature is one warning. Its three curried arrows
    // remain distinct from a function taking three arguments in one call.
    reports(
        Flavor::Ml,
        "clamp : int -> int -> int -> int\nclamp a b c = (a - 1) * (b - 1) * (c - 1)\n",
        &["redundant type signature on `clamp`: inference derives `(int) -> (int) -> (int) -> int` without it"],
    );
}

#[test]
fn the_reported_set_is_deletable_as_a_whole_not_only_one_at_a_time() {
    // `pick` is pinned to `int` by its annotations and nothing else: the body
    // only compares. Each annotation alone IS removable, because the other two
    // still hold the type down — so judging them one at a time reports all
    // three. Delete all three and `pick` generalises, which is a different
    // program. A reader acts on the whole list, so the whole list has to be
    // safe: the rule reports a set it has verified can go together.
    let raised = messages(
        Flavor::Default,
        "fn pick(a: int, b: int) -> int = match a <= b {\n  true => a\n  false => b\n}\nlet chosen = pick(1, 2)\n",
    );
    assert!(
        !raised
            .iter()
            .any(|m| m.starts_with("redundant return type annotation on `pick`")),
        "reported every annotation of a mutually-pinned signature: {raised:?}"
    );
    assert_eq!(
        raised,
        vec![
            "redundant type annotation on parameter `a` of `pick`: inference derives `int` without it",
            "redundant type annotation on parameter `b` of `pick`: inference derives `int` without it",
        ],
        "the set must be one the reader can delete whole"
    );
}

#[test]
fn source_metadata_matches_exact_annotations_without_message_parsing() {
    use crate::{redundant_annotation_sites, RedundantTarget};
    use osprey_syntax::{annotation_edits, AnnotationTarget};
    for (flavor, source, count) in [
        (
            Flavor::Default,
            "fn greet(name: string) -> string = \"hi \" + name\n",
            2,
        ),
        (
            Flavor::Ml,
            "greet : string -> string\ngreet (name : string) = \"hi \" + name\n",
            2,
        ),
        (Flavor::Ml, "greeting : string\ngreeting = \"hi\"\n", 1),
        (Flavor::Ml, "make : Unit -> string\nmake () = \"hi\"\n", 1),
    ] {
        let parsed = parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let sites = redundant_annotation_sites(&parsed.program);
        assert_eq!(sites.len(), count, "{source}");
        assert_eq!(
            sites.iter().map(|s| s.warning.clone()).collect::<Vec<_>>(),
            redundant_annotations(&parsed.program)
        );
        let ranges = annotation_edits(source, flavor);
        let mut edits = Vec::new();
        for site in sites {
            let found: Vec<_> = ranges
                .iter()
                .filter(|range| {
                    if let Some(position) = site.annotation_position {
                        return position == range.position;
                    }
                    site.warning.position == Some(range.owner_position)
                        && match (&site.target, &range.target) {
                            (
                                RedundantTarget::Parameter {
                                    name: left,
                                    index: a,
                                },
                                AnnotationTarget::Parameter {
                                    name: right,
                                    index: b,
                                },
                            ) => left == right && a == b,
                            (RedundantTarget::Return, AnnotationTarget::Return)
                            | (RedundantTarget::Binding, AnnotationTarget::Binding) => true,
                            _ => false,
                        }
                })
                .collect();
            assert_eq!(found.len(), 1, "ambiguous source site {site:?} in {source}");
            if let Some(found) = found.first() {
                edits.extend(found.edits.clone());
            }
        }
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
        let mut erased = source.to_owned();
        for edit in edits {
            erased.replace_range(edit.range, &edit.new_text);
        }
        let candidate = parse_program_with_flavor(&erased, flavor);
        assert!(
            candidate.errors.is_empty(),
            "{erased}\n{:?}",
            candidate.errors
        );
        assert!(
            crate::check_program(&candidate.program).is_empty(),
            "{erased}"
        );
        assert!(
            redundant_annotations(&candidate.program).is_empty(),
            "{erased}"
        );
    }
}

#[test]
fn interpolated_annotation_provenance_cannot_select_an_outer_type() {
    use crate::{redundant_annotation_sites, RedundantTarget};
    use osprey_syntax::annotation_edits;
    let fragment = "(fn(inner: (int) -> int) => inner(1) == 1)(fn(x) => x)";
    let Some(type_offset) = fragment.find("(int)") else {
        panic!("fixture type missing");
    };
    let collision_column = "let __frag__ = ".len() + type_offset;
    let outer = format!(
        "let outer:{}(int) -> int = fn(x) => x\n",
        " ".repeat(collision_column - "let outer:".len())
    );
    let source = format!("{outer}let rendered = \"\\n🦅 ${{{fragment}}}\"\n");
    let parsed = parse_program_with_flavor(&source, Flavor::Default);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(
        crate::check_program(&parsed.program),
        Vec::<crate::error::TypeError>::new()
    );
    let sites: Vec<_> = redundant_annotation_sites(&parsed.program).into_iter().filter(|site| {
        matches!(&site.target, RedundantTarget::Parameter { name, .. } if name == "inner")
    }).collect();
    let [site] = sites.as_slice() else {
        panic!("expected inner redundancy: {sites:?}");
    };
    let Some(position) = site.annotation_position else {
        panic!("missing fragment provenance");
    };
    assert_eq!(
        position.line, 2,
        "fragment-local line 1 must not collide with outer annotation"
    );
    let candidates: Vec<_> = annotation_edits(&source, Flavor::Default)
        .into_iter()
        .filter(|candidate| candidate.position == position)
        .collect();
    let [candidate] = candidates.as_slice() else {
        panic!("ambiguous source edit: {candidates:?}");
    };
    assert_eq!(
        source.get(candidate.highlight.clone()),
        Some(": (int) -> int")
    );
    assert!(candidate.highlight.start >= outer.len());
    let mut edits = candidate.edits.clone();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
    let mut erased = source;
    for edit in edits {
        erased.replace_range(edit.range, &edit.new_text);
    }
    assert!(
        erased.starts_with(&outer),
        "unrelated outer annotation was changed"
    );
    let after = parse_program_with_flavor(&erased, Flavor::Default);
    assert!(after.errors.is_empty(), "{erased}\n{:?}", after.errors);
    assert!(crate::check_program(&after.program).is_empty(), "{erased}");
    assert!(redundant_annotation_sites(&after.program).iter().all(
        |site| !matches!(&site.target, RedundantTarget::Parameter { name, .. } if name == "inner")
    ));
}

#[test]
fn satisfied_numeric_obligations_do_not_hide_redundant_annotations() {
    for (flavor, source) in [
        (Flavor::Default, "fn softWeight(d: float) = 1.0 / (1.0 + d + d * d)\nfn report() = {\n    handle Arith { divideByZero _ l => l }\n    print(softWeight(2.0))\n}\nreport()\n"),
        (Flavor::Ml, "softWeight : float -> float\nsoftWeight d = 1.0 / (1.0 + d + d * d)\nreport () =\n    handle Arith\n        divideByZero _ l => l\n    print (softWeight 2.0)\nreport ()\n"),
        (Flavor::Ml, "lerpQuarter : (float, float) -> float\nlerpQuarter (a, b) = a + (b - a) * 0.25\nprint (lerpQuarter (1.0, 2.0))\n"),
    ] {
        let raised = warnings(flavor, source);
        assert_eq!(raised.len(), 1, "{flavor}: {raised:?}\n{source}");
    }
}
