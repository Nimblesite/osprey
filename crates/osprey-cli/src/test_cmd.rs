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
//! Unchanged native suites reuse content-addressed executables across runs.

use crate::{TEST_CACHE_DIR_ENV, TEST_COVERAGE_BUILD_ENV, USAGE};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
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
}

/// One suite's parsed coverage dump: flattened line → hit count
/// [TESTING-COVERAGE-DUMP].
type LineHits = BTreeMap<u32, u64>;

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
    let mut report = BTreeMap::new();
    for output in outputs {
        let Some(file) = files.get(output.index) else {
            failed += 1;
            continue;
        };
        replay_suite(file, &output, opts.quiet);
        if !output.passed {
            failed += 1;
        }
        if opts.coverage {
            let dump = coverage_dump_path(file);
            collect_suite_coverage(file, &dump, &mut report, opts.quiet);
        }
    }
    if opts.coverage {
        report_total(&report);
    }
    if let Some(out) = &opts.coverage_json {
        write_coverage_json(out, &report);
    }
    println!(
        "# suites: {} passed, {} failed",
        files.len() - failed,
        failed
    );
    if failed > 0 {
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
    let result = suite_command(file, opts.filter.as_deref(), dump.as_deref())
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
) -> std::io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    let cache_dir = std::env::var_os(TEST_CACHE_DIR_ENV)
        .filter(|path| !path.is_empty())
        .map_or_else(|| std::env::temp_dir().join(TEST_CACHE_DIR), PathBuf::from);
    let _ = command
        .arg(file)
        .args(["--run", "--quiet"])
        .env(TEST_CACHE_DIR_ENV, cache_dir);
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

fn replay_suite(file: &Path, output: &SuiteOutput, quiet: bool) {
    let mut stdout = std::io::stdout().lock();
    if !quiet {
        let _ = writeln!(stdout, "# file: {}", file.display());
    }
    let _ = stdout.write_all(&output.stdout);
    let _ = std::io::stderr().lock().write_all(&output.stderr);
}

/// Where one suite's coverage dump lands (the scratch dir the compiled
/// binaries already use).
fn coverage_dump_path(file: &Path) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}.oscov.txt",
        crate::scratch_stem(&file.display().to_string())
    ))
}

/// Parse one suite's dump into the merged report and print its line rate.
fn collect_suite_coverage(
    file: &Path,
    dump: &Path,
    report: &mut BTreeMap<String, LineHits>,
    quiet: bool,
) {
    let Some(hits) = parse_dump(dump) else {
        eprintln!("osprey test: no coverage dump for {}", file.display());
        return;
    };
    let _ = std::fs::remove_file(dump);
    let (covered, total) = line_rate(&hits);
    if !quiet {
        println!(
            "# coverage: {} ({covered}/{total} lines) {}",
            percent(covered, total),
            file.display()
        );
    }
    let _ = report.insert(file.display().to_string(), hits);
}

/// Read a `[TESTING-COVERAGE-DUMP]` file: `# osprey-coverage v1` then one
/// `<line> <hits>` row per coverable line. `None` when missing/unreadable.
fn parse_dump(path: &Path) -> Option<LineHits> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next() != Some("# osprey-coverage v1") {
        return None;
    }
    let mut hits = LineHits::new();
    for row in lines {
        let mut cols = row.split_whitespace();
        if let (Some(line), Some(count)) = (
            cols.next().and_then(|c| c.parse().ok()),
            cols.next().and_then(|c| c.parse().ok()),
        ) {
            let _ = hits.insert(line, count);
        }
    }
    Some(hits)
}

fn line_rate(hits: &LineHits) -> (usize, usize) {
    (hits.values().filter(|h| **h > 0).count(), hits.len())
}

fn percent(covered: usize, total: usize) -> String {
    if total == 0 {
        return String::from("100.0%");
    }
    // Line counts fit u32 comfortably; saturate rather than misconvert.
    let as_f64 = |n: usize| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    let pct = as_f64(covered) / as_f64(total) * 100.0;
    format!("{pct:.1}%")
}

/// Print the aggregate `# coverage total:` row across every suite
/// [TESTING-COVERAGE-CLI].
fn report_total(report: &BTreeMap<String, LineHits>) {
    let (covered, total) = report
        .values()
        .map(line_rate)
        .fold((0, 0), |(c, t), (sc, st)| (c + sc, t + st));
    println!(
        "# coverage total: {} ({covered}/{total} lines)",
        percent(covered, total)
    );
}

/// Write the merged machine-readable report the editor integration consumes
/// [TESTING-COVERAGE-JSON]: `{"files":{"<path>":{"lines":{"<line>":hits}}}}`.
fn write_coverage_json(out: &str, report: &BTreeMap<String, LineHits>) {
    let files = report
        .iter()
        .map(|(file, hits)| {
            let lines = hits
                .iter()
                .map(|(line, count)| format!("\"{line}\":{count}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}:{{\"lines\":{{{lines}}}}}", json_string(file))
        })
        .collect::<Vec<_>>()
        .join(",");
    if let Err(e) = std::fs::write(out, format!("{{\"files\":{{{files}}}}}")) {
        eprintln!("osprey test: cannot write coverage json {out}: {e}");
    }
}

/// Minimal JSON string encoding for a path (quotes and backslashes only —
/// paths never contain control characters the discovery walk would produce).
fn json_string(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_dump_path_is_stable_and_scratch_scoped() {
        let file = Path::new("/proj/tests/math_test.osp");
        let dump = coverage_dump_path(file);
        assert!(dump.starts_with(std::env::temp_dir()));
        let name = dump.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(name.ends_with(".oscov.txt"), "dump name: {name}");
        // Same suite → same dump path (the run and the collector must agree).
        assert_eq!(dump, coverage_dump_path(file));
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

        assert_eq!(parse(&[]).expect("default").path, ".");
        assert!(parse(&["--filter".to_string()]).is_err());
        assert!(parse(&["--bogus".to_string()]).is_err());
        assert!(parse(&["a".to_string(), "b".to_string()]).is_err());
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

    // [TESTING-COVERAGE-DUMP] parsing, rates, and the JSON shape the editor
    // integration reads [TESTING-COVERAGE-JSON].
    #[test]
    fn dump_parsing_rates_and_json_round_trip() {
        let dir = std::env::temp_dir().join(format!("osprey-cov-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let dump = dir.join("suite.oscov.txt");
        std::fs::write(&dump, "# osprey-coverage v1\n3 2\n7 0\n12 1\n").expect("write dump");
        let hits = parse_dump(&dump).expect("parse");
        assert_eq!(line_rate(&hits), (2, 3));
        assert_eq!(percent(2, 3), "66.7%");
        assert_eq!(percent(0, 0), "100.0%");

        // A dump without the v1 header is rejected, not misread.
        std::fs::write(&dump, "3 2\n").expect("rewrite");
        assert!(parse_dump(&dump).is_none());

        let mut report = BTreeMap::new();
        let _ = report.insert(String::from("a\"b.test.osp"), hits);
        let json = dir.join("cov.json");
        write_coverage_json(&json.display().to_string(), &report);
        let text = std::fs::read_to_string(&json).expect("read json");
        assert_eq!(
            text,
            "{\"files\":{\"a\\\"b.test.osp\":{\"lines\":{\"3\":2,\"7\":0,\"12\":1}}}}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
