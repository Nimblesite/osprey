//! The type-discipline seam of spec 0050 ([TYPES-DISCIPLINE-SEAM]).
//!
//! A source marked `// osprey: types=<name>` is handed to the checker package
//! registered under `<name>` before project assembly. The checker answers with
//! an export signature and an erased module; the erased module is parsed by
//! the ordinary frontend, ascribed to that signature and then flows through
//! assembly, `check_program` and codegen exactly as an `hm` source does.
//! Nothing below this module knows a discipline's name.

mod cache;
mod erase;
mod imports;
pub mod protocol;
mod registry;
mod runner;
#[cfg(test)]
mod tests;

pub use registry::{Discipline, Registry};
pub use runner::{CheckerRunner, ProcessRunner};

use crate::{ProjectConfig, ProjectError, SourceFile};
use std::path::Path;

/// The identity discipline: `check_program`, in process, no erasure.
pub const HM: &str = "hm";

/// Every failure of the seam itself is one diagnostic kind, so a wrong checker
/// is never reported as a type error in the user's module.
pub const ERROR_PREFIX: &str = "discipline-error";

/// The value of a leading `// osprey: types=<name>` marker (`# osprey:` in
/// Python flavor), read by the same header scan as the flavor marker.
/// Implements [TYPES-DISCIPLINE-SELECT].
#[must_use]
pub fn marker(source: &str) -> Option<&str> {
    source.lines().find_map(|line| {
        let t = line.trim();
        t.strip_prefix("// osprey: types=")
            .or_else(|| t.strip_prefix("//osprey: types="))
            .or_else(|| t.strip_prefix("# osprey: types="))
            .or_else(|| t.strip_prefix("#osprey: types="))
            .map(str::trim)
    })
}

/// The discipline a source is checked by: file marker, then
/// `[project].types`, then `hm`. Implements [TYPES-DISCIPLINE-SELECT].
#[must_use]
pub fn select<'a>(config: &'a ProjectConfig, source: &'a str) -> &'a str {
    marker(source).or(config.types.as_deref()).unwrap_or(HM)
}

/// Whether any source of the project needs a checker process at all. A
/// project with no marker and no `[project].types` never reads the registry,
/// builds a checker or links anything new ([TYPES-DISCIPLINE-NO-HARM]).
#[must_use]
pub fn any_disciplined(config: &ProjectConfig, sources: &[SourceFile]) -> bool {
    sources
        .iter()
        .any(|source| select(config, &source.source) != HM)
}

/// Run every non-`hm` source through its discipline and return the sources
/// the assembler sees: `hm` sources unchanged, the others replaced by their
/// erased modules under the signatures their checkers wrote. Sources are
/// visited in dependency order so each request carries the export
/// signatures of what it imports ([TYPES-DISCIPLINE-PROTOCOL]).
///
/// # Errors
/// Every `discipline-error` of [TYPES-DISCIPLINE-VERIFICATION]: unknown or
/// duplicate name, checker build or crash, timeout, malformed response,
/// unparsable erased module, module without a signature, or an imported
/// `hm` module whose signature could not be inferred.
pub fn apply(
    config: &ProjectConfig,
    sources: Vec<SourceFile>,
    runner: &dyn CheckerRunner,
) -> Result<Vec<SourceFile>, Vec<ProjectError>> {
    if !any_disciplined(config, &sources) {
        return Ok(sources);
    }
    let registry = Registry::discover(config).map_err(|error| vec![error])?;
    apply_with(config, sources, runner, &registry)
}

/// [`apply`] against an already discovered registry.
///
/// # Errors
/// As [`apply`].
pub fn apply_with(
    config: &ProjectConfig,
    sources: Vec<SourceFile>,
    runner: &dyn CheckerRunner,
    registry: &Registry,
) -> Result<Vec<SourceFile>, Vec<ProjectError>> {
    let order = imports::order(&sources);
    let needed = imports::needed(config, &sources);
    let mut exports = imports::Exports::default();
    let mut visited: Vec<SourceFile> = Vec::with_capacity(sources.len());
    let mut errors = Vec::new();
    let mut slots: Vec<Option<SourceFile>> = sources.into_iter().map(Some).collect();
    for index in order {
        let Some(source) = slots.get_mut(index).and_then(Option::take) else {
            continue;
        };
        let seam = Seam {
            exports: &mut exports,
            visited: &visited,
            wanted: needed.contains(&index),
        };
        match check_module(config, registry, runner, source, seam) {
            Ok(checked) => visited.push(checked),
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(visited)
    } else {
        Err(errors)
    }
}

/// What one module's check sees of the modules before it.
struct Seam<'a> {
    /// Export signatures gathered so far.
    exports: &'a mut imports::Exports,
    /// Every source already visited, in dependency order.
    visited: &'a [SourceFile],
    /// Whether a disciplined module imports this one, so its `hm` signature
    /// must be inferred.
    wanted: bool,
}

/// The seam function of [TYPES-DISCIPLINE-SEAM], once per module. For `hm`
/// the module is returned unchanged, its export signature inferred when a
/// disciplined module imports it; for any other discipline the registered
/// checker runs with the signatures of the module's imports and its erased
/// module is returned under its signature.
fn check_module(
    config: &ProjectConfig,
    registry: &Registry,
    runner: &dyn CheckerRunner,
    source: SourceFile,
    seam: Seam<'_>,
) -> Result<SourceFile, ProjectError> {
    let Seam {
        exports,
        visited,
        wanted,
    } = seam;
    let name = select(config, &source.source);
    if name == HM {
        if wanted {
            exports.record_hm(config, &source, visited)?;
        }
        return Ok(source);
    }
    let discipline = registry
        .get(name)
        .ok_or_else(|| unknown(&source.path, name, &source.source))?;
    let request = protocol::Request::new(&source, config, exports.for_source(&source));
    let response = cache::run_cached(config, discipline, runner, &request)
        .map_err(|message| failure(&source.path, discipline, &message))?;
    let signature = response.signature.clone();
    let erased = erase::erased_source(&source, discipline, response)?;
    exports.record_checked(&erased, name, &signature);
    Ok(erased)
}

fn unknown(path: &Path, name: &str, source: &str) -> ProjectError {
    let line = source
        .lines()
        .position(|line| marker(line).is_some())
        .map(|index| index + 1);
    ProjectError {
        message: format!(
            "{ERROR_PREFIX}: no dependency registers the type discipline `{name}`; \
             add the package that declares `[discipline] name = \"{name}\"` under [dependencies]"
        ),
        path: Some(path.to_path_buf()),
        line,
        column: None,
    }
}

pub(crate) fn failure(path: &Path, discipline: &Discipline, message: &str) -> ProjectError {
    ProjectError {
        message: format!(
            "{ERROR_PREFIX}: checker `{}` of discipline `{}` {message}",
            discipline.package, discipline.name
        ),
        path: Some(path.to_path_buf()),
        line: None,
        column: None,
    }
}
