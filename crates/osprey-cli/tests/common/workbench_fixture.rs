//! Staging for the real workbench `GoalCases`: a private copy of the maintained
//! `goals.ospml` with one visibility word added, beside the real fixture host.

use std::path::Path;
use std::process::Output;

/// `--quiet` keeps the printed formula, and the runner still prints its
/// passing case and plan: the assertion keeps every byte rather than the first line.
pub(crate) const VALID_FORMULA: &str = "p ∧ q → q ∧ p";
pub(crate) const VALID_STDOUT: &str = "p ∧ q → q ∧ p\nok 1 - goal formula\np ∧ q → q ∧ p\nok 2 - later valid formula\n1..2\n# tests=2 passed=2 failed=0 skipped=0\n";

/// Stage a driver for `formula` from the maintained sources, so the regression
/// exercises the real helper rather than a reimplementation of it.
pub(crate) fn stage(root: &Path, repo_root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    copy_goal_case(root, repo_root)?;
    copy_fixture_host(root, repo_root)?;
    std::fs::write(root.join("main.ospml"), driver())?;
    std::fs::write(root.join("osprey.toml"), manifest(repo_root)?)
}

/// The maintained `formula` line stays verbatim and only gains `export`, so the
/// driver can reach it; every other byte is the real source.
fn copy_goal_case(root: &Path, repo_root: &Path) -> std::io::Result<()> {
    const HIDDEN: &str = "    formula text = FixtureFiles::formula";
    let goals = std::fs::read_to_string(repo_root.join("apps/workbench/tests/goals.ospml"))?;
    assert_eq!(
        goals.matches(HIDDEN).count(),
        1,
        "the staged helper definition moved"
    );
    let exported = goals.replacen(HIDDEN, "    export formula text = FixtureFiles::formula", 1);
    std::fs::write(root.join("goalcase.ospml"), exported)
}

/// The real `FixtureFiles` host, including its typed test failure boundary.
fn copy_fixture_host(root: &Path, repo_root: &Path) -> std::io::Result<()> {
    let host = repo_root.join("apps/workbench/tests/app.ospml");
    std::fs::write(root.join("fixtures.ospml"), std::fs::read_to_string(host)?)
}

fn driver() -> String {
    format!(
        r#"namespace goalfault

import workbench::Formulas
import workbenchtests::GoalCases
import workbenchtests::FixtureFiles

render value = FixtureFiles::ok (print (Formulas::show value))
parse text = FixtureFiles::bind (GoalCases::formula text, render)
run () = match readFile "formula.txt"
    Success text => parse text
    Error problem => Error(message = "fixture formula.txt: ${{problem}}")
later () = parse "{VALID_FORMULA}"
test ("goal formula", FixtureFiles::checked run)
test ("later valid formula", FixtureFiles::checked later)
"#
    )
}

/// `.` reaches the staged copies; listing them again would load them twice. The
/// tests directory itself is excluded, so no second `GoalCases` can appear.
fn manifest(repo_root: &Path) -> std::io::Result<String> {
    let roots: Vec<String> = std::iter::once(".".to_string())
        .chain(support_roots(repo_root)?)
        .collect();
    manifest_text(&roots)
}

/// A staged manifest for `roots`, with the array serialized rather than hand-quoted.
///
/// Hand quoting left a Windows `C:\…` path unescaped — `\w` is not a TOML escape —
/// and would have broken on any checkout path containing a quote.
fn manifest_text(roots: &[String]) -> std::io::Result<String> {
    let listed = serde_json::to_string(roots).map_err(|error| {
        std::io::Error::other(format!("cannot serialize the source_roots array: {error}"))
    })?;
    Ok(format!(
        "[project]\nname = \"goal_fault_stage\"\nsource_roots = {listed}\ndefault_namespace = \"goalfault\"\nentry = \"main.ospml\"\n\n[modules]\nallow_wildcard_imports = false\n"
    ))
}

const FAILED_CASE: &str = "not ok 1 - goal formula";
const FAILED_SUMMARY: &str = "# tests=2 passed=1 failed=1 skipped=0";
const FAILED_TAP: &str = "not ok 1 - goal formula\np ∧ q → q ∧ p\nok 2 - later valid formula\n1..2\n# tests=2 passed=1 failed=1 skipped=0\n";

/// A fixture fault fails its active test normally, retaining the parser context.
pub(crate) fn assert_faulted(output: &Output, memory: &str) {
    assert_failed_case(output, memory);
    assert_context(&String::from_utf8_lossy(&output.stdout), memory);
}

pub(crate) fn assert_missing(output: &Output, memory: &str) {
    assert_failed_case(output, memory);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("fixture formula.txt: readFile: formula.txt:"),
        "{memory}: {output:?}"
    );
}

fn assert_failed_case(output: &Output, memory: &str) {
    assert_eq!(output.status.code(), Some(1), "{memory}: {output:?}");
    assert_failed_stdout(&String::from_utf8_lossy(&output.stdout), memory);
}

fn assert_failed_stdout(stdout: &str, memory: &str) {
    assert!(!stdout.contains('\u{22a5}'), "{memory}: fabricated bottom");
    assert!(stdout.contains(FAILED_CASE), "{memory}: {stdout}");
    assert!(stdout.contains(FAILED_SUMMARY), "{memory}: {stdout}");
    assert!(
        stdout.contains("p ∧ q → q ∧ p\nok 2 - later valid formula"),
        "{memory}: no later valid case: {stdout}"
    );
    assert_eq!(
        stdout.split_once('\n').map(|(_, tap)| tap),
        Some(FAILED_TAP),
        "{memory}: changed failed fixture's TAP"
    );
}

fn assert_context(stdout: &str, memory: &str) {
    assert!(
        stdout.contains("goal formula p \u{2227}"),
        "{memory}: {stdout}"
    );
    assert!(stdout.contains("a proposition after"), "{memory}: {stdout}");
}

fn support_roots(repo_root: &Path) -> std::io::Result<Vec<String>> {
    let tests = repo_root.join("apps/workbench/tests");
    let manifest = std::fs::read_to_string(tests.join("osprey.toml"))?;
    let listed = listed_roots(&manifest)?;
    Ok(absolute_roots(&tests, &listed))
}

/// The manifest's own roots, as written, without its `.`.
fn listed_roots(manifest: &str) -> std::io::Result<Vec<&str>> {
    let listed = manifest
        .split("source_roots = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .ok_or_else(|| std::io::Error::other("the workbench manifest declares no source_roots"))?;
    Ok(listed
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|root| *root != ".")
        .collect())
}

fn absolute_roots(tests: &Path, listed: &[&str]) -> Vec<String> {
    listed
        .iter()
        .map(|root| tests.join(root).to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{assert_failed_stdout, manifest_text, FAILED_TAP};

    #[test]
    fn failed_fixture_keeps_exact_plan_and_later_valid_case() {
        assert_failed_stdout(&format!("# check 'fixture formula.txt: readFile: formula.txt: No such file or directory' failed: expected true, got false\n{FAILED_TAP}"), "gc");
    }

    #[test]
    #[should_panic(expected = "changed failed fixture's TAP")]
    fn missing_tap_plan_is_rejected() {
        assert_failed_stdout("# failure\nnot ok 1 - goal formula\np ∧ q → q ∧ p\nok 2 - later valid formula\n# tests=2 passed=1 failed=1 skipped=0\n", "gc");
    }

    #[test]
    #[should_panic(expected = "changed failed fixture's TAP")]
    fn extra_fabricated_formula_is_rejected() {
        assert_failed_stdout("# failure\np ∧ q → q ∧ p\nnot ok 1 - goal formula\np ∧ q → q ∧ p\nok 2 - later valid formula\n1..2\n# tests=2 passed=1 failed=1 skipped=0\n", "arc");
    }

    /// Roots a Windows checkout, or a quote-containing directory, actually produces.
    const HOSTILE: [&str; 4] = [
        r"C:\work\osprey\examples\proof",
        r#"C:\a b\c\"d\e"#,
        "/posix/with\"quote",
        r"back\slash",
    ];

    #[test]
    fn windows_and_quoted_roots_survive_manifest_parsing() -> std::io::Result<()> {
        let roots: Vec<String> = HOSTILE.iter().map(|root| (*root).to_string()).collect();
        let text = manifest_text(&roots)?;
        let parsed =
            osprey_project::ProjectConfig::parse(&text, std::path::Path::new("stage/osprey.toml"))
                .map_err(|errors| std::io::Error::other(format!("{errors:?}")))?;
        let measured: Vec<String> = parsed
            .source_roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        assert_eq!(measured, roots, "every declared root round-trips verbatim");
        Ok(())
    }
}
