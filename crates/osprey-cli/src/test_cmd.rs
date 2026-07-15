//! `osprey test` — discover and run test suites. Implements
//! [TESTING-CLI-RUN], [TESTING-FILE-CONVENTION], [TESTING-FILTER]
//! (docs/specs/0027-TestingFramework.md).
//!
//! `path` (default `.`) is a single file run as-is, or a directory searched
//! recursively for `*.test.osp` / `*.test.ospml`, sorted for determinism,
//! skipping hidden, `target`, and `node_modules` directories. Each file
//! compiles and runs like `osprey <file> --run`; its TAP output streams
//! through under a `# file:` header; the exit code aggregates suite outcomes.

use crate::{execute_native, load_input, report_type_errors, Cli, USAGE};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct Opts {
    path: String,
    filter: Option<String>,
    quiet: bool,
}

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
    run_suites(&files, opts.quiet)
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut opts = Opts {
        path: String::from("."),
        filter: None,
        quiet: false,
    };
    let mut path = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quiet" => opts.quiet = true,
            "--filter" => {
                let next = it
                    .next()
                    .ok_or_else(|| format!("--filter requires a test name\n{USAGE}"))?;
                opts.filter = Some(next.clone());
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

fn run_suites(files: &[PathBuf], quiet: bool) -> ExitCode {
    let mut failed = 0usize;
    for file in files {
        if !quiet {
            println!("# file: {}", file.display());
        }
        if !suite_passes(file) {
            failed += 1;
        }
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
fn suite_passes(file: &Path) -> bool {
    let cli = Cli::run_native(file.display().to_string());
    let Ok(input) = load_input(&cli) else {
        return false;
    };
    if report_type_errors(&input) > 0 {
        return false;
    }
    matches!(
        execute_native(&input, "default", osprey_debug::BuildKind::Release),
        Ok(0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(parse(&[]).expect("default").path, ".");
        assert!(parse(&["--filter".to_string()]).is_err());
        assert!(parse(&["--bogus".to_string()]).is_err());
        assert!(parse(&["a".to_string(), "b".to_string()]).is_err());
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
}
