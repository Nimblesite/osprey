//! The `test` subcommand ([TESTING-FRAMEWORK]): TAP output, filters, listing, verdicts and skips.

use super::{
    finish, osprey, repo_root, run_args, run_file, run_source, set_mode, temp_dir, temp_osp, Out,
    HELLO,
};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

// --- testing framework (docs/specs/language/0027-TestingFramework.md) ---------------

pub(super) const PASSING_TESTS: &str = "test(\"adds\", fn() => expect(1 + 1, 2))\n\
test(\"labeled\", fn() => check(\"sum\", 4, 2 + 2))\n";

pub(super) const FAILING_TESTS: &str = "test(\"bad math\", fn() => expect(1 + 1, 3))\n\
test(\"good math\", fn() => expect(2 + 2, 4))\n";

/// Write `body` to `<dir>/<name>` and return the path.
pub(super) fn write_in(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    let _ = std::fs::write(&path, body);
    path
}

#[cfg(unix)]
pub(super) fn executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let script = write_in(dir, name, body);
    set_mode(&script, 0o755);
    script
}

#[cfg(unix)]
const CONCURRENCY_PROBE_SCRIPT: &str = "#!/bin/sh\nmarker=\"$OSPREY_PARALLEL_PROBE/worker-$$\"\n\
         : > \"$marker\"\n\
         count=$(find \"$OSPREY_PARALLEL_PROBE\" -name 'worker-*' | wc -l)\n\
         if [ \"$count\" -gt 1 ]; then : > \"$OSPREY_PARALLEL_PROBE/overlap\"; fi\n\
         sleep 1\nrm -f \"$marker\"\nexit 1\n";

#[cfg(unix)]
pub(super) fn parallel_probe_fixture(
    root: &Path,
    tests: &Path,
    bin: &Path,
    driver_name: &str,
) -> (PathBuf, PathBuf) {
    let probe = root.join("probe");
    for dir in [tests, bin, &probe] {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = write_in(tests, "one.test.osp", PASSING_TESTS);
    let _ = write_in(tests, "two.test.osp", PASSING_TESTS);
    let driver = executable_script(bin, driver_name, CONCURRENCY_PROBE_SCRIPT);
    (probe, driver)
}

/// Applies the `OSPREY_TEST_JOBS` override when `jobs` is given.
#[cfg(unix)]
fn apply_jobs(cmd: &mut Command, jobs: Option<&str>) {
    if let Some(value) = jobs {
        let _ = cmd.env("OSPREY_TEST_JOBS", value);
    }
}

#[cfg(unix)]
pub(super) fn run_concurrency_probe(dir: &Path, driver: &Path, jobs: Option<&str>) -> Out {
    let mut cmd = osprey();
    let _ = cmd
        .args(["test", dir.to_string_lossy().as_ref(), "--quiet"])
        .env("OSPREY_CC", driver)
        .env("OSPREY_PARALLEL_PROBE", dir.join("probe"));
    apply_jobs(&mut cmd, jobs);
    finish(&mut cmd)
}

#[cfg(unix)]
pub(super) fn run_conformance_probe(root: &Path, jobs: Option<&str>) -> Out {
    let mut cmd = Command::new("zsh");
    let _ = cmd
        .arg(repo_root().join("src/toolchain/crates/run_test_corpus.sh"))
        .arg("default")
        .current_dir(repo_root())
        .env("OSPREY_ROOT", root)
        .env("OSPREY_PARALLEL_PROBE", root.join("probe"));
    apply_jobs(&mut cmd, jobs);
    finish(&mut cmd)
}

#[cfg(unix)]
pub(super) fn assert_parallel_default_has_serial_escape(
    probe: &Path,
    mut run: impl FnMut(Option<&str>) -> Out,
    failure: &str,
) {
    let parallel = run(None);
    assert_eq!(parallel.code, Some(1), "{}", parallel.stderr);
    let overlap = probe.join("overlap");
    assert!(overlap.exists(), "{failure}");
    let _ = std::fs::remove_file(&overlap);
    let serial = run(Some("1"));
    assert_eq!(serial.code, Some(1), "{}", serial.stderr);
    assert!(!overlap.exists(), "OSPREY_TEST_JOBS=1 was not serial");
}

#[cfg(unix)]
pub(super) fn run_cache_probe(dir: &Path, driver: &Path) -> Out {
    let mut cmd = osprey();
    let _ = cmd
        .args(["test", dir.to_string_lossy().as_ref(), "--quiet"])
        .env("OSPREY_CC", driver)
        .env("OSPREY_CC_COUNT", dir.join("cc-count"))
        .env("OSPREY_TEST_CACHE_DIR", dir.join("cache"));
    finish(&mut cmd)
}

// [TESTING-BUILTIN-TEST][TESTING-BUILTIN-EXPECT][TESTING-BUILTIN-CHECK]
// [TESTING-RUNTIME][TESTING-TAP][TESTING-EXIT] a test binary reports TAP and
// exits by outcome.
#[test]
fn test_builtins_emit_tap_and_exit_status() {
    let o = run_source("tap_pass", PASSING_TESTS, &["--run"]);
    o.expect_stdout(
        0,
        &["ok 1 - adds", "1..2", "# tests=2 passed=2 failed=0"],
        &[],
    );

    let o = run_source("tap_fail", FAILING_TESTS, &["--run"]);
    o.expect_stdout(
        1,
        &[
            "# expect failed: expected 3, got 2",
            "not ok 1 - bad math",
            "ok 2 - good math",
            "# tests=2 passed=1 failed=1",
        ],
        &[],
    );
}

// [TESTING-FILTER] the env var exact-matches one case; others skip silently.
#[test]
fn test_filter_env_selects_one_case() {
    let prog = temp_osp("tap_filter", FAILING_TESTS);
    let mut cmd = osprey();
    let _ = cmd
        .arg(&prog)
        .arg("--run")
        .env("OSPREY_TEST_FILTER", "good math");
    let o = finish(&mut cmd);
    o.expect_stdout(0, &["ok 1 - good math", "1..1"], &["bad math"]);
}

// [TESTING-LIST] static discovery lists literal names with positions and
// skips dynamic names; a testless file lists as [].
#[test]
fn list_tests_reports_literal_cases_as_json() {
    let o = run_source(
        "list_tests",
        "let name = \"dyn\"\ntest(\"first\", fn() => expect(1, 1))\ntest(name, fn() => expect(1, 1))\n",
        &["--list-tests"],
    );
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert_eq!(
        o.stdout.trim(),
        "[{\"name\":\"first\",\"line\":2,\"column\":1}]"
    );

    let o = run_source("list_none", HELLO, &["--list-tests"]);
    assert_eq!(o.stdout.trim(), "[]");
}

// [TESTING-DOC] a `///` block above a case travels through `--list-tests` as
// `summary` (the Test Explorer's inline description) and `doc` (the hover
// markdown); an undocumented case emits neither key.
#[test]
fn list_tests_carries_case_documentation() {
    let o = run_source(
        "list_tests_docs",
        "fn add(a, b) = a + b\n\
         /// Addition is commutative.\n\
         ///\n\
         /// # Parameters\n\
         /// - left: the first addend\n\
         ///\n\
         /// # Since\n\
         /// 0.3\n\
         test(\"commutes\", fn() => expect(add(1, 2), add(2, 1)))\n\
         test(\"bare\", fn() => expect(1, 1))\n",
        &["--list-tests"],
    );
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    let out = o.stdout.trim();
    assert!(
        out.contains("\"summary\":\"Addition is commutative.\""),
        "{out}"
    );
    assert!(out.contains("**Parameters**"), "sections render: {out}");
    assert!(
        out.contains("- `left` \\u2014 the first addend")
            || out.contains("- `left` — the first addend"),
        "{out}"
    );
    assert!(out.contains("**Since**"), "{out}");
    // The case's own line, not the doc block's first line.
    assert!(out.contains("\"name\":\"commutes\",\"line\":9"), "{out}");
    // The undocumented case keeps the bare three-key shape.
    assert!(
        out.contains("{\"name\":\"bare\",\"line\":10,\"column\":1}"),
        "{out}"
    );
    // Whatever the content, the whole array must be a single JSON line the
    // extension can JSON.parse.
    assert!(!out.trim_end().contains('\n'), "one line of JSON: {out}");

    // The ML twin lowers the `(** … *)` form to the same wire shape.
    let ml_dir = temp_dir("list_tests_docs_ml");
    let ml = write_in(
        &ml_dir,
        "docs.test.ospml",
        "add a b = a + b\n\
         (** Addition is commutative. *)\n\
         test \"commutes\" (\\() => check \"c\" (add 1 2) (add 2 1))\n",
    );
    let o = run_file(&ml, &["--list-tests"]);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert!(
        o.stdout
            .contains("\"summary\":\"Addition is commutative.\""),
        "{}",
        o.stdout
    );
}

// [TESTING-CLI-RUN][TESTING-FILE-CONVENTION] the runner discovers
// *.test.osp{,ml} under a directory, streams TAP under headers, and aggregates
// the exit status.
/// [TESTING-SKIP-WARNING] a skipped case is never silent: `osprey test` echoes
/// every TAP `# SKIP` directive as a stderr warning naming the case and reason.
#[test]
fn test_subcommand_warns_on_every_skipped_case() {
    let dir = temp_dir("skip-warns");
    let _ = write_in(
        &dir,
        "skips.test.osp",
        "type Verdict = Pass | Fail(string) | Skip(string)\n\
         test(\"parked case\", fn() => Skip(\"blocked on #123\"))\n\
         test(\"live case\", fn() => expect(1, 1))\n",
    );
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    o.expect_stdout(0, &["# SKIP blocked on #123"], &[]);
    assert!(
        o.stderr
            .contains("warning: test 'parked case' skipped: blocked on #123"),
        "{}",
        o.stderr
    );
    assert!(!o.stderr.contains("live case"), "{}", o.stderr);
}

/// [TESTING-VERDICT] `test` reports the states the program's OWN `Verdict`
/// declares. A payload-free `Skip`/`Fail` reports with no reason rather than
/// failing codegen on a binder the declaration never named, and a reasonless
/// skip still prints a bare `# SKIP` directive that the diagnostic path sees.
///
/// [TESTING-SKIP-REASON] and that reasonless skip is an ERROR: the TAP is
/// unchanged — the case still reports skipped, not failed — but the runner
/// fails the suite, because a hole in coverage nobody wrote a cause for is a
/// defect rather than a debt someone chose.
#[test]
fn a_verdict_declaring_payload_free_states_reports_each_of_them() {
    let dir = temp_dir("verdict-bare");
    let _ = write_in(
        &dir,
        "bare.test.osp",
        "type Verdict = Pass | Fail | Skip\n\
         test(\"parked\", fn() => Skip)\n\
         test(\"green\", fn() => Pass)\n",
    );
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    o.expect_stdout(
        1,
        &[
            "ok 1 - parked # SKIP\n",
            "ok 2 - green\n",
            "# tests=2 passed=1 failed=0 skipped=1",
        ],
        &[],
    );
    assert!(
        o.stderr
            .contains("error: test 'parked' skipped with no reason; every skip must name one"),
        "{}",
        o.stderr
    );
    assert!(
        !o.stderr.contains("warning: test 'parked'"),
        "an unexplained skip is an error, never also a warning: {}",
        o.stderr
    );
}

/// [TESTING-SKIP-REASON] a skip that NAMES a reason stays a warning and keeps
/// the suite green — the two strengths are decided by the reason alone, not by
/// whether the `Verdict` declaration gives `Skip` a payload at all.
/// Writes one `Skip`-verdict suite `file` in a fresh `tag` dir and runs it;
/// `reason` is the only varying input of the case.
fn run_skip_case(tag: &str, file: &str, reason: &str) -> Out {
    let dir = temp_dir(tag);
    let _ = write_in(
        &dir,
        file,
        &format!(
            "type Verdict = Pass | Fail(string) | Skip(string)\n\
             test(\"parked\", fn() => Skip(\"{reason}\"))\n"
        ),
    );
    run_args(&["test", dir.to_string_lossy().as_ref()])
}

#[test]
fn a_reasoned_skip_warns_while_an_unexplained_one_fails_the_suite() {
    let green = run_skip_case("skip-reasoned", "ok.test.osp", "blocked on #123");
    assert_eq!(green.code, Some(0), "{}\n{}", green.stdout, green.stderr);
    assert!(
        green
            .stderr
            .contains("warning: test 'parked' skipped: blocked on #123"),
        "{}",
        green.stderr
    );

    // The SAME declaration with an empty reason is the error case: the author
    // had somewhere to write the cause and left it blank.
    let red = run_skip_case("skip-blank", "blank.test.osp", "");
    assert_eq!(red.code, Some(1), "{}\n{}", red.stdout, red.stderr);
    assert!(
        red.stderr
            .contains("error: test 'parked' skipped with no reason"),
        "{}",
        red.stderr
    );
    assert!(
        red.stdout.contains("# suites: 0 passed, 1 failed"),
        "{}",
        red.stdout
    );
}

/// [TESTING-VERDICT] the payload is read per state, not per union: a `Verdict`
/// that gives `Fail` a reason and `Skip` none reports both correctly.
#[test]
fn a_verdict_may_give_one_state_a_reason_and_another_none() {
    let dir = temp_dir("verdict-mixed");
    let _ = write_in(
        &dir,
        "mixed.test.osp",
        "type Verdict = Pass | Fail(string) | Skip\n\
         test(\"parked\", fn() => Skip)\n\
         test(\"broken\", fn() => Fail(\"expected 3\"))\n",
    );
    let o = run_args(&["test", dir.to_string_lossy().as_ref()]);
    o.expect_stdout(
        1,
        &[
            "ok 1 - parked # SKIP\n",
            "# fail: expected 3",
            "not ok 2 - broken",
        ],
        &[],
    );
}

/// [TESTING-VERDICT] a `Verdict` state `test` has no report primitive for is
/// rejected by name, not by a binder error from a pattern nobody wrote.
#[test]
fn a_verdict_state_test_cannot_report_is_rejected_by_name() {
    let o = run_source(
        "verdict_extra_state",
        "type Verdict = Pass | Fail(string) | Skip(string) | Todo(string)\n\
         test(\"case\", fn() => Pass)\n",
        &["--run"],
    );
    o.expect_rejected(&["Todo", "Verdict"]);
}
