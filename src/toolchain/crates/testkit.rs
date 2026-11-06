//! Assertions shared by every crate's unit tests.
//!
//! Rendered output — LLVM IR, hover markdown, a docs page, an export document —
//! is checked the same way everywhere: does the text contain this, and not
//! that. Each crate used to spell that as one bare
//! `assert!(text.contains(needle))` per needle, which reports only `false` when
//! it trips, and the copies drifted apart on their failure messages while
//! asserting the same thing. Included by path rather than published as a
//! dev-dependency, so it stays one file with one set of wordings.

/// Assert `text` holds every needle, naming the one that is missing and
/// printing the whole rendering.
pub(crate) fn shows(text: &str, needles: &[&str]) {
    contents(text, needles, &[]);
}

/// Assert both required and forbidden fragments, retaining the full rendering.
pub(crate) fn contents(text: &str, present: &[&str], absent: &[&str]) {
    for (needles, expected) in [(present, true), (absent, false)] {
        for needle in needles {
            assert_eq!(
                text.contains(needle),
                expected,
                "contains({needle:?}) must be {expected} in:\n{text}"
            );
        }
    }
}

#[test]
fn mixed_text_expectations_check_both_polarities() {
    contents("visible", &["visible"], &["hidden"]);
}

#[test]
#[should_panic(expected = "contains(\"missing\") must be true")]
fn a_missing_required_fragment_fails() {
    shows("visible", &["missing"]);
}

#[test]
#[should_panic(expected = "contains(\"visible\") must be false")]
fn a_present_forbidden_fragment_fails() {
    contents("visible", &[], &["visible"]);
}
