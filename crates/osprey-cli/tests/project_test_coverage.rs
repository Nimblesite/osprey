//! End-to-end coverage of a project suite ([TESTING-PROJECT-SUITE], #291): a
//! directory holding an `osprey.toml` runs as one suite, and its coverage is
//! reported per physical source file, so lines from a sibling source root are
//! attributed to the file that wrote them rather than to the test entry.

#![expect(
    clippy::expect_used,
    reason = "fixture setup and subprocess failures must fail these tests immediately"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create project directory");
    }
    std::fs::write(path, text).expect("write project source");
}

const MANIFEST: &str = "[project]\nname = \"covered\"\nsource_roots = [\"framework/src\", \"tests\"]\ndefault_namespace = \"app\"\nentry = \"tests/main.ospml\"\n";

/// `greet` runs under the suite; `shout` never does.
const CALC: &str = r#"namespace calc

(** Text helpers the suite exercises. *)
module Words
    (** Greet someone by name. *)
    export greet : string -> string
    greet name = "hi ${name}"

    (** Shout a word; no test calls it. *)
    export shout : string -> string
    shout word = "${word}!"
"#;

const MAIN: &str = r#"namespace app

import calc::Words

test "greets" (\() => expect (Words::greet "ann", "hi ann"))
"#;

fn project(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("osprey_project_cov_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("osprey.toml"), MANIFEST);
    write(&root.join("framework/src/calc.ospml"), CALC);
    write(&root.join("tests/main.ospml"), MAIN);
    root
}

fn osprey_test(args: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo_root())
        .arg("test")
        .args(args)
        .output()
        .expect("run osprey test");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The `lines` table the report holds for the file ending in `suffix`, in
/// ascending line order.
fn file_lines(report: &serde_json::Value, suffix: &str) -> Vec<(u32, u64)> {
    let files = report["files"].as_object().expect("files object");
    let (_, entry) = files
        .iter()
        .find(|(path, _)| path.ends_with(suffix))
        .expect("the report covers the file");
    let lines = entry["lines"].as_object().expect("lines object");
    let mut rows: Vec<(u32, u64)> = lines
        .iter()
        .map(|(line, hits)| {
            let line = line.parse().expect("numeric line");
            (line, hits.as_u64().expect("numeric hits"))
        })
        .collect();
    rows.sort_unstable();
    rows
}

#[test]
fn a_project_suite_reports_coverage_for_each_source_file() {
    let root = project("per_file");
    let json = root.join("cov.json");
    let (code, stdout, stderr) = osprey_test(&[
        root.to_string_lossy().as_ref(),
        "--coverage-json",
        json.to_string_lossy().as_ref(),
    ]);
    assert_eq!(code, Some(0), "stdout={stdout}\nstderr={stderr}");
    assert!(stdout.contains("ok 1 - greets"), "{stdout}");
    assert!(stdout.contains("# suites: 1 passed, 0 failed"), "{stdout}");
    for file in ["calc.ospml", "main.ospml"] {
        assert!(
            stdout
                .lines()
                .any(|row| row.starts_with("# coverage: ") && row.ends_with(file)),
            "no per-file row for {file}: {stdout}"
        );
    }
    assert!(stdout.contains("# coverage total: "), "{stdout}");

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).expect("coverage json"))
            .expect("valid coverage json");
    // `greet` is defined on calc.ospml's own line 7 and runs once; `shout`,
    // on line 11, never runs. Flattened lines would land past the entry's.
    assert_eq!(file_lines(&report, "calc.ospml"), vec![(7, 1), (11, 0)]);
    assert_eq!(file_lines(&report, "main.ospml"), vec![(5, 1)]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_project_suite_runs_without_coverage() {
    let root = project("plain");
    let (code, stdout, stderr) = osprey_test(&[root.to_string_lossy().as_ref()]);
    assert_eq!(code, Some(0), "stdout={stdout}\nstderr={stderr}");
    assert!(
        stdout.contains(&format!("# file: {}", root.display())),
        "{stdout}"
    );
    assert!(stdout.contains("ok 1 - greets"), "{stdout}");
    let _ = std::fs::remove_dir_all(&root);
}
