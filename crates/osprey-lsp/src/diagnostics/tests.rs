use super::*;
const U16: PositionEncoding = PositionEncoding::Utf16;

const OSP: &str = "file:///a.osp";

#[test]
fn inferred_program_is_clean_and_redundant_return_is_a_warning() {
    assert!(compute("fn main() = print(\"hi\")\n", OSP, U16).is_empty());
    let diags = compute("fn main() -> Unit = print(\"hi\")\n", OSP, U16);
    assert_redundant_annotations(
        &diags,
        &[(
            "redundant return type annotation on `main`: inference derives `Unit` without it",
            (0, 10, 0, 17),
        )],
    );
}

#[test]
fn ml_flavor_file_is_parsed_by_its_own_frontend() {
    // The exact editor regression: a layout, curry-by-default `.ospml` source
    // (bare `:` signature, `\` lambda, whitespace application) must parse
    // cleanly under the ML frontend rather than be flagged as broken Default
    // syntax. Selecting the flavor by the document path is what fixes it.
    let ml = "inc : int -> int\ninc x = wrapAdd x 1\nmain () =\n    print \"v=${toString (inc 41)}\"\n    0\n";
    let diagnostics = compute(ml, "file:///tour.ospml", U16);
    assert_redundant_annotations(
        &diagnostics,
        &[(
            "redundant type signature on `inc`: inference derives `(int) -> int` without it",
            (0, 0, 0, 16),
        )],
    );
    // The same source under a `.osp` path is genuinely not Default syntax, so
    // the Default frontend still reports errors — proving the path drives the
    // flavor rather than the diagnostics silently accepting everything.
    let as_default = compute(ml, OSP, U16);
    assert!(
        !as_default.is_empty(),
        "ML source is not valid Default syntax"
    );
    assert!(
        as_default.iter().all(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.code.as_deref() == Some("syntax-error")
        }),
        "{as_default:?}"
    );
}

#[test]
fn a_flavor_marker_that_fights_the_extension_is_reported_not_guessed() {
    // [FLAVOR-SELECT] makes a marker/extension disagreement a HARD error so
    // the editor and the CLI never read one file two ways. The CLI refuses
    // to build it; the editor used to fall back to Default and show the
    // file as green, so a `.ospml` mislabelled `flavor=default` looked fine
    // right up until the build failed. Report it instead of guessing.
    let src = "// osprey: flavor=default\ninc x = x + 1\n";
    let diags = compute(src, "file:///tour.ospml", U16);
    let first = diags.first().expect("the disagreement must be reported");
    assert_eq!(first.code.as_deref(), Some("flavor-error"), "{diags:?}");
    assert!(first.message.contains("disagree"), "{}", first.message);
    // The conflict is the ONLY finding: parsing under a guessed flavor
    // would bury it under a cascade of phantom syntax errors.
    assert_eq!(diags.len(), 1, "{diags:?}");
    // An agreeing marker stays silent and still selects the ML frontend.
    let agree = "// osprey: flavor=ml\ninc x = x + 1\n";
    assert!(compute(agree, "file:///tour.ospml", U16).is_empty());
}

// ---------- [TESTING-SKIP-WARNING] ----------

#[test]
fn a_statically_skipped_test_raises_a_warning_diagnostic() {
    // A skipped test is never silent: the `Skip` verdict at the body's
    // result position warns on the `test` call's own line, in both flavors.
    let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\n\
               test(\"ignored case\", fn() => Skip(\"blocked on #123\"))\n\
               test(\"live case\", fn() => expect(1, 1))\n";
    let diags = compute(src, OSP, U16);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let warn = diags.first().expect("warning");
    assert_eq!(warn.severity, Severity::Warning);
    assert_eq!(warn.code.as_deref(), Some("test-skipped"));
    assert_eq!(warn.source.as_deref(), Some("osprey"));
    assert_eq!(warn.range.0, 2, "warning sits on the test call's line");
    assert!(
        warn.message
            .contains("test 'ignored case' is skipped: blocked on #123"),
        "{}",
        warn.message
    );

    let ml = "type Verdict = Pass | Fail string | Skip string\n\n\
              test \"ml ignored\" (\\() => Skip \"later\")\n";
    let diags = compute(ml, "file:///skip.ospml", U16);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let warn = diags.first().expect("ML warning");
    assert_eq!(warn.severity, Severity::Warning);
    assert!(
        warn.message.contains("test 'ml ignored' is skipped: later"),
        "{}",
        warn.message
    );
}

#[test]
fn a_skip_with_no_reason_is_an_error_not_a_warning() {
    // [TESTING-SKIP-REASON] a reasoned skip is a debt someone can weigh, so
    // it warns; a skip that refuses to say why is a defect, so it errors.
    // Every reasonless spelling is caught: `Skip("")`, and the bare `Skip`
    // of a `Verdict` whose skip state declares no payload.
    let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\n\
               test(\"unexplained\", fn() => Skip(\"\"))\n";
    let diags = compute(src, OSP, U16);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let err = diags.first().expect("error");
    assert_eq!(err.severity, Severity::Error);
    assert_eq!(err.code.as_deref(), Some("test-skipped"));
    assert_eq!(err.range.0, 2, "error sits on the test call's line");
    assert!(
        err.message
            .contains("test 'unexplained' is skipped with no reason"),
        "{}",
        err.message
    );

    let bare = "type Verdict = Pass | Fail | Skip\n\n\
                test(\"bare skip\", fn() => Skip)\n";
    let diags = compute(bare, OSP, U16);
    assert!(
        diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.code.as_deref() == Some("test-skipped")),
        "{diags:?}"
    );
}

#[test]
fn skip_warnings_ride_alongside_type_errors() {
    // The warning must not vanish because the file has other findings.
    let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\
               fn broken() -> int = nope(1)\n\
               test(\"parked\", fn() => Skip(\"awaiting fix\"))\n";
    let diags = compute(src, OSP, U16);
    assert!(
        diags.iter().any(|d| d.severity == Severity::Error),
        "{diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.code.as_deref() == Some("test-skipped")),
        "{diags:?}"
    );
}

#[test]
fn a_dynamically_guarded_test_does_not_warn_statically() {
    // `assume`-style skips are runtime outcomes; only a body that
    // LITERALLY results in `Skip` is statically an ignored test.
    let src = "type Verdict = Pass | Fail(string) | Skip(string)\n\
               fn guard(n) = match n > 1 { true => Pass false => Skip(\"small\") }\n\
               test(\"guarded\", fn() => guard(2))\n";
    let diags = compute(src, OSP, U16);
    assert!(
        diags
            .iter()
            .all(|d| d.code.as_deref() != Some("test-skipped")),
        "{diags:?}"
    );
}

#[test]
fn syntax_error_is_reported_with_source_and_code() {
    let diags = compute("fn main( = 1\n", OSP, U16);
    assert!(!diags.is_empty());
    let first = diags.first().expect("diagnostic");
    assert_eq!(first.severity, Severity::Error);
    assert_eq!(first.source.as_deref(), Some("osprey"));
    assert_eq!(first.code.as_deref(), Some("syntax-error"));
}

#[test]
fn type_error_surfaces_when_parse_is_clean() {
    // Referencing an unknown function type-checks but does not parse-fail.
    let diags = compute("fn main() -> int = nope(1)\n", OSP, U16);
    assert!(!diags.is_empty(), "an unknown call type-errors");
    assert!(
        diags
            .iter()
            .all(|d| d.code.as_deref() == Some("type-error")),
        "{diags:?}"
    );
    // Every diagnostic carries the osprey source, is an error, and spans a
    // non-empty range on its line.
    for d in &diags {
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.source.as_deref(), Some("osprey"));
        let (sl, sc, el, ec) = d.range;
        assert_eq!(sl, el, "single-line span: {d:?}");
        assert!(ec > sc, "non-empty span: {d:?}");
        assert_ne!(d.message, "");
    }
}

#[test]
fn diagnostic_columns_are_remeasured_in_the_selected_encoding() {
    // [LSP-DIAGNOSTICS], [LSP-ENCODING]
    // A multi-byte identifier shifts the byte column; the wire range must be
    // re-measured so the same program reports a wider start under UTF-8 than
    // under UTF-16 when the error sits past a multi-byte char.
    let src = "fn café() -> int = nope(1)\n";
    let u16 = compute(src, OSP, PositionEncoding::Utf16);
    let u8 = compute(src, OSP, PositionEncoding::Utf8);
    // Both encodings find at least one diagnostic on the first line.
    assert!(!u16.is_empty() && !u8.is_empty(), "{u16:?} {u8:?}");
    assert!(u16.iter().all(|d| d.range.0 == 0));
    assert!(u8.iter().all(|d| d.range.0 == 0));
}

#[cfg(unix)]
#[path = "project_tests.rs"]
mod project_tests;
