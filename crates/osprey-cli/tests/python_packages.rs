//! The ported Python packages ([PORT-SUITE] in
//! docs/specs/ecosystem/0043-PythonFlavorPorts.md). Every `packages/<name>/tests`
//! Python-flavor project has assertions comparing the port with
//! values the real `CPython` package computed. It must print exactly its
//! committed `expected_output.txt` — one `ok` line per assertion and a
//! `N passed, 0 failed` total — under the default, GC and ARC memory backends,
//! and an ARC run must exit with no live objects. The native ML framework also
//! stays in this execution matrix; `CPython` provenance applies to Python ports.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every published package with a test manifest, sorted; tooling stays separate.
fn suites() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(repo().join("packages"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|package| package.join("osprey.toml").is_file())
        .map(|package| package.join("tests"))
        .filter(|tests| tests.join("osprey.toml").is_file())
        .collect();
    found.sort();
    found
}

#[test]
fn every_shipped_package_registers_an_execution_suite() -> std::io::Result<()> {
    let registered = suites();
    for entry in std::fs::read_dir(repo().join("packages"))? {
        let package = entry?.path();
        if package.join("osprey.toml").is_file() {
            assert!(
                registered.contains(&package.join("tests")),
                "{} must register tests/osprey.toml in the backend matrix",
                package.display()
            );
        }
    }
    Ok(())
}

#[test]
fn package_tooling_containers_do_not_enter_the_library_matrix() {
    let registered = suites();
    assert!(!registered.contains(&repo().join("packages/manager/tests")));
    assert!(registered.iter().all(|suite| suite
        .parent()
        .is_some_and(|package| package.join("osprey.toml").is_file())));
}

/// All existing Torch groups must discharge before the native oracle run.
#[test]
fn the_complete_torch_suite_discharges_its_storage_handlers() {
    let assembled = osprey_project::load_and_assemble(&repo().join("packages/torch/tests"));
    assert!(assembled.is_ok(), "{assembled:#?}");
    let Ok(assembled) = assembled else { return };
    let lowered = osprey_types::lower_static_checked(&assembled.program);
    assert!(lowered.is_ok(), "source diagnostics: {lowered:#?}");
    let Ok(lowered) = lowered else { return };
    let errors = osprey_types::check_program(&lowered);
    assert!(errors.is_empty(), "residual diagnostics: {errors:#?}");
}

/// `(succeeded, stdout, stderr)` of the suite run under `memory`.
fn run(suite: &Path, memory: &str, cache: &Path) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(suite)
        .args(["--run", "--quiet", &format!("--memory={memory}")])
        .env("OSPREY_ARC_DEBUG", "1")
        .env("OSPREY_TEST_CACHE_DIR", cache)
        .output();
    output.map_or_else(
        |error| (false, String::new(), error.to_string()),
        |out| {
            (
                out.status.success(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        },
    )
}

/// The live-object count of the single ARC exit sentinel, `None` when the
/// sentinel is absent or repeated: an unverified run never reads as leak-free.
fn arc_live(stderr: &str) -> Option<String> {
    let counts: Vec<&str> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("[osp-arc] exit: "))
        .filter_map(|rest| rest.split(' ').next())
        .collect();
    match counts.as_slice() {
        [count] => Some((*count).to_string()),
        _ => None,
    }
}

fn successful_oracle(output: &str) -> bool {
    let lines: Vec<_> = output.lines().collect();
    let Some((summary, outcomes)) = lines.split_last() else {
        return false;
    };
    summary
        .strip_suffix(" passed, 0 failed")
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|count| {
            count > 0
                && outcomes
                    .iter()
                    .filter(|line| line.starts_with("ok ") && line.len() > 3)
                    .count()
                    == count
                && !outcomes.iter().any(|line| line.starts_with("FAIL "))
        })
}

#[test]
fn oracle_success_requires_real_passing_assertions() {
    assert!(successful_oracle("ok one\nok two\n2 passed, 0 failed\n"));
    assert!(successful_oracle(
        "ok multiline\npattern\n1 passed, 0 failed\n"
    ));
    for bad in [
        "",
        "0 passed, 0 failed\n",
        "FAIL one\n1 passed, 0 failed\n",
        "ok one\n1 passed, 1 failed\n",
        "ok one\n2 passed, 0 failed\n",
        "ok \n1 passed, 0 failed\n",
    ] {
        assert!(!successful_oracle(bad), "accepted invalid oracle: {bad}");
    }
}

#[test]
fn every_python_oracle_golden_records_nonzero_successful_assertions() -> std::io::Result<()> {
    for suite in suites() {
        let manifest = std::fs::read_to_string(suite.join("osprey.toml"))?;
        if !manifest_list(&manifest, "python_sources").is_empty() {
            let output = std::fs::read_to_string(suite.join("expected_output.txt"))?;
            assert!(
                successful_oracle(&output),
                "invalid passing oracle: {}",
                suite.display()
            );
        }
    }
    Ok(())
}

/// The failures of one suite under one backend, as readable lines.
fn failures(suite: &Path, memory: &str, cache: &Path) -> Vec<String> {
    let name = suite.display();
    let golden = std::fs::read_to_string(suite.join("expected_output.txt")).unwrap_or_default();
    let (ok, stdout, stderr) = run(suite, memory, cache);
    let mut problems = Vec::new();
    if !ok {
        problems.push(format!(
            "{name} [{memory}] exited unsuccessfully:\n{stderr}"
        ));
    }
    if golden.is_empty() || stdout != golden {
        problems.push(format!(
            "{name} [{memory}] output differs from expected_output.txt:\n{stdout}"
        ));
    }
    if memory == "arc" && arc_live(&stderr).as_deref() != Some("0") {
        problems.push(format!("{name} [arc] did not exit leak-free:\n{stderr}"));
    }
    problems
}

#[test]
fn every_ported_package_matches_its_cpython_oracle_under_every_memory_backend(
) -> std::io::Result<()> {
    let suites = suites();
    assert!(!suites.is_empty(), "no packages/*/tests suites were found");
    let cache = package_cache()?;
    let problems: Vec<String> = suites
        .iter()
        .flat_map(|suite| {
            let cache = &cache;
            ["default", "gc", "arc"]
                .into_iter()
                .flat_map(move |memory| failures(suite, memory, cache))
        })
        .collect();
    std::fs::remove_dir_all(cache)?;
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

fn package_cache() -> std::io::Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "osprey-package-cache-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&directory)?;
    Ok(directory)
}

/// The quoted strings of a comma-separated list body.
fn quoted_items(items: &str) -> Vec<String> {
    items
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// The string items of the one-line manifest array `key = [...]`.
fn manifest_list(manifest: &str, key: &str) -> Vec<String> {
    manifest
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key} = [")))
        .and_then(|rest| rest.strip_suffix(']'))
        .map(quoted_items)
        .unwrap_or_default()
}

/// The names a module's `__all__ = [...]` exports, however many lines it spans.
fn exports(source: &str) -> Vec<String> {
    let Some(start) = source.find("__all__") else {
        return Vec::new();
    };
    let rest = &source[start..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find(']') else {
        return Vec::new();
    };
    quoted_items(&rest[open + 1..open + close])
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `name` occurs in `text` as a whole word.
fn names_word(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

/// The suite's own sources: every `tests/*.py` except the generated `expected.py`.
fn suite_text(suite: &Path) -> String {
    python_files(suite)
        .iter()
        .filter(|file| file.file_name().is_some_and(|name| name != "expected.py"))
        .map(|file| std::fs::read_to_string(file).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// [PORT-SUITE]: the `__all__` names of every module under `src/` that the
/// suite never spells, so no public function ships untested.
fn unexercised_exports(package: &Path) -> Vec<String> {
    let text = suite_text(&package.join("tests"));
    let mut found = Vec::new();
    for module in python_files(&package.join("src")) {
        let source = std::fs::read_to_string(&module).unwrap_or_default();
        for name in exports(&source) {
            if !names_word(&text, &name) {
                found.push(format!("{}: `{name}`", module.display()));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_public_export_of_every_ported_package_is_exercised_by_its_suite() {
    let problems: Vec<String> = suites()
        .iter()
        .flat_map(|suite| {
            unexercised_exports(&suite.parent().map(Path::to_path_buf).unwrap_or_default())
        })
        .collect();
    assert!(
        problems.is_empty(),
        "exports no suite exercises (test them, or drop them from `__all__`):\n{}",
        problems.join("\n")
    );
}

fn oracle_provenance(expected: &str, oracle: &str) -> bool {
    expected
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(&format!("Generated by {oracle}"))
}

#[test]
fn oracle_provenance_accepts_wrapping_but_keeps_the_generator_identity() {
    let oracle = "scripts/oracles/tiktoken.py";
    assert!(oracle_provenance(
        "Generated by\nscripts/oracles/tiktoken.py; never hand-edit",
        oracle
    ));
    assert!(!oracle_provenance(
        "Generated by scripts/oracles/numpy.py",
        oracle
    ));
    assert!(!oracle_provenance(
        "Expected tiktoken values with no generator",
        oracle
    ));
}

/// [PORT-TEST-ORACLE]: `expected.py` was written by the package's oracle
/// through the shared writer, and that oracle exists.
fn assert_expected_is_oracle_generated(package: &Path) {
    let name = package
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let oracle = format!("scripts/oracles/{name}.py");
    let expected = std::fs::read_to_string(package.join("tests/expected.py")).unwrap_or_default();
    assert!(
        oracle_provenance(&expected, &oracle),
        "{}: tests/expected.py was not written by {oracle}",
        package.display()
    );
    assert!(repo().join(&oracle).is_file(), "{oracle} is missing");
}

/// Every `.py` file beneath `directory`.
fn python_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for path in std::fs::read_dir(next)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
        {
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "py") {
                found.push(path);
            }
        }
    }
    found
}

/// [PACKAGE-MANIFEST]: with `[package]` present, every `python_sources` path
/// lies inside one of the manifest's `source_roots`.
fn assert_sources_rooted(package: &Path, manifest: &str) {
    let roots = manifest_list(manifest, "source_roots");
    for source in manifest_list(manifest, "python_sources") {
        assert!(
            roots
                .iter()
                .any(|root| source.starts_with(&format!("{root}/"))),
            "{}: python source `{source}` is outside source_roots {roots:?}",
            package.display()
        );
    }
}

/// [PACKAGE-SOURCE-CANONICAL]: the registry refuses a release holding any
/// `.py` file whose first line is not the Python-flavor marker, so `CPython`
/// code (an oracle, a tool) can never ship inside a port.
fn assert_python_files_marked(package: &Path) {
    for file in python_files(package) {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        assert_eq!(
            text.lines().next(),
            Some("# osprey: flavor=python"),
            "{} must begin with `# osprey: flavor=python`",
            file.display()
        );
    }
}

#[test]
fn every_ported_package_is_a_publishable_python_flavor_package() {
    for suite in suites() {
        let package = suite.parent().map(Path::to_path_buf).unwrap_or_default();
        let manifest = std::fs::read_to_string(package.join("osprey.toml")).unwrap_or_default();
        assert!(
            manifest.contains("[package]") && manifest.contains("name = \"@osprey/"),
            "{} must declare an @osprey [package]",
            package.display()
        );
        assert!(
            package.join("README.md").is_file(),
            "{} needs a README.md naming the ported package and its divergences",
            package.display()
        );
        assert_sources_rooted(&package, &manifest);
        assert_python_files_marked(&package);
        // [PORT-TEST-ORACLE] describes Python ports. The native ML framework
        // has its own golden, still executed above under all memory backends.
        if manifest_list(&manifest, "python_sources").is_empty() {
            assert!(
                python_files(&package).is_empty(),
                "{}: Python source needs a declared python_sources manifest",
                package.display()
            );
        } else {
            assert_expected_is_oracle_generated(&package);
        }
    }
}

#[test]
fn the_native_framework_remains_in_the_full_backend_execution_matrix() {
    let native = repo().join("packages/native/tests");
    assert!(
        suites().contains(&native),
        "native goldens must run in every backend"
    );
    let manifest =
        std::fs::read_to_string(native.join("osprey.toml")).expect("native test manifest");
    assert!(
        manifest.contains("entry = \"main.ospml\""),
        "the native framework suite is ML, not a CPython port"
    );
    assert_ne!(
        std::fs::read_to_string(native.join("expected_output.txt")).expect("native golden"),
        String::new(),
        "the native framework needs an asserted output golden"
    );
}
