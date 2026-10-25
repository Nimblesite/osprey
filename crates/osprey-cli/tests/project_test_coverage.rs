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

#[test]
fn folded_module_alias_initialization_remains_in_source_coverage(
) -> Result<(), Box<dyn std::error::Error>> {
    assert_alias_coverage("alias_top_level", &MAIN.replace("Words", "Facade"))?;
    assert_alias_coverage(
        "alias_main",
        "namespace app\nimport calc::Facade\nmain () = test \"greets\" (\\() => expect (Facade::greet \"ann\", \"hi ann\"))\n",
    )
}

fn assert_alias_coverage(name: &str, entry: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = project(name);
    write(
        &root.join("framework/src/facade.ospml"),
        "namespace calc\nimport calc::Words\nmodule Facade\n    export greet = Words::greet\n",
    );
    write(&root.join("tests/main.ospml"), entry);
    let json = root.join("cov.json");
    let (code, stdout, stderr) = osprey_test(&[
        root.to_string_lossy().as_ref(),
        "--coverage-json",
        json.to_string_lossy().as_ref(),
    ]);
    assert_eq!(code, Some(0), "stdout={stdout}\nstderr={stderr}");
    let report = serde_json::from_slice(&std::fs::read(&json)?)?;
    assert_eq!(file_lines(&report, "facade.ospml"), vec![(4, 1)]);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn cached_command(root: &Path, memory: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command
        .current_dir(repo_root())
        .arg("test")
        .arg(root)
        .arg("--coverage-json")
        .arg(root.join("cov.json"))
        .arg(format!("--memory={memory}"))
        .env("OSPREY_TEST_CACHE_DIR", root.join("cache"))
        .env("OSPREY_ARC_DEBUG", "1");
    command
}

fn cached_run(root: &Path, memory: &str) -> std::io::Result<std::process::Output> {
    cached_command(root, memory).output()
}

fn assert_arc_exit(stderr: &[u8]) {
    let stderr = String::from_utf8_lossy(stderr);
    let sentinels: Vec<_> = stderr
        .lines()
        .filter(|line| line.starts_with("[osp-arc] exit:"))
        .collect();
    assert_eq!(sentinels.len(), 1, "{stderr}");
    assert!(
        sentinels
            .iter()
            .all(|line| line.starts_with("[osp-arc] exit: 0 live objects,")),
        "{stderr}"
    );
}

fn cached_coverage(root: &Path, memory: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = cached_run(root, memory)?;
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("# suites: 1 passed, 0 failed"));
    let report = serde_json::from_slice(&std::fs::read(root.join("cov.json"))?)?;
    assert_eq!(file_lines(&report, "calc.ospml"), vec![(7, 1), (11, 0)]);
    if memory == "arc" {
        assert_arc_exit(&output.stderr);
    }
    Ok(())
}

fn artifact_counts(root: &Path) -> Result<[usize; 3], std::io::Error> {
    let artifacts: Vec<_> = std::fs::read_dir(root.join("cache"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    Ok(["ll", "o", "out"].map(|suffix| {
        artifacts
            .iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == suffix))
            .count()
    }))
}

#[test]
fn allocator_runs_share_codegen_but_link_and_execute_each_runtime(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = project("shared_codegen");
    for memory in ["default", "gc", "arc"] {
        cached_coverage(&root, memory)?;
    }
    assert_eq!(
        artifact_counts(&root)?,
        [1, 1, 3],
        "one IR/object, three separately linked executables"
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn shared_codegen_cannot_replay_a_stale_success_after_source_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = project("shared_codegen_invalidation");
    cached_coverage(&root, "default")?;
    write(
        &root.join("framework/src/calc.ospml"),
        &CALC.replace("hi ${name}", "bye ${name}"),
    );
    let failed = cached_run(&root, "default")?;
    assert!(!failed.status.success(), "stale success: {failed:?}");
    assert!(
        String::from_utf8_lossy(&failed.stdout).contains("not ok 1 - greets"),
        "{failed:?}"
    );
    assert_eq!(artifact_counts(&root)?, [2, 2, 2]);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn static_arm_project() -> PathBuf {
    let root = project("static_arm_hits");
    let _ = std::fs::remove_file(root.join("framework/src/calc.ospml"));
    write(
        &root.join("framework/src/calc.osp"),
        "namespace calc;\nstatic effect Pick {\n    get: fn() -> int\n    unused: fn() -> int\n}\nmodule Words {\n    export fn greet(name) = {\n        handle static Pick {\n            get => 7\n            unused => 9\n        }\n        let chosen = perform Pick.get()\n        if chosen == 7 { \"hi ${name}\" } else { \"wrong\" }\n    }\n}\n",
    );
    root
}

#[test]
fn static_handler_arms_record_execution_and_keep_unused_arms_uncovered(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = static_arm_project();
    for memory in ["default", "gc", "arc"] {
        let output = cached_run(&root, memory)?;
        assert!(output.status.success(), "{output:?}");
        let report = serde_json::from_slice(&std::fs::read(root.join("cov.json"))?)?;
        let lines = file_lines(&report, "calc.osp");
        assert_eq!(lines.iter().find(|(line, _)| *line == 9), Some(&(9, 1)));
        assert_eq!(lines.iter().find(|(line, _)| *line == 10), Some(&(10, 0)));
        if memory == "arc" {
            assert_arc_exit(&output.stderr);
        }
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
fn install_driver(root: &Path, generation: usize) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("cache-cc");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nexec clang \"$@\"\n# {}\n",
            "x".repeat(generation)
        ),
    )?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn run_driver(root: &Path) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(root.to_path_buf()).chain(std::env::split_paths(&search)),
    )?;
    Ok(cached_command(root, "default")
        .env("PATH", path)
        .env("OSPREY_CC", "cache-cc")
        .output()?)
}

#[cfg(unix)]
#[test]
fn a_compiler_resolved_from_path_invalidates_all_cached_artifacts_when_changed(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = project("compiler_identity");
    for generation in [1, 2] {
        install_driver(&root, generation)?;
        let output = run_driver(&root)?;
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            artifact_counts(&root)?,
            [generation; 3],
            "changed compiler must invalidate every stage"
        );
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn static_handler_project() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = project("static_handler_coverage");
    std::fs::remove_file(root.join("framework/src/calc.ospml"))?;
    std::fs::remove_file(root.join("tests/main.ospml"))?;
    write(&root.join("osprey.toml"), "[project]\nname = \"static-covered\"\nsource_roots = [\"framework/src\", \"tests\"]\npython_sources = [\"framework/src/caps.py\", \"framework/src/reader.py\", \"tests/main.py\"]\ndefault_namespace = \"fixture\"\nentry = \"tests/main.py\"\n");
    write(&root.join("framework/src/caps.py"), "# osprey: flavor=python\n# osprey: namespace=fixture\n# osprey: module=Caps\n__all__ = [\"Read\"]\n@static_effect\nclass Read:\n    def value() -> int: ...\n    def unused() -> int: ...\n");
    write(&root.join("framework/src/reader.py"), "# osprey: flavor=python\n# osprey: namespace=fixture\n# osprey: module=Reader\nfrom fixture.Caps import Read\n__all__ = [\"answer\"]\n\n@static_handler(Read)\nclass answer:\n    def value():\n        return 7\n    def unused():\n        return 99\n");
    write(&root.join("tests/main.py"), "# osprey: flavor=python\nfrom fixture import Reader\nfrom fixture.Caps import Read\ndef main():\n    print(Reader.answer(lambda: perform(Read.value())))\n");
    Ok(root)
}

#[test]
fn static_handler_arm_coverage_records_execution_and_retains_unused_arms(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = static_handler_project()?;
    let output = cached_run(&root, "default")?;
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line == "7"),
        "{output:?}"
    );
    let report = serde_json::from_slice(&std::fs::read(root.join("cov.json"))?)?;
    let lines = file_lines(&report, "reader.py");
    for row in [(8, 1), (9, 1), (11, 0)] {
        assert!(
            lines.contains(&row),
            "missing or incorrect handler coverage {row:?}: {lines:?}"
        );
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}
