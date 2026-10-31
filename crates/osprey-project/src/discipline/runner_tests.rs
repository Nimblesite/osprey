//! Real-process regressions for the checker deadline of
//! [TYPES-DISCIPLINE-PROTOCOL]: the one deadline taken at the top of `run`
//! covers build, spawn, request delivery and the response, proven with tiny
//! `sh` checkers in temp directories instead of scripted answers.

#![cfg(unix)]

use super::{build, run_executable, CheckerRunner, ProcessRunner};
use crate::discipline::protocol::Request;
use crate::discipline::{cache, Discipline};
use crate::{ProjectConfig, SourceFile};
use osprey_syntax::Flavor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A 256 KiB request, larger than any pipe buffer, so a checker that never
/// reads would block a synchronous writer for ever.
fn large_request() -> String {
    "request ".repeat(32 * 1024)
}

/// A tiny shell-script checker in its own temp directory, executable and
/// removed when the test ends, pass or fail.
struct Script {
    path: PathBuf,
    dir: PathBuf,
}

impl Script {
    fn new(name: &str, body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osprey-runner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("checker temp dir");
        let path = dir.join(name);
        std::fs::write(&path, body).expect("checker script");
        let mut permissions = std::fs::metadata(&path)
            .expect("checker metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("checker chmod");
        Self { path, dir }
    }

    fn failure(&self, request: &str, deadline: Instant) -> (String, Duration) {
        let started = Instant::now();
        let error = run_executable(&self.path, &self.dir, request, deadline)
            .expect_err("the checker must fail");
        (error, started.elapsed())
    }

    fn rejects_within(&self, request: &str, deadline: Instant, expected: &str, limit: Duration) {
        let (error, elapsed) = self.failure(request, deadline);
        assert!(error.contains(expected), "{error}");
        assert!(elapsed < limit, "deadline enforced after {elapsed:?}");
    }

    fn runner(&self) -> (ProcessRunner, Discipline) {
        std::fs::write(self.dir.join("checker.ospml"), "namespace bounded\n")
            .expect("checker source");
        (
            ProcessRunner::new(self.path.clone(), self.dir.join("build")),
            discipline(&self.dir),
        )
    }

    fn build(&self, deadline: Instant) -> Result<PathBuf, String> {
        let output = self.dir.join("checker");
        build(&self.path, &self.path, &output, deadline).map(|()| output)
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// One deadline `seconds` from now, exactly as `run` takes it.
fn deadline(seconds: u64) -> Instant {
    Instant::now() + Duration::from_secs(seconds)
}

fn invalid_diagnostic(status: i32) -> String {
    let checker = Script::new(
        &format!("binary-status-{status}.sh"),
        &format!("#!/bin/sh\nprintf 'checker response'\nprintf '\\377' >&2\nexit {status}\n"),
    );
    checker.failure("request", deadline(30)).0
}

/// A discipline whose checker is `root/checker.ospml` and whose build lands
/// in `root/build`, as a registry entry would describe it.
fn discipline(root: &Path) -> Discipline {
    Discipline {
        name: "bounded".into(),
        package: "@osprey/bounded".into(),
        root: root.to_path_buf(),
        checker: root.join("checker.ospml"),
        timeout: Duration::from_secs(30),
    }
}

/// The names in `dir`, sorted, so a test can prove what survived a build.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("temp dir listing")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_checker_that_never_reads_stdin_is_killed_at_the_deadline() {
    // Review item 1 of 0041: `feed` used to write synchronously, so a checker
    // that never read stdin blocked the compiler on a full pipe for ever and
    // the deadline never applied.
    let checker = Script::new("sleep.sh", "#!/bin/sh\nsleep 30\n");
    checker.rejects_within(
        &large_request(),
        deadline(1),
        "exceeded its time limit",
        Duration::from_secs(5),
    );
}

#[test]
fn a_descendant_holding_the_pipes_cannot_outlive_the_deadline() {
    // `sleep 3 &` inherits stdout and stderr, so both pipes stay open after
    // the checker itself exits. Waiting for them to close used to outlive the
    // deadline by the descendant's whole lifetime.
    let checker = Script::new("orphan.sh", "#!/bin/sh\nsleep 3 &\nexit 0\n");
    checker.rejects_within(
        "request",
        deadline(1),
        "exceeded its time limit",
        Duration::from_secs(2),
    );
}

#[test]
fn a_checker_that_exits_without_reading_reports_no_response() {
    // A checker exiting cleanly with no output is the no-response error, and
    // the stdin writer thread neither panics nor hangs on the closed pipe.
    let checker = Script::new("exit.sh", "#!/bin/sh\nexit 0\n");
    checker.rejects_within(
        &large_request(),
        deadline(30),
        "no response",
        Duration::from_secs(10),
    );
}

#[test]
fn a_checker_that_echoes_the_request_returns_it_whole() {
    // The happy path: the whole 256 KiB request reaches a checker that only
    // answers after stdin closes, and its echo comes back.
    let checker = Script::new("cat.sh", "#!/bin/sh\ncat\n");
    let request = large_request();
    let response =
        run_executable(&checker.path, &checker.dir, &request, deadline(30)).expect("cat echoes");
    assert_eq!(
        response.len(),
        request.len() + 1,
        "whole request plus the newline"
    );
}

#[test]
fn a_failed_build_is_never_reused_as_a_cache_entry() {
    // A compile that writes a partial artifact and then fails used to leave it
    // at the cache path, so the next request ran a binary the build never
    // produced. The artifact may only appear after a successful build.
    let compiler = Script::new(
        "partial-build.sh",
        "#!/bin/sh\nfor output; do :; done\nprintf 'partial' > \"$output\"\nexit 1\n",
    );
    let build_dir = compiler.dir.join("build");
    let (runner, _) = compiler.runner();
    let first = runner.executable(&discipline(&compiler.dir), deadline(30));
    assert!(first.is_err(), "a failed build is an error: {first:?}");
    let second = runner.executable(&discipline(&compiler.dir), deadline(30));
    assert!(
        second.is_err(),
        "a failed build must never be reused: {second:?}"
    );
    assert!(
        entries(&build_dir).is_empty(),
        "no artifact survives a failed build: {:?}",
        entries(&build_dir)
    );
}

#[test]
fn checker_compilation_produces_a_runnable_artifact() {
    let compiler = Script::new(
        "build.sh",
        "#!/bin/sh\nfor output; do :; done\nprintf '#!/bin/sh\\ncat\\n' > \"$output\"\nchmod +x \"$output\"\n",
    );
    let output = compiler.build(deadline(30)).expect("checker builds");
    assert_eq!(
        run_executable(&output, &compiler.dir, "request", deadline(30)),
        Ok("request\n".to_string())
    );
}

#[test]
fn checker_compilation_requires_an_artifact_even_after_a_successful_exit() {
    let compiler = Script::new("empty-build.sh", "#!/bin/sh\nexit 0\n");
    let error = compiler
        .build(deadline(30))
        .expect_err("a successful process without an artifact is not a build");
    assert_eq!(error, "failed to build: (no stderr)");
}

#[test]
fn checker_compile_errors_preserve_a_bounded_utf8_diagnostic_tail() {
    // Three-byte scalars: the 800-byte cut lands inside a character, so the
    // tail starts at the next boundary and keeps exactly 266 whole euros.
    let diagnostic = "€".repeat(500);
    let body = format!("#!/bin/sh\nprintf '%s' '{diagnostic}' >&2\nexit 7\n");
    let compiler = Script::new("bad-build.sh", &body);
    let error = compiler
        .build(deadline(30))
        .expect_err("compiler failure must be reported");
    assert_eq!(error, format!("failed to build: {}", "€".repeat(266)));
}

#[test]
fn checker_compilation_cannot_outlive_its_deadline() {
    let compiler = Script::new("slow-build.sh", "#!/bin/sh\nexec sleep 30\n");
    let started = Instant::now();
    let error = compiler
        .build(deadline(1))
        .expect_err("checker compilation must stop at its deadline");
    assert_eq!(error, "could not be built: exceeded its time limit");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_timed_out_build_leaves_no_artifact_and_no_lingering_wait() {
    // The compile writes a partial artifact and then hangs: neither the partial
    // artifact nor the descendant `sleep` may hold the compiler past the limit.
    let compiler = Script::new(
        "hung-build.sh",
        "#!/bin/sh\nfor output; do :; done\nprintf 'partial' > \"$output\"\nsleep 3\n",
    );
    let started = Instant::now();
    let error = compiler
        .build(deadline(1))
        .expect_err("a hung compile must stop at its deadline");
    let elapsed = started.elapsed();
    assert_eq!(error, "could not be built: exceeded its time limit");
    assert!(
        elapsed < Duration::from_secs(2),
        "the compile outlived its deadline by {elapsed:?}"
    );
    assert!(
        entries(&compiler.dir) == vec!["hung-build.sh".to_string()],
        "only the compiler survives a timed-out build: {:?}",
        entries(&compiler.dir)
    );
}

#[test]
fn a_non_utf8_build_diagnostic_is_reported_rather_than_silently_empty() {
    // Invalid stderr used to fail the read and vanish, so a failed compile with
    // a binary diagnostic reported "(no stderr)".
    let compiler = Script::new("binary-build.sh", "#!/bin/sh\nprintf '\\377' >&2\nexit 7\n");
    let error = compiler
        .build(deadline(30))
        .expect_err("a failed compile is an error");
    assert_eq!(
        error,
        "failed to build: could not decode stderr: invalid UTF-8"
    );
}

#[test]
fn a_non_utf8_crash_diagnostic_is_reported_rather_than_silently_empty() {
    let error = invalid_diagnostic(7);
    assert!(
        error.contains("could not decode stderr: invalid UTF-8"),
        "{error}"
    );
    assert!(error.contains("a response"), "{error}");
}

#[test]
fn an_unrepresentable_time_limit_is_an_error_not_a_panic() {
    // `Instant::now() + Duration::MAX` panics; a limit the clock cannot reach
    // is a discipline error the compiler can report.
    let root = std::env::temp_dir().join(format!("osprey-runner-overflow-{}", std::process::id()));
    let unbounded = Discipline {
        timeout: Duration::MAX,
        ..discipline(&root)
    };
    let runner = ProcessRunner::new(root.join("compiler"), root.join("build"));
    let error = runner
        .run(&unbounded, "request")
        .expect_err("an unrepresentable limit is an error");
    assert!(error.contains("the clock cannot reach"), "{error}");
}

#[test]
fn missing_compilers_report_a_spawn_failure() {
    let fixture = Script::new("fixture.sh", "#!/bin/sh\nexit 0\n");
    let error = build(
        &fixture.dir.join("missing-compiler"),
        &fixture.path,
        &fixture.dir.join("output"),
        deadline(30),
    )
    .expect_err("an absent compiler cannot produce a checker");
    assert!(error.starts_with("could not be built:"), "{error}");
}

#[test]
fn crashed_checkers_never_return_their_partial_response_as_success() {
    let checker = Script::new(
        "crash.sh",
        "#!/bin/sh\nprintf 'partial'\nprintf 'checker crashed' >&2\nexit 7\n",
    );
    let error = run_executable(&checker.path, &checker.dir, "request", deadline(30))
        .expect_err("partial output from a failed checker is not a response");
    assert_eq!(
        error,
        "exited with an error with a response: checker crashed"
    );
}

#[test]
fn a_successful_checker_cannot_hide_an_invalid_diagnostic() {
    let error = invalid_diagnostic(0);
    assert!(
        error.contains("could not decode stderr: invalid UTF-8"),
        "{error}"
    );
}

#[test]
fn a_successful_build_cannot_hide_an_invalid_diagnostic() {
    let compiler = Script::new(
        "binary-compiled.sh",
        "#!/bin/sh\nfor output; do :; done\nprintf '#!/bin/sh\\ncat\\n' > \"$output\"\nprintf '\\377' >&2\nexit 0\n",
    );
    let error = compiler
        .build(deadline(30))
        .expect_err("invalid build diagnostic bytes must not disappear on success");
    assert!(
        error.contains("could not decode stderr: invalid UTF-8"),
        "{error}"
    );
}

#[test]
fn timed_out_checkers_cannot_leave_descendants_running() {
    let checker = Script::new(
        "descendant.sh",
        "#!/bin/sh\n(sleep 2; printf 'escaped' > escaped) &\nwait\n",
    );
    let (error, _) = checker.failure("request", deadline(1));
    assert!(error.contains("exceeded its time limit"), "{error}");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        !checker.dir.join("escaped").exists(),
        "a timed-out checker's descendant continued to write files"
    );
}

#[test]
fn cold_checker_builds_are_cached_and_rebuilt_after_compiler_changes() {
    let compiler = Script::new("cold-build.sh", "#!/bin/sh\nfor output; do :; done\nprintf '#!/bin/sh\\ncat\\n' > \"$output\"\nchmod +x \"$output\"\n");
    let (runner, contract) = compiler.runner();
    assert_eq!(runner.run(&contract, "first"), Ok("first\n".into()));
    let before = runner
        .binary_digest(&contract)
        .expect("built binary digest");
    assert_eq!(runner.run(&contract, "cached"), Ok("cached\n".into()));
    let updated = std::fs::read_to_string(&compiler.path)
        .expect("compiler")
        .replace("cat", "printf changed");
    std::fs::write(&compiler.path, updated).expect("changed compiler");
    assert_ne!(
        runner
            .binary_digest(&contract)
            .expect("rebuilt binary digest"),
        before
    );
    assert_eq!(runner.run(&contract, "ignored"), Ok("changed".into()));
}

#[test]
fn a_successful_compile_must_report_failure_to_publish_its_artifact() {
    let compiler = Script::new(
        "publish-build.sh",
        "#!/bin/sh\nfor output; do :; done\nprintf binary > \"$output\"\n",
    );
    let blocked = compiler.dir.join("checker");
    std::fs::create_dir(&blocked).expect("destination occupied by a directory");
    let error = compiler
        .build(deadline(30))
        .expect_err("artifact cannot replace a directory");
    assert!(error.contains("cannot publish"), "{error}");
    assert_eq!(entries(&compiler.dir), vec!["checker", "publish-build.sh"]);
}

#[test]
fn one_deadline_covers_both_cold_compilation_and_the_checker_response() {
    let compiler = Script::new("combined-budget.sh", "#!/bin/sh\nsleep 0.7\nfor output; do :; done\nprintf '#!/bin/sh\\nsleep 0.7\\ncat\\n' > \"$output\"\nchmod +x \"$output\"\n");
    let (runner, contract) = compiler.runner();
    let limited = Discipline {
        timeout: Duration::from_secs(1),
        ..contract
    };
    let started = Instant::now();
    let error = runner
        .run(&limited, "request")
        .expect_err("build and response share one budget");
    assert!(error.contains("exceeded its time limit"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn module() -> SourceFile {
    let text = "// osprey: types=bounded\nnamespace proof\n\nmodule Nat\n    export zero = 0\n";
    let parsed = osprey_syntax::parse_program_with_flavor(text, Flavor::Ml);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    SourceFile {
        path: PathBuf::from("src/Nat.ospml"),
        flavor: Flavor::Ml,
        source: text.to_string(),
        program: parsed.program,
        discipline: None,
    }
}

fn accepted_line() -> String {
    let line = serde_json::json!({ "version": crate::discipline::protocol::VERSION, "result": "accepted",
        "signature": "signature Nat\n    zero : int\n",
        "erased": { "flavor": "ml", "source": "namespace proof\n\nmodule Nat\n    export zero = 0\n" },
        "diagnostics": [] })
    .to_string();
    let _parsed = crate::discipline::protocol::Response::parse(&line).expect("the fixture parses");
    line
}

/// A 0.7s build of a 0.7s checker that answers once: `printf` from a quoted
/// heredoc keeps the JSON escapes, and the `$0.ran` sidecar fails a rerun, so a
/// "hit" cannot be a silent rerun of unchanged binary bytes.
fn two_budget_compiler(answer: &str) -> String {
    format!(
        "#!/bin/sh\nsleep 0.7\nfor output; do :; done\ncat > \"$output\" <<'CHECKER'\n#!/bin/sh\n[ -e \"$0.ran\" ] && exit 1\n: > \"$0.ran\" || exit 1\nsleep 0.7\nprintf '%s\\n' '{answer}'\nCHECKER\nchmod +x \"$output\"\n"
    )
}

fn cache_fixture(name: &str) -> (Script, ProjectConfig, Request, ProcessRunner, Discipline) {
    let answer = accepted_line();
    let compiler = Script::new(name, &two_budget_compiler(&answer));
    let (runner, contract) = compiler.runner();
    let config = ProjectConfig::for_root(&compiler.dir);
    let request = Request::new(&module(), &config, Vec::new());
    (compiler, config, request, runner, contract)
}

#[test]
fn a_cold_cached_request_cannot_spend_a_budget_on_the_build_and_another_on_the_answer() {
    // [TYPES-DISCIPLINE-PROTOCOL]: the digest builds a cold checker, so one
    // request that also runs it must not get two limits (0.7 + 0.7 > 1s).
    let (_compiler, config, request, runner, contract) = cache_fixture("two-budget-cache.sh");
    let limited = Discipline {
        timeout: Duration::from_secs(1),
        ..contract
    };
    let started = Instant::now();
    let error = cache::run_cached(&config, &limited, &runner, &request)
        .expect_err("one budget, not two: the intact answer would print above");
    assert!(error.contains("exceeded its time limit"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_cached_request_inside_a_generous_budget_answers_and_is_then_a_hit() {
    let (_compiler, config, request, runner, contract) = cache_fixture("generous-cache.sh");
    let generous = Discipline {
        timeout: Duration::from_secs(30),
        ..contract
    };
    let cold = cache::run_cached(&config, &generous, &runner, &request).expect("cold answers");
    let warm = cache::run_cached(&config, &generous, &runner, &request).expect("cache hit");
    assert_eq!(cold, warm, "the ran marker would fail a rerun");
}
