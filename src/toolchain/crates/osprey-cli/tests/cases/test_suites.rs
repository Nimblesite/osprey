//! The `test` subcommand over directories: parallelism, caching, scratch hygiene, coverage and broken suites.

use std::path::{Path, PathBuf};

use super::test_runner::{
    assert_parallel_default_has_serial_escape, executable_script, parallel_probe_fixture,
    run_cache_probe, run_concurrency_probe, run_conformance_probe, write_in, FAILING_TESTS,
    PASSING_TESTS,
};
use super::{finish, osprey, read_text, run_args, run_file, run_source, temp_dir, temp_osp, HELLO};

#[test]
fn test_subcommand_runs_directories_and_files() {
    let dir = temp_dir("suite");
    let _ = write_in(&dir, "pass.test.osp", PASSING_TESTS);
    let _ = write_in(
        &dir,
        "fail.test.ospml",
        "test \"ml bad\" (\\() => check \"v\" 3 (1 + 1))\n",
    );
    let _ = write_in(&dir, "ignored.osp", HELLO);

    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    o.expect_stdout(
        1,
        &[
            "# file:",
            "ok 1 - adds",
            "# check 'v' failed: expected 3, got 2",
            "# suites: 1 passed, 1 failed",
        ],
        &["v=hi"],
    );

    // A single file runs regardless of naming; --filter reaches the children.
    let single = write_in(&dir, "single.osp", FAILING_TESTS);
    let o = run_args(&[
        "test",
        single.to_string_lossy().as_ref(),
        "--filter",
        "good math",
        "--quiet",
    ]);
    o.expect_stdout(0, &["# suites: 1 passed, 0 failed"], &["# file:"]);
}

// [TESTING-PARALLEL] Independent suites compile concurrently by default, while
// configuration can force serial execution for constrained or diagnostic runs.
#[cfg(unix)]
#[test]
fn test_subcommand_parallel_default_has_serial_escape_hatch() {
    let dir = temp_dir("suite_parallel");
    let (probe, driver) = parallel_probe_fixture(&dir, &dir, &dir, "probe-cc.sh");
    assert_parallel_default_has_serial_escape(
        &probe,
        |jobs| run_concurrency_probe(&dir, &driver, jobs),
        "test suites compiled serially by default",
    );
}

#[cfg(unix)]
#[test]
fn conformance_corpus_parallel_default_has_serial_escape_hatch() {
    let root = temp_dir("conformance_parallel");
    let tests = root.join("src/osprey/tests");
    let bin = root.join("target/release");
    let (probe, _) = parallel_probe_fixture(&root, &tests, &bin, "osprey");
    assert_parallel_default_has_serial_escape(
        &probe,
        |jobs| run_conformance_probe(&root, jobs),
        "conformance suites ran serially by default",
    );
}

/// One cache probe run: the test subcommand succeeds and the counting
/// compiler driver has been invoked `count` times so far.
#[cfg(unix)]
fn probe_compiles(dir: &Path, driver: &Path, count: &str) {
    let probe = run_cache_probe(dir, driver);
    assert_eq!(probe.code, Some(0), "{}", probe.stderr);
    assert_eq!(read_text(&dir.join("cc-count")), count);
}

/// Every path left under `scratch`, which a clean run leaves empty.
fn leaked_paths(scratch: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(scratch)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect()
}

#[cfg(unix)]
#[test]
fn test_subcommand_reuses_unchanged_compiled_suites() {
    // [TESTING-NATIVE-CACHE] cached suites reuse compilation and linking;
    // untracked link directories still rebuild on every invocation.
    let dir = temp_dir("suite_cache");
    let _ = write_in(&dir, "cached.test.osp", PASSING_TESTS);
    let _ = write_in(
        &dir,
        "cached_http.test.osp",
        "let client = httpCreateClient(\"http://127.0.0.1:1\", 1) ?: -1\n\
         let closed = httpCloseClient(client) ?: -1\n\
         test(\"http client\", fn() => expect(closed, 0))\n",
    );
    let linked = format!("// @link: m\n{PASSING_TESTS}");
    let _ = write_in(&dir, "cached_link.test.osp", &linked);
    let custom_search = format!("// @linkdir: {}\n{PASSING_TESTS}", dir.display());
    let _ = write_in(&dir, "uncached_linkdir.test.osp", &custom_search);
    let driver = executable_script(
        &dir,
        "counting-cc.sh",
        "#!/bin/sh\nprintf x >> \"$OSPREY_CC_COUNT\"\nexec clang \"$@\"\n",
    );

    // Three cached object/link pairs, plus one uncached combined invocation.
    probe_compiles(&dir, &driver, "xxxxxxx");
    probe_compiles(&dir, &driver, "xxxxxxxx");
}

#[cfg(unix)]
#[test]
fn run_removes_temporary_native_artifacts() {
    let dir = temp_dir("native_cleanup");
    let scratch = dir.join("scratch");
    let _ = std::fs::create_dir_all(&scratch);
    let suite = write_in(&dir, "cleanup.test.osp", PASSING_TESTS);
    let mut cmd = osprey();
    let _ = cmd
        .args([suite.to_string_lossy().as_ref(), "--run", "--quiet"])
        .env("TMPDIR", &scratch)
        .env_remove("OSPREY_TEST_CACHE_DIR");
    let output = finish(&mut cmd);
    assert_eq!(output.code, Some(0), "{}", output.stderr);
    let leaked = leaked_paths(&scratch);
    assert!(leaked.is_empty(), "temporary artifacts leaked: {leaked:?}");
}

// [TESTING-CLI-RUN-SCRATCH] `--run` on a test file keeps relative output out of
// the caller's directory, in both flavors; an ordinary program still writes there.
#[test]
fn run_keeps_test_file_output_out_of_the_callers_directory() {
    const OUTPUT: &str = "relative_output.txt";
    let osp = "let saved = writeFile(\"relative_output.txt\", \"kept\")\n\
               match saved {\n    Success => print(\"saved\")\n    Error => print(\"failed\")\n}\n\
               match readFile(\"relative_output.txt\") {\n    Success => print(\"read\")\n    Error => print(\"lost\")\n}\n";
    let ospml = "saved = writeFile \"relative_output.txt\" \"kept\"\n\
                 match saved\n    Success _value => print \"saved\"\n    Error _message => print \"failed\"\n\
                 match readFile \"relative_output.txt\"\n    Success _value => print \"read\"\n    Error _message => print \"lost\"\n";
    for (name, body, leaves_output) in [
        ("suite.test.osp", osp, false),
        ("suite.test.ospml", ospml, false),
        ("program.osp", osp, true),
        ("program.ospml", ospml, true),
    ] {
        let dir = temp_dir(&format!("run_cwd_{}", name.replace('.', "_")));
        let caller = dir.join("caller");
        let scratch = dir.join("scratch");
        let _ = std::fs::create_dir_all(&caller);
        let _ = std::fs::create_dir_all(&scratch);
        let source = write_in(&dir, name, body);
        let mut cmd = osprey();
        let _ = cmd
            .args([source.to_string_lossy().as_ref(), "--run", "--quiet"])
            .current_dir(&caller)
            .env("TMPDIR", &scratch)
            .env_remove("OSPREY_TEST_CACHE_DIR");
        let output = finish(&mut cmd);
        assert_eq!(output.code, Some(0), "{name}: {}", output.stderr);
        assert_eq!(output.stdout, "saved\nread\n", "{name}");
        assert_eq!(caller.join(OUTPUT).exists(), leaves_output, "{name}");
        let leaked = leaked_paths(&scratch);
        assert!(leaked.is_empty(), "{name}: scratch leaked: {leaked:?}");
    }
}

// A compile-error suite fails the run; an empty discovery set is loud.
#[test]
fn test_subcommand_reports_broken_and_missing_suites() {
    let dir = temp_dir("suite_bad");
    let _ = write_in(&dir, "broken.test.osp", "test(\"x\", fn() => expect(1)\n");
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    o.expect_stdout(1, &["# suites: 0 passed, 1 failed"], &[]);

    let empty = temp_dir("suite_empty");
    let o = run_args(&["test", empty.to_string_lossy().as_ref()]);
    o.expect_stderr(1, &["no test files found"], &[]);

    let o = run_args(&["test", "--filter"]);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    let o = run_args(&["test", "--nonsense"]);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    let o = run_args(&["test", "a", "b"]);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
}

// [TESTING-COVERAGE-CLI][TESTING-COVERAGE-JSON] --coverage instruments each
// suite and prints per-file and total line rates; --coverage-json also writes
// the merged machine-readable report the editor integration consumes.
#[test]
fn test_subcommand_coverage_reports_lines_and_writes_json() {
    let dir = temp_dir("suite_cov");
    let _ = write_in(&dir, "pass.test.osp", PASSING_TESTS);
    let json = dir.join("cov.json");
    let o = run_args(&[
        "test",
        dir.to_string_lossy().as_ref(),
        "--coverage-json",
        json.to_string_lossy().as_ref(),
    ]);
    o.expect_stdout(0, &["# coverage: ", "# coverage total: "], &[]);
    let report = std::fs::read_to_string(&json).expect("coverage json");
    assert!(report.starts_with("{\"files\":{"), "{report}");
    assert!(report.contains("pass.test.osp"), "{report}");
    assert!(report.contains("\"lines\":{"), "{report}");

    // --quiet drops the per-file rows but keeps the aggregate.
    let o = run_args(&[
        "test",
        dir.to_string_lossy().as_ref(),
        "--coverage",
        "--quiet",
    ]);
    o.expect_stdout(0, &["# coverage total: "], &["# coverage: "]);
}

// A type-error suite fails before execution, so it produces no dump; the run
// keeps going, and an unwritable JSON path is reported without aborting.
#[test]
fn test_subcommand_coverage_survives_type_errors_and_bad_json_paths() {
    let dir = temp_dir("suite_cov_bad");
    let _ = write_in(&dir, "pass.test.osp", PASSING_TESTS);
    let _ = write_in(&dir, "typebad.test.osp", "let n: int = \"nope\"\n");
    let bogus = dir.join("no-such-dir").join("cov.json");
    let o = run_args(&[
        "test",
        dir.to_string_lossy().as_ref(),
        "--coverage-json",
        bogus.to_string_lossy().as_ref(),
        "--quiet",
    ]);
    o.expect_stdout(1, &["# suites: 1 passed, 1 failed"], &[]);
    assert!(o.stderr.contains("type mismatch"), "{}", o.stderr);
    assert!(o.stderr.contains("no coverage dump"), "{}", o.stderr);
    assert!(
        o.stderr.contains("cannot write coverage json"),
        "{}",
        o.stderr
    );
}

// A nested test() must not silently reshuffle counters: it fails the
// enclosing case loudly [TESTING-BUILTIN-TEST], and a zero-case run still
// prints its plan so a matchless filter is visible [TESTING-TAP].
#[test]
fn nested_tests_fail_loudly_and_zero_case_runs_keep_the_plan() {
    let nested = temp_osp(
        "tap_nested",
        "test(\"outer\", fn() => {\n    expect(1, 2)\n    test(\"inner\", fn() => expect(3, 3))\n})\n",
    );
    let o = run_file(&nested, &["--run"]);
    o.expect_stdout(
        1,
        &["# nested test 'inner' skipped", "not ok 1 - outer"],
        &[],
    );

    let prog = temp_osp("tap_nomatch", PASSING_TESTS);
    let mut cmd = osprey();
    let _ = cmd
        .arg(&prog)
        .arg("--run")
        .env("OSPREY_TEST_FILTER", "matches nothing");
    let o = finish(&mut cmd);
    o.expect_stdout(0, &["1..0", "# tests=0 passed=0 failed=0"], &[]);
}

// [TESTING-EQUALITY] an Error operand is a visible mismatch, never a blind
// payload read. `checkedAdd` is the value-level overflow spelling; `intDiv`
// returns a plain `int` under [ARITH-EFFECT], so it has no Error to render.
#[test]
fn error_result_assertions_render_the_error() {
    let o = run_source(
        "tap_err_result",
        "test(\"overflow\", fn() => expect(checkedAdd(9223372036854775807, 1), 2))\n",
        &["--run"],
    );
    o.expect_stdout(
        1,
        &[
            "# expect failed: expected 2, got Error(integer overflow)",
            "not ok 1 - overflow",
        ],
        &[],
    );
}
