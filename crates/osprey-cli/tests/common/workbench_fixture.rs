//! Staging for the real workbench `GoalCases`: a private copy of the maintained
//! `goals.ospml` with one visibility word added, beside the real fixture host.

use std::path::Path;
use std::process::Output;

/// `--quiet` keeps the printed formula, and the runner still prints its
/// zero-test plan: the assertion keeps every byte rather than the first line.
pub(crate) const VALID_STDOUT: &str =
    "p ∧ q → q ∧ p\n1..0\n# tests=0 passed=0 failed=0 skipped=0\n";

/// Stage a driver for `formula` from the maintained sources, so the regression
/// exercises the real helper rather than a reimplementation of it.
pub(crate) fn stage(root: &Path, repo_root: &Path, formula: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    copy_goal_case(root, repo_root)?;
    copy_fixture_host(root, repo_root)?;
    std::fs::write(root.join("main.ospml"), driver(formula))?;
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

/// The real `FixtureFiles` host, so no abort helper is copied.
fn copy_fixture_host(root: &Path, repo_root: &Path) -> std::io::Result<()> {
    let host = repo_root.join("apps/workbench/tests/app.ospml");
    std::fs::write(root.join("fixtures.ospml"), std::fs::read_to_string(host)?)
}

fn driver(formula: &str) -> String {
    format!("namespace goalfault\n\nimport workbench::Formulas\nimport workbenchtests::GoalCases\n\nmain () = print (Formulas::show (GoalCases::formula \"{formula}\"))\n")
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

/// The refusal on every OS: a failed status and no `⊥` on the way out.
pub(crate) fn assert_faulted(output: &Output, memory: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "{memory}: the faulted fixture still succeeded: stdout={stdout}"
    );
    assert!(
        !stdout.contains('\u{22a5}'),
        "{memory}: the faulted fixture still printed bottom: stdout={stdout}"
    );
    assert_context(&String::from_utf8_lossy(&output.stderr), memory);
    assert_aborted(output, memory);
}

/// The report names the fixture it could not read and the parser's own reason.
fn assert_context(stderr: &str, memory: &str) {
    assert!(
        stderr.contains("goal formula p \u{2227}"),
        "{memory}: the parse context is missing: stderr={stderr}"
    );
    assert!(
        stderr.contains("a proposition after"),
        "{memory}: the parser's reason is missing: stderr={stderr}"
    );
}

/// On Unix the launcher re-exits with its aborted child's code, so SIGABRT
/// arrives as 128 + 6 rather than as a signal on this status; the other OS
/// families keep the non-success and stderr assertions above.
fn assert_aborted(output: &Output, memory: &str) {
    #[cfg(unix)]
    {
        const SIGABRT: i32 = 6;
        assert_eq!(
            output.status.code(),
            Some(128 + SIGABRT),
            "{memory}: the child was not aborted: {}",
            output.status
        );
    }
    #[cfg(not(unix))]
    let _ = (output, memory);
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
    use super::manifest_text;

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
