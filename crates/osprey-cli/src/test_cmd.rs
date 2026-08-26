//! `osprey test` — discover and run test suites. Implements
//! [TESTING-CLI-RUN], [TESTING-FILE-CONVENTION], [TESTING-FILTER],
//! [TESTING-COVERAGE-CLI], [TESTING-PARALLEL], [TESTING-NATIVE-CACHE]
//! (docs/specs/0027-TestingFramework.md).
//!
//! `path` (default `.`) is a single file run as-is, or a directory searched
//! recursively for `*.test.osp` / `*.test.ospml`, sorted for determinism,
//! skipping hidden, `target`, and `node_modules` directories. Each file
//! compiles and runs like `osprey <file> --run`; suites execute concurrently,
//! then their TAP output is replayed in sorted file order under `# file:`
//! headers. The exit code aggregates suite outcomes.
//! `--coverage` instruments each suite and reports per-file and total line
//! coverage; `--coverage-json <path>` also writes the merged hit counts.
//! `--memory=default|gc|arc` selects one backend for every discovered suite;
//! when omitted, child compiler invocations use the compiler's default.
//! Unchanged native suites reuse content-addressed executables across runs.

use crate::{TEST_CACHE_DIR_ENV, TEST_COVERAGE_BUILD_ENV, USAGE};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const TEST_JOBS_ENV: &str = "OSPREY_TEST_JOBS";
const MIN_DEFAULT_JOBS: usize = 2;
const TEST_CACHE_DIR: &str = "osprey-test-cache-v1";

struct Opts {
    path: String,
    filter: Option<String>,
    quiet: bool,
    coverage: bool,
    coverage_json: Option<String>,
    memory: Option<String>,
}

use crate::test_coverage::{
    collect_suite_coverage, coverage_dump_path, report_total, write_coverage_json,
};
use crate::test_skips::{declared_test_names, skip_diagnostics, SKIP_ERROR};

pub(crate) fn run(args: &[String]) -> ExitCode {
    let opts = match parse(args) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    let files = discover(Path::new(&opts.path));
    if files.is_empty() {
        eprintln!("osprey test: no test files found under {}", opts.path);
        return ExitCode::FAILURE;
    }
    let jobs = match configured_jobs(files.len()) {
        Ok(jobs) => jobs,
        Err(message) => {
            eprintln!("osprey test: {message}");
            return ExitCode::from(2);
        }
    };
    run_suites(&files, &opts, jobs)
}

fn configured_jobs(suite_count: usize) -> Result<usize, String> {
    let jobs = match std::env::var(TEST_JOBS_ENV) {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|jobs| *jobs > 0)
            .ok_or_else(|| format!("{TEST_JOBS_ENV} must be a positive integer"))?,
        Err(std::env::VarError::NotPresent) => std::thread::available_parallelism()
            .map_or(MIN_DEFAULT_JOBS, usize::from)
            .max(MIN_DEFAULT_JOBS),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(format!("{TEST_JOBS_ENV} must be valid UTF-8"));
        }
    };
    Ok(jobs.min(suite_count).max(1))
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut opts = Opts {
        path: String::from("."),
        filter: None,
        quiet: false,
        coverage: false,
        coverage_json: None,
        memory: None,
    };
    let mut path = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quiet" => opts.quiet = true,
            "--coverage" => opts.coverage = true,
            "--filter" => {
                let next = it
                    .next()
                    .ok_or_else(|| format!("--filter requires a test name\n{USAGE}"))?;
                opts.filter = Some(next.clone());
            }
            "--coverage-json" => {
                let next = it
                    .next()
                    .ok_or_else(|| format!("--coverage-json requires a path\n{USAGE}"))?;
                opts.coverage = true;
                opts.coverage_json = Some(next.clone());
            }
            flag if flag.starts_with("--memory=") => {
                opts.memory = Some(crate::parse_memory(
                    flag.strip_prefix("--memory=").unwrap_or_default(),
                )?);
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown flag {flag}\n{USAGE}"));
            }
            _ if path.is_none() => path = Some(a.clone()),
            other => return Err(format!("unexpected argument {other}\n{USAGE}")),
        }
    }
    if let Some(p) = path {
        opts.path = p;
    }
    Ok(opts)
}

/// A single file runs as-is regardless of naming; a directory is searched for
/// `[TESTING-FILE-CONVENTION]` files, sorted for a deterministic run order.
fn discover(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut out = Vec::new();
    visit(path, &mut out);
    out.sort();
    out
}

fn is_test_file(path: &Path) -> bool {
    matches_file_name(path, |name| {
        name.ends_with(".test.osp") || name.ends_with(".test.ospml")
    })
}

fn skipped_dir_entry(path: &Path) -> bool {
    matches_file_name(path, |name| {
        name.starts_with('.') || name == "target" || name == "node_modules"
    })
}

fn matches_file_name(path: &Path, predicate: impl FnOnce(&str) -> bool) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(predicate)
}

fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `file_type()` does not follow symlinks, so a symlinked directory is
        // never descended into — a link cycle cannot recurse forever.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if skipped_dir_entry(&path) {
            continue;
        }
        if file_type.is_dir() {
            visit(&path, out);
        } else if file_type.is_file() && is_test_file(&path) {
            out.push(path);
        }
    }
}

struct SuiteOutput {
    index: usize,
    passed: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_suites(files: &[PathBuf], opts: &Opts, jobs: usize) -> ExitCode {
    let outputs = execute_suites(files, opts, jobs);
    let mut failed = 0usize;
    let mut coverage_broken = false;
    let mut report = BTreeMap::new();
    for output in outputs {
        let Some(file) = files.get(output.index) else {
            failed += 1;
            continue;
        };
        let unexplained = replay_suite(file, &output, opts.quiet);
        if !output.passed || unexplained {
            failed += 1;
        }
        if opts.coverage {
            let dump = coverage_dump_path(file);
            let collected = collect_suite_coverage(file, &dump, &mut report, opts.quiet);
            // Only a suite that PASSED is expected to have left evidence. A
            // failed one may have died before writing — it is reported either
            // way, but it already counted as a failure and must not count twice.
            if !collected && output.passed {
                coverage_broken = true;
            }
        }
    }
    if opts.coverage {
        report_total(&report);
    }
    if let Some(out) = &opts.coverage_json {
        // An explicitly requested artifact that was not produced is a failure:
        // a green exit code otherwise claims a file the caller cannot read.
        if !write_coverage_json(out, &report) {
            coverage_broken = true;
        }
    }
    println!(
        "# suites: {} passed, {} failed",
        files.len() - failed,
        failed
    );
    // Counted apart from `failed`: a coverage problem is not a suite verdict,
    // so folding it in would misreport the `# suites:` line. It still fails the
    // COMMAND — absent evidence must never exit green ([TESTING-COVERAGE]).
    if failed > 0 || coverage_broken {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn execute_suites(files: &[PathBuf], opts: &Opts, jobs: usize) -> Vec<SuiteOutput> {
    // [TESTING-PARALLEL] bounded workers capture independently, then sort.
    let next = AtomicUsize::new(0);
    let completed = Mutex::new(Vec::with_capacity(files.len()));
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let _ = scope.spawn(|| suite_worker(files, opts, &next, &completed));
        }
    });
    let mut outputs = match completed.into_inner() {
        Ok(outputs) => outputs,
        Err(poisoned) => poisoned.into_inner(),
    };
    outputs.sort_by_key(|output| output.index);
    outputs
}

fn suite_worker(
    files: &[PathBuf],
    opts: &Opts,
    next: &AtomicUsize,
    completed: &Mutex<Vec<SuiteOutput>>,
) {
    loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(file) = files.get(index) else {
            return;
        };
        let output = execute_suite(file, opts, index);
        match completed.lock() {
            Ok(mut outputs) => outputs.push(output),
            Err(poisoned) => poisoned.into_inner().push(output),
        }
    }
}

fn execute_suite(file: &Path, opts: &Opts, index: usize) -> SuiteOutput {
    let dump = opts.coverage.then(|| coverage_dump_path(file));
    // The dump path is derived from the suite path, so it is stable across
    // runs. Remove any leftover before the process starts: without this an
    // artifact from an interrupted earlier run is read back as if it were this
    // run's evidence ([TESTING-COVERAGE-DUMP]).
    if let Some(stale) = dump.as_deref() {
        let _ = std::fs::remove_file(stale);
    }
    let result = suite_command(
        file,
        opts.filter.as_deref(),
        dump.as_deref(),
        opts.memory.as_deref(),
    )
    .and_then(|mut command| command.output());
    match result {
        Ok(output) => SuiteOutput {
            index,
            passed: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        },
        Err(error) => SuiteOutput {
            index,
            passed: false,
            stdout: Vec::new(),
            stderr: format!("osprey test: cannot run {}: {error}\n", file.display()).into_bytes(),
        },
    }
}

fn suite_command(
    file: &Path,
    filter: Option<&str>,
    dump: Option<&Path>,
    memory: Option<&str>,
) -> std::io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    let cache_dir = std::env::var_os(TEST_CACHE_DIR_ENV)
        .filter(|path| !path.is_empty())
        .map_or_else(|| std::env::temp_dir().join(TEST_CACHE_DIR), PathBuf::from);
    let _ = command
        .arg(file)
        .args(["--run", "--quiet"])
        .env(TEST_CACHE_DIR_ENV, cache_dir);
    if let Some(memory) = memory {
        let _ = command.arg(format!("--memory={memory}"));
    }
    close_stdin(&mut command);
    if let Some(value) = filter {
        let _ = command.env("OSPREY_TEST_FILTER", value);
    }
    match dump {
        Some(path) => {
            let _ = command
                .env(TEST_COVERAGE_BUILD_ENV, "1")
                .env("OSPREY_COVERAGE", path);
        }
        None => {
            let _ = command
                .env_remove(TEST_COVERAGE_BUILD_ENV)
                .env_remove("OSPREY_COVERAGE");
        }
    }
    Ok(command)
}

fn close_stdin(command: &mut Command) {
    let _ = command.stdin(Stdio::null());
}

/// Replay one suite's captured output, then report whether any of its skips
/// was unexplained — an error the runner counts as a suite failure
/// ([TESTING-SKIP-REASON]).
fn replay_suite(file: &Path, output: &SuiteOutput, quiet: bool) -> bool {
    let mut stdout = std::io::stdout().lock();
    if !quiet {
        let _ = writeln!(stdout, "# file: {}", file.display());
    }
    let _ = stdout.write_all(&output.stdout);
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(&output.stderr);
    let reported = skip_diagnostics(&output.stdout, &declared_test_names(file));
    for line in &reported {
        let _ = writeln!(stderr, "{line}");
    }
    reported.iter().any(|line| line.starts_with(SKIP_ERROR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn suite_command_closes_stdin_so_input_cannot_hang_on_a_terminal() {
        let mut command = Command::new("sh");
        let _ = command.args(["-c", "read line"]);
        close_stdin(&mut command);

        let output = command.output().expect("stdin probe");
        assert!(
            !output.status.success(),
            "suite children must receive EOF rather than reading terminal input"
        );
    }

    #[test]
    fn parse_reads_path_filter_and_quiet_and_rejects_junk() {
        let ok = parse(&[
            "suite".to_string(),
            "--filter".to_string(),
            "adds".to_string(),
            "--quiet".to_string(),
        ])
        .expect("valid opts");
        assert_eq!(ok.path, "suite");
        assert_eq!(ok.filter.as_deref(), Some("adds"));
        assert!(ok.quiet);
        assert!(!ok.coverage);
        assert!(ok.memory.is_none());

        assert_eq!(parse(&[]).expect("default").path, ".");
        assert!(parse(&["--filter".to_string()]).is_err());
        assert!(parse(&["--bogus".to_string()]).is_err());
        assert!(parse(&["a".to_string(), "b".to_string()]).is_err());
    }

    #[test]
    fn memory_backend_is_single_mode_and_optional() {
        let default = parse(&[]).expect("compiler default");
        assert!(default.memory.is_none());

        for memory in ["default", "gc", "arc"] {
            let opts = parse(&[format!("--memory={memory}")]).expect("valid memory backend");
            assert_eq!(opts.memory.as_deref(), Some(memory));
        }
        assert!(parse(&["--memory=bogus".to_string()]).is_err());

        let default_command = suite_command(Path::new("suite.test.osp"), None, None, None)
            .expect("default suite command");
        assert!(!default_command
            .get_args()
            .any(|arg| arg.to_string_lossy().starts_with("--memory=")));

        let gc_command = suite_command(Path::new("suite.test.osp"), None, None, Some("gc"))
            .expect("GC suite command");
        assert!(gc_command
            .get_args()
            .any(|arg| arg == std::ffi::OsStr::new("--memory=gc")));
    }

    // [TESTING-COVERAGE-CLI]: --coverage turns instrumentation on;
    // --coverage-json implies it and captures the output path.
    #[test]
    fn parse_reads_coverage_flags() {
        let plain = parse(&["--coverage".to_string()]).expect("coverage");
        assert!(plain.coverage);
        assert!(plain.coverage_json.is_none());

        let json =
            parse(&["--coverage-json".to_string(), "cov.json".to_string()]).expect("coverage json");
        assert!(json.coverage);
        assert_eq!(json.coverage_json.as_deref(), Some("cov.json"));
        assert!(parse(&["--coverage-json".to_string()]).is_err());
    }

    #[test]
    fn test_paths_are_classified() {
        // [TESTING-FILE-CONVENTION] both flavor suffixes are exact.
        let file_rule: fn(&Path) -> bool = is_test_file;
        let skip_rule: fn(&Path) -> bool = skipped_dir_entry;
        let cases = [
            (file_rule, "money.test.osp", true),
            (file_rule, "json.test.ospml", true),
            (file_rule, "money.osp", false),
            (file_rule, "notes.txt", false),
            (skip_rule, "proj/.git", true),
            (skip_rule, "proj/target", true),
            (skip_rule, "proj/node_modules", true),
            (skip_rule, "proj/src", false),
        ];
        for (rule, path, expected) in cases {
            assert_eq!(rule(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn discover_returns_a_single_file_as_is_and_walks_directories_sorted() {
        let root = std::env::temp_dir().join(format!("osprey-test-cmd-{}", std::process::id()));
        let nested = root.join("nested");
        let skipped = root.join("target");
        std::fs::create_dir_all(&nested).expect("mkdir nested");
        std::fs::create_dir_all(&skipped).expect("mkdir target");
        std::fs::write(root.join("b.test.osp"), "").expect("write b");
        std::fs::write(nested.join("a.test.ospml"), "").expect("write a");
        std::fs::write(root.join("ignore.osp"), "").expect("write ignore");
        std::fs::write(skipped.join("c.test.osp"), "").expect("write skipped");

        // A single file is returned verbatim, no matter its name.
        let plain = root.join("ignore.osp");
        assert_eq!(discover(&plain), vec![plain.clone()]);

        // A directory walk finds only *.test.* files, skips target/, and sorts.
        let found = discover(&root);
        assert_eq!(
            found,
            vec![root.join("b.test.osp"), nested.join("a.test.ospml")]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        );

        // An unreadable directory yields nothing rather than panicking.
        assert!(discover(&root.join("does-not-exist")).is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }
}
