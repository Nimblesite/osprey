//! Enforcement gate for the "redundant annotations are defects" rule.
//!
//! Osprey is Hindley-Milner, so every type the compiler can infer must be left
//! off the source. That rule lived only in prose, and nothing in the tree
//! checked it: `any_type_comprehensive` carried EIGHT removable `-> string`
//! annotations through every gate on the way to green. A style rule no gate
//! asserts is a suggestion.
//!
//! An annotation is redundant when deleting it leaves a program that still
//! type-checks with the same meaning. Only `-> any` is excluded by
//! construction: it is the ERASURE, not a description of an inferred type —
//! dropping it from `fn getDynamicValue() -> any = 42` still compiles but
//! infers `int`, which is a different program. Effectful returns are excluded
//! for the same reason: the row is not always recoverable from the body alone.

mod common;
#[path = "common/effect_execution.rs"]
mod effect_execution;

#[path = "common/annotation_oracle.rs"]
mod annotation_oracle;

use annotation_oracle::{compiles, corpus_offenders, outline, redundant_in};
use std::path::Path;

/// Return annotations that carry meaning beyond the type the body infers.
///
/// `Unit` used to sit here too, and it was a FALSE exemption by the time this
/// gate went green: it made every `-> Unit` unreportable, hiding removable ones
/// at `json_document_query.test.osp:25` and in `src/osprey/examples/statefulhttp`. The
/// exemption was only ever load-bearing because the outline itself faked `Unit`
/// for a return type nobody proved — annotated and bare read identically, so
/// the oracle could not tell a written `Unit` from an absent one. That fallback
/// is gone ([TYPE-RENDER-HOLES]: an unproved return drops the arrow entirely),
/// so `-> Unit` is now reported exactly when the checker proves `Unit` on its
/// own, like every other spelling.
const LOAD_BEARING: [&str; 1] = ["any"];

/// The concrete return annotation on a `fn` line, as (byte range, spelling).
///
/// `rfind` rather than `find`: a higher-order parameter carries its own arrow
/// (`fn apply(f: fn(int) -> int) -> string`), and the LAST arrow before the `=`
/// is the one that belongs to the declaration.
///
/// The second `find` catches a declaration whose body starts on the NEXT line,
/// where `=` ends the header. It was written `trim_end`, which cannot ever be
/// `"="`: the index points at the SPACE before it, so the slice is `" ="` and
/// only the trailing side gets trimmed. The arm was dead, and `api_browser.osp`
/// carried a removable `-> Unit` through the gate because of it.
fn return_annotation(line: &str) -> Option<(usize, usize, String)> {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("fn ") || trimmed.starts_with("export fn ")) {
        return None;
    }
    let body = line.find(" = ").or_else(|| {
        line.find(" =")
            .filter(|i| line.get(*i..).unwrap_or_default().trim() == "=")
    })?;
    let arrow = line.get(..body)?.rfind(" -> ")?;
    let spelling = line.get(arrow + 4..body)?.trim().to_string();
    // An effect row rides on the return type; the body alone may not pin it.
    if spelling.is_empty() || spelling.contains('!') || LOAD_BEARING.contains(&spelling.as_str()) {
        return None;
    }
    Some((arrow, body, spelling))
}

/// Every removable return annotation in `source`.
fn redundant_returns(path: &Path, source: &str) -> Vec<(usize, String)> {
    redundant_in(path, source, |line| return_annotation(line).into_iter().collect())
}

/// `onlySuccess` builds no `Error`, so nothing in this program determines E
/// ([ERR-RESULT] rule 2). `-> Result<int, string>` is the only thing that
/// could, which makes it load-bearing.
const FREE_ERROR_SIDE: &str = r#"fn onlySuccess(n) = Success { value: length(n) }
print("${(onlySuccess("a")) ?: 0}")
"#;

/// The same program with the free side written down — the annotation that
/// [`FREE_ERROR_SIDE`] is missing, and the only thing that can supply it.
const FREE_ERROR_SIDE_ANNOTATED: &str = r#"fn onlySuccess(n) -> Result<int, string> = Success { value: length(n) }
print("${(onlySuccess("a")) ?: 0}")
"#;

/// The same shape with the error side pinned BY CONTEXT instead: `parseInt`
/// returns `Result<int, string>` and the `Error` arm carries a `string`, so
/// the checker proves the whole type on its own.
const PINNED_ERROR_SIDE: &str = r#"fn fromBuiltin(f) = if f { parseInt("7") } else { Error { message: "e" } }
print("${(fromBuiltin(true)) ?: 0}")
"#;

#[test]
fn a_return_type_is_reported_exactly_when_the_checker_proved_all_of_it() {
    // This gate twice asserted something FALSE, and the corpus refuted it both
    // times. It first read every `-> Unit` as a tooling defect; thirteen of the
    // thirteen turned out to be annotations doing real work — `toGpu([])` has
    // no element type to infer, `animate` recurses through its own return, and
    // `identity<T>` binds a variable on purpose. It then swung the other way and
    // asserted that `Unit` is what an unproved slot SHOULD read as. It is not:
    // writing `-> Unit` on `bothArms` is a type error ("cannot unify Unit with
    // Result<t5, t6>"), so the checker knows more than `Unit` and the fallback
    // is a display lie. How a half-proved slot renders belongs to
    // `osprey-lsp`'s own gate ([LSP-HOVER-INFERRED-SIGNATURE]); this one must
    // not pin whatever spelling it currently picks.
    //
    // What IS this gate's business holds under either spelling: a body that
    // builds no `Error` leaves `E` undecided ([ERR-RESULT] rule 2). An
    // annotation supplying that side therefore carries information the body
    // does not, and erasing it must visibly change the
    // reported signature — which is what makes `redundant_in`'s "identical
    // reported signature" the right oracle rather than mere compilation.
    let probe = Path::new("error_side.osp");
    assert!(
        compiles(probe, FREE_ERROR_SIDE) && compiles(probe, PINNED_ERROR_SIDE),
        "both probes must compile, or this pins a type error rather than a report"
    );

    // E free: only the annotation can supply it, so erasing it costs the reader
    // a type and the gate must never recommend that.
    assert!(
        compiles(probe, FREE_ERROR_SIDE_ANNOTATED),
        "the annotated twin must compile, or it pins a type error rather than a report"
    );
    assert_ne!(
        outline(FREE_ERROR_SIDE_ANNOTATED),
        outline(FREE_ERROR_SIDE),
        "an annotation that supplies a side inference left open must change what \
         is reported; equal outlines would mean the annotation is being ignored"
    );
    assert!(
        redundant_returns(probe, FREE_ERROR_SIDE_ANNOTATED).is_empty(),
        "stripping must never be RECOMMENDED for a signature the checker cannot \
         rebuild on its own"
    );

    // E pinned by context: proved, so reported in full without any annotation.
    let pinned = outline(PINNED_ERROR_SIDE);
    assert!(
        pinned.contains("fn fromBuiltin(f: bool) -> Result<int, string>"),
        "a fully proved return type must be reported without an annotation; got {pinned}"
    );
    assert!(
        !pinned.contains("-> Unit"),
        "the checker proved this one, so nothing may fall back to Unit; got {pinned}"
    );

    // The payoff: writing the proved type down adds nothing, so the gate calls
    // it redundant and the outline is byte-identical either way.
    let annotated = PINNED_ERROR_SIDE.replacen(
        "fn fromBuiltin(f) =",
        "fn fromBuiltin(f) -> Result<int, string> =",
        1,
    );
    assert_eq!(
        outline(&annotated),
        pinned,
        "an annotation the checker can prove carries no information, so writing \
         it down must leave the reported symbols byte-identical"
    );
    assert_eq!(
        redundant_returns(probe, &annotated)
            .into_iter()
            .map(|(_, spelling)| spelling)
            .collect::<Vec<_>>(),
        vec!["Result<int, string>".to_string()],
        "and the gate must report exactly that annotation as removable"
    );
}

/// A declaration whose body starts on the NEXT line, so `=` ends the header.
/// This is the form the scanner's second arm exists for.
const BODY_ON_NEXT_LINE: &str = r#"fn describe(n: int) -> string =
    match n {
        0 => "zero"
        _ => "many"
    }
print(describe(1))
"#;

/// A higher-order parameter carries its own arrow, so the LAST one before the
/// body is the declaration's.
const HOF_PARAMETER: &str = r#"fn apply(f: fn(int) -> int, n: int) -> int = f(n)
print("${apply(f: |x| => wrapMul(x, 2), n: 4)}")
"#;

/// #163: a return annotation cannot change arithmetic values. This fixture
/// lives outside the corpus because its intentionally inferable annotation
/// must not exempt ordinary corpus sources from the annotation gate.
#[test]
fn arithmetic_return_annotations_preserve_values_in_both_flavors() {
    let default = r#"
fn inferredSum(a, b) = a + b
fn annotatedSum(a, b) -> int = a + b
fn parity() = {
    handle Arith { overflow _ _ _ wrapped => wrapped }
    print("${inferredSum(20, 22)}:${inferredSum(9223372036854775807, 1)}:${annotatedSum(9223372036854775807, 1)}")
}
parity()
"#;
    let ml = r#"
inferredSum (a, b) = a + b
annotatedSum : (int, int) -> int
annotatedSum (a, b) = a + b
parity () =
    handle Arith
        overflow _ _ _ wrapped => wrapped
    print "${inferredSum (20, 22)}:${inferredSum (9223372036854775807, 1)}:${annotatedSum (9223372036854775807, 1)}"
parity ()
"#;
    let expected = "42:-9223372036854775808:-9223372036854775808\n";
    effect_execution::assert_flavored_output("annotation_parity", "osp", default, expected);
    effect_execution::assert_flavored_output("annotation_parity", "ospml", ml, expected);
}

/// A provable `-> Unit`. `print` returns `Unit`, so the checker reaches the
/// same answer with the annotation deleted — and `Unit` was exempted outright
/// until this branch, which is exactly why it needs a POSITIVE case.
const PROVABLE_UNIT: &str = r#"fn announce(name: string) -> Unit = print("hi ${name}")
announce("world")
"#;

/// The same spelling with an effect row, which must stay excluded: the row
/// rides on the return type and the body alone may not pin it.
const EFFECTFUL_UNIT: &str = r#"effect Console { emit: fn(string) -> Unit }
fn shout(m: string) -> Unit !Console = perform Console.emit(m)
let _ = {
    handle Console {
        emit m => print(m)
    }
    shout("hey")
}
"#;

/// `-> any` is the ERASURE. Dropping it still compiles but infers `int`, which
/// is a different program, so it must never be reported.
const ERASED_RETURN: &str = r#"fn dynamic() -> any = 42
match dynamic() {
    x: int => print("${x}")
    _ => print("other")
}
"#;

#[test]
fn the_scanner_reads_every_shape_a_declaration_can_take() {
    let probe = Path::new("shapes.osp");

    // A header ending in a bare `=` is a real declaration and its annotation is
    // just as removable as one written inline. The arm that reads this shape
    // was DEAD — it tested `line[i..].trim_end() == "="` where `i` points at the
    // SPACE, so the slice is `" ="` and the comparison could never hold. Thirteen
    // removable annotations sat behind it, `api_browser.osp:412` among them.
    assert!(compiles(probe, BODY_ON_NEXT_LINE), "the probe must compile");
    let found = return_annotation("fn describe(n: int) -> string =")
        .expect("a header ending in `=` carries a return annotation");
    assert_eq!(found.2, "string", "and its spelling is the declared type");
    assert_eq!(
        redundant_returns(probe, BODY_ON_NEXT_LINE)
            .into_iter()
            .map(|(_, s)| s)
            .collect::<Vec<_>>(),
        vec!["string".to_string()],
        "so an inferable one is reported, exactly as the inline form is"
    );

    // A higher-order parameter's arrow belongs to the PARAMETER. Reading it as
    // the declaration's would report `int` (the parameter's result) and strip
    // the wrong span, producing a program that no longer parses.
    let hof = return_annotation("fn apply(f: fn(int) -> int, n: int) -> int = f(n)")
        .expect("the declaration's own arrow is the last one before the body");
    assert_eq!(
        hof.2, "int",
        "the spelling is the declaration's return type"
    );
    assert!(
        compiles(probe, HOF_PARAMETER),
        "and the probe itself must compile, or this pins a parse error"
    );

    // `any` is excluded by construction, in both directions: it is not reported,
    // and the reason is that erasing it changes what the program means.
    assert!(
        return_annotation("fn dynamic() -> any = 42").is_none(),
        "`-> any` is the erasure, not a description of an inferred type"
    );
    assert!(
        redundant_returns(probe, ERASED_RETURN).is_empty(),
        "so no `any` return is ever reported as removable"
    );

    // An effect row rides on the return type and is not always recoverable from
    // the body, so an effectful return is excluded whatever it returns.
    assert!(
        return_annotation("fn shout(m: string) -> Unit !Console = perform Console.emit(m)")
            .is_none(),
        "an effectful return carries a row the body alone may not pin"
    );
    assert!(
        compiles(probe, EFFECTFUL_UNIT) && redundant_returns(probe, EFFECTFUL_UNIT).is_empty(),
        "and no effectful `-> Unit` is ever reported, however provable it looks"
    );

    // `Unit` itself is NOT exempt, and this is the case that proves it. It was
    // on the exemption list until this branch, which made every `-> Unit`
    // unreportable and hid removable ones in `json_document_query` and
    // `src/osprey/examples/statefulhttp`. `print` returns `Unit`, so the checker reaches
    // the same answer without the annotation and it must be reported.
    assert!(compiles(probe, PROVABLE_UNIT), "the probe must compile");
    assert_eq!(
        redundant_returns(probe, PROVABLE_UNIT)
            .into_iter()
            .map(|(_, s)| s)
            .collect::<Vec<_>>(),
        vec!["Unit".to_string()],
        "a `-> Unit` the checker proves on its own is dead weight like any other"
    );
}

/// Sweeps the Default flavor only.
///
/// This is a REAL hole and is named rather than papered over: an ML signature
/// is a standalone `describe : JsonValue -> string` line, not an arrow inside a
/// `fn` header, so neither [`return_annotation`] nor the deletion it performs
/// applies to a `.ospml` file — 118 of them are ungated by this test. Gating
/// them needs its own reader and its own removal rule, and the stakes are known
/// to be real: `ec6a5cac` deleted ML signatures by hand and took `verdict`,
/// `workflows` and `type_equality_comprehensive` from green to a codegen error.
/// Tracked separately; do not read a green run here as ML coverage.
#[test]
fn no_corpus_program_carries_a_removable_return_annotation() {
    let offenders = corpus_offenders(|line| return_annotation(line).into_iter().collect());
    assert!(
        offenders.is_empty(),
        "Hindley-Milner infers these return types, so the annotations are defects \
         (CLAUDE.md: \"If removing an annotation still compiles with identical output, \
         it was redundant — remove it\"). {} found:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
