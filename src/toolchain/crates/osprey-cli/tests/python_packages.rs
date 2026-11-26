//! Execute every package's independent oracle under GC and ARC. [PORT-SUITE]
//! Exact golden stdout, positive assertions, zero failures and a clean ARC exit
//! are mandatory; native TAP and port totals prove the same acceptance contract.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
#[path = "common/bounded.rs"]
mod bounded;
#[path = "common/oracle_transcript.rs"]
mod oracle_transcript;
#[path = "common/package_matrix.rs"]
mod package_matrix;
use oracle_transcript::successful_oracle;
use package_matrix::{schedule, MATRIX_WORKERS};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn suites() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(repo().join("src/osprey/packages"))
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
    for entry in std::fs::read_dir(repo().join("src/osprey/packages"))? {
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
    assert!(!registered.contains(&repo().join("src/osprey/packages/manager/tests")));
    assert!(registered.iter().all(|suite| suite
        .parent()
        .is_some_and(|package| package.join("osprey.toml").is_file())));
}

#[test]
fn the_complete_torch_suite_discharges_its_storage_handlers() {
    let assembled =
        osprey_project::load_and_assemble(&repo().join("src/osprey/packages/torch/tests"));
    assert!(assembled.is_ok(), "{assembled:#?}");
    let Ok(assembled) = assembled else { return };
    let lowered = osprey_types::lower_static_checked(&assembled.program);
    assert!(lowered.is_ok(), "source diagnostics: {lowered:#?}");
    let Ok(lowered) = lowered else { return };
    let errors = osprey_types::check_program(&lowered);
    assert!(errors.is_empty(), "residual diagnostics: {errors:#?}");
}

/// A package suite may build a large program, but never more than this.
const SUITE_BUDGET: bounded::Budget = bounded::Budget {
    memory_mb: 4096,
    deadline: Duration::from_secs(600),
};

/// The suite's package folder name, for temp file names.
fn package_name(suite: &Path) -> String {
    suite
        .parent()
        .and_then(Path::file_name)
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// How many suite runs this test binary has started.
static RUNS: AtomicUsize = AtomicUsize::new(0);

/// Runs one suite under one backend inside `budget`. Returns the
/// command line it ran and the bounded run, or why it could not start.
fn run(
    suite: &Path,
    memory: &str,
    cache: &Path,
    budget: bounded::Budget,
) -> (String, std::io::Result<bounded::Bounded>) {
    let backend = format!("--memory={memory}");
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command
        .current_dir(repo())
        .arg(suite)
        .args(["--run", "--quiet", &backend])
        .env("OSPREY_ARC_DEBUG", "1")
        .env("OSPREY_TEST_CACHE_DIR", cache);
    let line = format!("osprey {} --run --quiet {backend}", suite.display());
    // Tests in this binary run the same suite and backend at once (the budget
    // test and the matrix both take chromecast); a per-run ordinal keeps one
    // run from reading or deleting the other's transcript files.
    let stem = format!(
        "osprey_package_{}_{}_{}_{memory}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed),
        package_name(suite)
    );
    (line, bounded::run_command(command, &stem, budget))
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

fn failures(suite: &Path, memory: &str, cache: &Path, budget: bounded::Budget) -> Vec<String> {
    let name = suite.display();
    let golden = std::fs::read_to_string(suite.join("expected_output.txt")).unwrap_or_default();
    let (line, outcome) = run(suite, memory, cache, budget);
    let run = match outcome {
        Ok(run) => run,
        Err(error) => return vec![format!("{name} [{memory}] could not run `{line}`: {error}")],
    };
    let context = format!("{name} [{memory}] `{line}` ({})", run.summary());
    let mut problems = Vec::new();
    if run.code != Some(0) || run.stopped.is_some() {
        problems.push(format!("{context} exited unsuccessfully:\n{}", run.stderr));
    }
    if golden.is_empty() || run.stdout != golden {
        problems.push(format!(
            "{context} output differs from expected_output.txt:\n{}",
            run.stdout
        ));
    }
    if memory == "arc" && arc_live(&run.stderr).as_deref() != Some("0") {
        problems.push(format!("{context} did not exit leak-free:\n{}", run.stderr));
    }
    problems
}

/// The managed backends every suite runs under.
const BACKENDS: [&str; 2] = ["gc", "arc"];

/// Runs exactly `matrix_jobs`: suites in parallel, one suite's backends one
/// after the other, so a suite that binds loopback ports never races its own
/// other run for them.
fn matrix_problems(suites: &[PathBuf], cache: &Path) -> Vec<String> {
    let jobs = matrix_jobs(suites);
    let lanes: Vec<&[(&Path, &str)]> = jobs.chunk_by(|a, b| a.0 == b.0).collect();
    schedule(&lanes, MATRIX_WORKERS, |lane| {
        lane.iter()
            .flat_map(|(suite, memory)| failures(suite, memory, cache, SUITE_BUDGET))
            .collect()
    })
}

fn matrix_jobs(suites: &[PathBuf]) -> Vec<(&Path, &str)> {
    suites
        .iter()
        .map(PathBuf::as_path)
        .flat_map(|suite| BACKENDS.map(|memory| (suite, memory)))
        .collect()
}

#[test]
fn the_package_matrix_runs_all_64_suite_backend_pairs_once() {
    let suites = suites();
    let jobs = matrix_jobs(&suites);
    let unique: std::collections::BTreeSet<_> = jobs.iter().copied().collect();
    // The unfinished qwen model is out of the matrix by user decision (2026-11-06); GPT2 keeps its own tests.
    assert!(suites
        .iter()
        .all(|suite| !suite.starts_with(repo().join("src/osprey/examples"))));
    assert_eq!(jobs.len(), 64, "all 32 packages need both managed backends");
    assert_eq!(unique.len(), jobs.len(), "duplicate job");
    for suite in &suites {
        for memory in BACKENDS {
            assert!(unique.contains(&(suite.as_path(), memory)));
        }
    }
}

#[test]
fn every_ported_package_matches_its_cpython_oracle_under_every_memory_backend(
) -> std::io::Result<()> {
    let suites = suites();
    assert!(
        !suites.is_empty(),
        "no src/osprey/packages/*/tests suites were found"
    );
    let cache = package_cache()?;
    let mut problems = matrix_problems(&suites, &cache);
    if let Err(error) = std::fs::remove_dir_all(cache) {
        problems.push(format!("package cache cleanup failed: {error}"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

/// Plan 0048: a killed or over-cap suite logs its command, memory backend,
/// peak RSS and elapsed time, so the runner's own log diagnoses the crash.
#[test]
fn a_suite_killed_at_its_budget_logs_command_backend_peak_and_time() -> std::io::Result<()> {
    let suites = suites();
    let suite = suites.first().expect("a package suite");
    let cache = package_cache()?;
    let over_cap = bounded::Budget {
        memory_mb: 0,
        deadline: SUITE_BUDGET.deadline,
    };
    let too_slow = bounded::Budget {
        memory_mb: SUITE_BUDGET.memory_mb,
        deadline: Duration::ZERO,
    };
    for memory in BACKENDS {
        for (budget, limit) in [(over_cap, "budget 0 MiB"), (too_slow, "0ns)")] {
            let problems = failures(suite, memory, &cache, budget);
            let first = problems.first().expect("a killed suite is a failure");
            let command = format!(
                "`osprey {} --run --quiet --memory={memory}`",
                suite.display()
            );
            assert!(first.contains(&command), "{first}");
            assert!(first.contains(&format!("[{memory}]")), "{first}");
            assert!(first.contains("killed by a signal"), "{first}");
            assert!(first.contains(", peak "), "{first}");
            assert!(first.contains(" MiB, "), "{first}");
            assert!(first.contains("stopped at "), "{first}");
            assert!(first.contains(limit), "{first}");
            assert!(first.contains("exited unsuccessfully"), "{first}");
        }
    }
    std::fs::remove_dir_all(cache)
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

fn quoted_items(items: &str) -> Vec<String> {
    items
        .split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn manifest_list(manifest: &str, key: &str) -> Vec<String> {
    manifest
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key} = [")))
        .and_then(|rest| rest.strip_suffix(']'))
        .map_or_default(quoted_items)
}

fn exports(source: &str) -> Vec<String> {
    let Some(start) = source.find("__all__") else {
        return Vec::new();
    };
    let rest = source.get(start..).unwrap_or_default();
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let Some(close) = rest.get(open..).unwrap_or_default().find(']') else {
        return Vec::new();
    };
    quoted_items(rest.get(open + 1..open + close).unwrap_or_default())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn names_word(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = text.get(..at).unwrap_or_default().chars().next_back();
        let after = text
            .get(at + name.len()..)
            .unwrap_or_default()
            .chars()
            .next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

fn suite_text(suite: &Path) -> String {
    python_files(suite)
        .iter()
        .filter(|file| file.file_name().is_some_and(|name| name != "expected.py"))
        .map(|file| std::fs::read_to_string(file).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// [PORT-SUITE]: reject public functions absent from the suite.
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
        .flat_map(|suite| unexercised_exports(&suite.parent().map_or_default(Path::to_path_buf)))
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
    let oracle = "src/osprey/packages/tiktoken/oracle/tiktoken.cpython";
    assert!(oracle_provenance(
        "Generated by\nsrc/osprey/packages/tiktoken/oracle/tiktoken.cpython; never hand-edit",
        oracle
    ));
    assert!(!oracle_provenance(
        "Generated by src/osprey/packages/numpy/oracle/numpy.cpython",
        oracle
    ));
    assert!(!oracle_provenance(
        "Expected tiktoken values with no generator",
        oracle
    ));
}

/// [PORT-TEST-ORACLE]: generated reference provenance and an existing oracle.
fn assert_expected_is_oracle_generated(package: &Path) {
    let name = package
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let oracle = format!("src/osprey/packages/{name}/oracle/{name}.cpython");
    let expected = std::fs::read_to_string(package.join("tests/expected.py")).unwrap_or_default();
    assert!(
        oracle_provenance(&expected, &oracle),
        "{}: tests/expected.py was not written by {oracle}",
        package.display()
    );
    assert!(repo().join(&oracle).is_file(), "{oracle} is missing");
}

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

/// [PACKAGE-MANIFEST]: every Python source is rooted in the manifest.
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

/// [PACKAGE-SOURCE-CANONICAL]: distinguish Osprey sources from `CPython` helpers.
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
        let package = suite.parent().map_or_default(Path::to_path_buf);
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
    let native = repo().join("src/osprey/packages/native/tests");
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

#[test]
fn fastapi_http_entrypoints_refuse_disabled_python_assertions() -> std::io::Result<()> {
    let python = if cfg!(windows) { "python" } else { "python3" };
    for script in ["e2e.cpython", "benchmark.cpython"] {
        let output = Command::new(python)
            .args(["-O", "-c", "import runpy,sys;runpy.run_path(sys.argv[1])"])
            .arg(
                repo()
                    .join("src/osprey/packages/fastapi/tests")
                    .join(script),
            )
            .output()?;
        assert!(
            !output.status.success(),
            "{script}: disabled assertions were accepted"
        );
        assert!(
            output.stdout.is_empty(),
            "{script}: tests ran without assertions"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("enabled assertions"),
            "{script}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
fn python_export_gate_reads_both_string_delimiters() {
    for source in [
        "__all__ = ['header', 'query']",
        "__all__ = [\"header\", \"query\"]",
        "__all__ = ['header', \"query\"]",
    ] {
        assert_eq!(exports(source), vec!["header", "query"], "{source}");
    }
}
