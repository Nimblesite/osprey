//! `osprey test` — discover and run test suites. Implements
//! [TESTING-CLI-RUN], [TESTING-FILE-CONVENTION], [TESTING-FILTER],
//! [TESTING-COVERAGE-CLI] (docs/specs/0027-TestingFramework.md).
//!
//! `path` (default `.`) is a single file run as-is, or a directory searched
//! recursively for `*.test.osp` / `*.test.ospml`, sorted for determinism,
//! skipping hidden, `target`, and `node_modules` directories. Each file
//! compiles and runs like `osprey <file> --run`; its TAP output streams
//! through under a `# file:` header; the exit code aggregates suite outcomes.
//! `--coverage` instruments each suite and reports per-file and total line
//! coverage; `--coverage-json <path>` also writes the merged hit counts.

use crate::{execute_native, load_input, report_type_errors, Cli, USAGE};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
    if let Some(filter) = &opts.filter {
        // The compiled test binaries inherit the environment; the filter is
        // an exact-name match applied by the C runtime [TESTING-FILTER].
        std::env::set_var("OSPREY_TEST_FILTER", filter);
    }
    let files = discover(Path::new(&opts.path));
    if files.is_empty() {
        eprintln!("osprey test: no test files found under {}", opts.path);
        return ExitCode::FAILURE;
    }
    run_suites(&files, &opts)
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
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".test.osp") || name.ends_with(".test.ospml"))
}

fn skipped_dir_entry(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') || name == "target" || name == "node_modules")
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

fn run_suites(files: &[PathBuf], opts: &Opts) -> ExitCode {
    let mut failed = 0usize;
    let mut report = BTreeMap::new();
    for file in files {
        if !opts.quiet {
            println!("# file: {}", file.display());
        }
        let hits = opts.coverage.then(|| coverage_dump_path(file));
        if !suite_passes(file, hits.as_deref()) {
            failed += 1;
        }
        if let Some(dump) = hits {
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

/// Compile and execute one test file like `--run`; `true` when it exits 0
/// [TESTING-EXIT]. Compile and type errors print their own diagnostics.
/// `coverage_dump` (when set) instruments the build and points the runtime's
/// exit-time dump at it [TESTING-COVERAGE-CLI].
fn suite_passes(file: &Path, coverage_dump: Option<&Path>) -> bool {
    let cli = Cli::run_native(file.display().to_string());
    let Ok(input) = load_input(&cli) else {
        return false;
    };
    if report_type_errors(&input) > 0 {
        return false;
    }
    let kind = match coverage_dump {
        Some(dump) => {
            std::env::set_var("OSPREY_COVERAGE", dump);
            osprey_debug::BuildKind::Coverage
        }
        None => osprey_debug::BuildKind::Release,
    };
    matches!(execute_native(&input, "default", kind), Ok(0))
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
    fn only_test_suffixed_files_are_recognized() {
        assert!(is_test_file(Path::new("money.test.osp")));
        assert!(is_test_file(Path::new("json.test.ospml")));
        assert!(!is_test_file(Path::new("money.osp")));
        assert!(!is_test_file(Path::new("notes.txt")));
    }

    #[test]
    fn hidden_target_and_node_modules_directories_are_skipped() {
        assert!(skipped_dir_entry(Path::new("proj/.git")));
        assert!(skipped_dir_entry(Path::new("proj/target")));
        assert!(skipped_dir_entry(Path::new("proj/node_modules")));
        assert!(!skipped_dir_entry(Path::new("proj/src")));
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
