//! Result cache of [TYPES-DISCIPLINE-PROTOCOL]: a response is kept under the
//! hash of its request and the checker source, so an unchanged module with
//! unchanged imports is never re-checked.

use super::protocol::{Request, Response};
use super::{CheckerRunner, Discipline};
use crate::ProjectConfig;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

/// Run `request` through `runner`, answering from the cache when it holds the
/// same request against the same checker source.
pub(super) fn run_cached(
    config: &ProjectConfig,
    discipline: &Discipline,
    runner: &dyn CheckerRunner,
    request: &Request,
) -> Result<Response, String> {
    let line = request.to_line()?;
    let checker = std::fs::read(&discipline.checker).unwrap_or_default();
    let entry = entry_path(config, discipline, &line, &checker);
    if let Ok(hit) = std::fs::read_to_string(&entry) {
        if let Ok(response) = Response::parse(&hit) {
            return Ok(response);
        }
    }
    let answer = runner.run(discipline, &line)?;
    let response = Response::parse(&answer)?;
    remember(&entry, &answer);
    Ok(response)
}

/// A stable 64-bit digest of `bytes` for cache and build file names.
#[must_use]
pub(super) fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn entry_path(
    config: &ProjectConfig,
    discipline: &Discipline,
    line: &str,
    checker: &[u8],
) -> PathBuf {
    let dir = config.directory.join(".osprey/cache/disciplines");
    let mut hasher = DefaultHasher::new();
    line.hash(&mut hasher);
    checker.hash(&mut hasher);
    discipline.name.hash(&mut hasher);
    dir.join(format!("{:016x}.json", hasher.finish()))
}

/// A cache write that fails leaves the compile correct, only slower.
fn remember(path: &std::path::Path, answer: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, answer);
}
