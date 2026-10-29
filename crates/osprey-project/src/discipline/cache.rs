//! Result cache of [TYPES-DISCIPLINE-PROTOCOL]: a response is kept under the
//! hash of its request and the checker binary that answered it, so only an
//! unchanged module handed to an unchanged binary is answered from disk.

use super::protocol::{Request, Response};
use super::{CheckerRunner, Discipline};
use crate::ProjectConfig;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Run `request` through `runner`, answering from the cache when it holds the
/// same request against the same checker binary.
///
/// The request takes **one** budget ([TYPES-DISCIPLINE-PROTOCOL]): the digest
/// that builds a cold checker, the cache lookup and the answer all spend the
/// discipline's limit from the instant the request starts, so a cold build
/// cannot mint a second one. A hit that arrives after the limit has already
/// gone by is that same failure, not an answer.
pub(super) fn run_cached(
    config: &ProjectConfig,
    discipline: &Discipline,
    runner: &dyn CheckerRunner,
    request: &Request,
) -> Result<Response, String> {
    let started = Instant::now();
    let line = request.to_line()?;
    let binary = runner.binary_digest(discipline)?;
    let entry = entry_path(config, discipline, &line, binary);
    if let Some(response) = cached(&entry) {
        return remaining(discipline, started).map(|_left| response);
    }
    let answer = runner.run(&budgeted(discipline, started)?, &line)?;
    let response = Response::parse(&answer)?;
    // The checker can exit a poll after the deadline: a late answer is not one.
    let _left = remaining(discipline, started)?;
    store(&entry, &answer);
    Ok(response)
}

/// The discipline as the checker sees it: only what is left of its limit.
///
/// The runner still takes its deadline from the timeout it is given, so it
/// lands on this request's single deadline instead of a fresh one.
fn budgeted(discipline: &Discipline, started: Instant) -> Result<Discipline, String> {
    Ok(Discipline {
        timeout: remaining(discipline, started)?,
        ..discipline.clone()
    })
}

/// What is left of the limit — `started` is the one origin — or the failure
/// once there is nothing left to spend.
fn remaining(discipline: &Discipline, started: Instant) -> Result<Duration, String> {
    let left = discipline.timeout.saturating_sub(started.elapsed());
    if left.is_zero() {
        return Err(super::runner::TIME_LIMIT.to_string());
    }
    Ok(left)
}

/// A stable 64-bit digest of `bytes` for cache and build file names.
#[must_use]
pub(super) fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn entry_path(config: &ProjectConfig, discipline: &Discipline, line: &str, binary: u64) -> PathBuf {
    let dir = config.directory.join(".osprey/cache/disciplines");
    let mut hasher = DefaultHasher::new();
    line.hash(&mut hasher);
    binary.hash(&mut hasher);
    discipline.name.hash(&mut hasher);
    dir.join(format!("{:016x}.json", hasher.finish()))
}

/// A cached answer, when the file reads and parses; anything else is a miss.
fn cached(path: &Path) -> Option<Response> {
    Response::parse(&std::fs::read_to_string(path).ok()?).ok()
}

/// A cache write that fails leaves the compile correct, only slower.
fn store(path: &Path, answer: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, answer);
}
