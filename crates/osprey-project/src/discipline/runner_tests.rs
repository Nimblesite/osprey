//! Real-process regressions for the checker deadline of
//! [TYPES-DISCIPLINE-PROTOCOL]: the one deadline taken at the top of `run`
//! covers build, spawn, request delivery and the response, proven with tiny
//! `sh` checkers in temp directories instead of scripted answers.

#![cfg(unix)]

use super::run_executable;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
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

#[test]
fn a_checker_that_never_reads_stdin_is_killed_at_the_deadline() {
    // Review item 1 of 0041: `feed` used to write synchronously, so a checker
    // that never read stdin blocked the compiler on a full pipe for ever and
    // the deadline never applied.
    let checker = Script::new("sleep.sh", "#!/bin/sh\nsleep 30\n");
    let started = Instant::now();
    let answer = run_executable(&checker.path, &checker.dir, &large_request(), deadline(1));
    let elapsed = started.elapsed();
    let error = answer.expect_err("the sleeping checker exceeds its limit");
    assert!(error.contains("exceeded its time limit"), "{error}");
    assert!(
        elapsed < Duration::from_secs(5),
        "deadline enforced after {elapsed:?}"
    );
}

#[test]
fn a_checker_that_exits_without_reading_reports_no_response() {
    // A checker exiting cleanly with no output is the no-response error, and
    // the stdin writer thread neither panics nor hangs on the closed pipe.
    let checker = Script::new("exit.sh", "#!/bin/sh\nexit 0\n");
    let started = Instant::now();
    let answer = run_executable(&checker.path, &checker.dir, &large_request(), deadline(30));
    let elapsed = started.elapsed();
    let error = answer.expect_err("no output is no response");
    assert!(error.contains("no response"), "{error}");
    assert!(
        elapsed < Duration::from_secs(10),
        "writer thread never blocks the poll: {elapsed:?}"
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
