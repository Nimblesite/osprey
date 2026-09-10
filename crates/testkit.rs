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
    for needle in needles {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}
