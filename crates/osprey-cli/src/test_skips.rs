//! TAP skip diagnostics for `osprey test`. Implements
//! [TESTING-SKIP-WARNING-RUN], [TESTING-SKIP-REASON] and [TESTING-TAP-AMBIGUITY]
//! (docs/specs/0027-TestingFramework.md).
//!
//! A skipped case is a case that did not run, and a run that says nothing about
//! it reads exactly like a run where it passed. Every `# SKIP` directive is
//! echoed: with a reason as a `warning:`, without one as an `error:` that fails
//! the suite. The hard part is a case whose NAME contains `# SKIP` — the
//! declared names are what break that tie.

use std::path::Path;

/// The suite's statically-discoverable test names, used only to disambiguate a
/// TAP description that itself contains `# SKIP` ([TESTING-TAP-AMBIGUITY]).
/// An unreadable or unparsable file yields none — the suite already failed
/// loudly elsewhere, and skip detection then falls back to the plain split.
pub(crate) fn declared_test_names(file: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(flavor) = osprey_syntax::resolve_flavor(None, &file.display().to_string(), &source)
    else {
        return Vec::new();
    };
    osprey_lsp::test_case_names(&osprey_syntax::parse_program_with_flavor(&source, flavor).program)
}

/// One diagnostic line per TAP `# SKIP` directive in a suite's output — a
/// skipped case is never silent ([TESTING-SKIP-WARNING-RUN], [TESTING-TAP]).
pub(crate) fn skip_diagnostics(stdout: &[u8], declared: &[String]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| skip_diagnostic(line, declared))
        .collect()
}

/// The prefix an unexplained skip carries ([TESTING-SKIP-REASON]). Callers
/// match on it to decide whether the suite failed.
pub(crate) const SKIP_ERROR: &str = "error: ";

/// The diagnostic for one TAP line carrying the `# SKIP` directive
/// (`ok N - name # SKIP reason`); `None` for every other line.
///
/// A reasoned skip is a `warning:`; a directive that names no reason is an
/// `error:` and fails the suite ([TESTING-SKIP-REASON]) — a hole in coverage
/// nobody wrote a cause for cannot be weighed, only discovered later.
///
/// A test NAME may itself contain `# SKIP`, which makes the raw TAP line
/// ambiguous ([TESTING-TAP-AMBIGUITY]). The declared names break the tie: a
/// description that matches one verbatim is that case's whole name, so the
/// case ran and no diagnostic is due.
fn skip_diagnostic(line: &str, declared: &[String]) -> Option<String> {
    let description = line.strip_prefix("ok ")?.split_once(" - ")?.1;
    if declared.iter().any(|known| known == description) {
        return None;
    }
    // The directive is always LAST on the line, so split from the right: a name
    // that itself contains `# SKIP` keeps all of it ([TESTING-TAP-AMBIGUITY]).
    let (name, directive) = description.rsplit_once(" # SKIP")?;
    let reason = directive.trim();
    Some(if reason.is_empty() {
        format!("{SKIP_ERROR}test '{name}' skipped with no reason; every skip must name one")
    } else {
        format!("warning: test '{name}' skipped: {reason}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // [TESTING-SKIP-WARNING]: every TAP `# SKIP` directive becomes one stderr
    // diagnostic, reason included; passing, failing, and plan lines stay
    // silent. [TESTING-SKIP-REASON]: a directive with no reason is an `error:`,
    // not a `warning:`, and the caller fails the suite on it.
    #[test]
    fn skip_warnings_echo_each_tap_skip_directive() {
        let stdout =
            b"ok 1 - kept\nok 2 - parked # SKIP blocked on #123\nnot ok 3 - bad\nok 4 - bare # SKIP \n1..4\n";
        let reported = skip_diagnostics(stdout, &[]);
        assert_eq!(
            reported,
            [
                "warning: test 'parked' skipped: blocked on #123",
                "error: test 'bare' skipped with no reason; every skip must name one",
            ]
        );
        assert!(
            reported
                .iter()
                .filter(|line| line.starts_with(SKIP_ERROR))
                .count()
                == 1,
            "exactly the unexplained skip fails the suite: {reported:?}"
        );
        assert!(skip_diagnostics(b"ok 1 - clean\n1..1\n", &[]).is_empty());
    }

    // [TESTING-TAP-AMBIGUITY] a PASSING case whose NAME contains `# SKIP`
    // must not be reported as skipped: the declared names break the tie.
    #[test]
    fn a_test_name_containing_the_skip_directive_is_not_a_skip() {
        let declared = [String::from("name with # SKIP inside it")];
        let passing = b"ok 1 - name with # SKIP inside it\n1..1\n";
        assert!(
            skip_diagnostics(passing, &declared).is_empty(),
            "a name that merely contains the directive is not a skip"
        );
        // The SAME name genuinely skipped still warns: its description is the
        // name PLUS a directive, so it matches no declared name verbatim.
        let skipped = b"ok 1 - name with # SKIP inside it # SKIP really parked\n1..1\n";
        assert_eq!(
            skip_diagnostics(skipped, &declared),
            ["warning: test 'name with # SKIP inside it' skipped: really parked"]
        );
        // With no declared names (unparsable suite) the plain split still runs,
        // so a genuine skip is never silently dropped.
        assert_eq!(
            skip_diagnostics(b"ok 1 - parked # SKIP why\n", &[]),
            ["warning: test 'parked' skipped: why"]
        );
    }
}
