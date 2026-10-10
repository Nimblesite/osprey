//! In-process diagnostics.
//!
//! The TypeScript server wrote each edit to a temp file, shelled out to the
//! `osprey` binary, and scraped stderr with a wall of regexes. Here the
//! compiler front-end is called directly: [`osprey_syntax::parse_program`] for
//! syntax errors and [`osprey_types::check_program`] for type errors, mapped to
//! the [`lspkit_server::Diagnostic`] the diagnostics bus fans out.
//! Implements [LSP-DIAGNOSTICS].

use lspkit_server::{Diagnostic, Severity};
use lspkit_vfs::PositionEncoding;
use osprey_ast::{Position, Program};
use osprey_project::{AssembledProject, ProjectError};
use std::path::{Path, PathBuf};

// URI/path resolution and project discovery live once, in `workspace`: the
// editor's idea of which files form a program must be the compiler's.
use crate::workspace::{file_path, same_path};

use crate::warning_actions::Analysis;

const SOURCE: &str = "osprey";

/// Compute diagnostics for `source`. The document `path` selects the flavor
/// (`.ospml` ⇒ ML), so a layout-flavor file is parsed by its own frontend
/// instead of misreported as broken Default syntax. Syntax errors are reported
/// alone (an unparsable file is not type-checked, matching the CLI gate); a clean
/// parse is then type-checked.
#[cfg(test)]
#[must_use]
pub(crate) fn compute(source: &str, path: &str, encoding: PositionEncoding) -> Vec<Diagnostic> {
    analyze(source, path, encoding).diagnostics
}

#[cfg(test)]
pub(crate) fn analyze(source: &str, path: &str, encoding: PositionEncoding) -> Analysis {
    analyze_live(source, path, encoding, None)
}

#[cfg(test)]
pub(crate) fn analyze_live(
    source: &str,
    path: &str,
    encoding: PositionEncoding,
    vfs: Option<&lspkit_vfs::Vfs>,
) -> Analysis {
    analyze_cached(
        source,
        path,
        encoding,
        vfs,
        &crate::project_cache::ProjectCache::default(),
    )
}

pub(crate) fn analyze_cached(
    source: &str,
    path: &str,
    encoding: PositionEncoding,
    vfs: Option<&lspkit_vfs::Vfs>,
    cache: &crate::project_cache::ProjectCache,
) -> Analysis {
    let snapshot = cache.sources(path, vfs, Some(source));
    let view = snapshot
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map_or_else(crate::workspace::View::default, |sources| {
            sources.view(path)
        });
    // [FLAVOR-SELECT] makes a marker/extension disagreement a hard error, and
    // the CLI refuses to build such a file. Resolve FIRST and report the
    // conflict as the document's only finding: guessing a flavor would parse
    // the file with the wrong frontend and bury the real fault under a cascade
    // of phantom syntax and type errors.
    let flavor = match osprey_syntax::resolve_flavor(view.configured, path, source) {
        Ok(flavor) => flavor,
        Err(message) => {
            return vec![diagnostic(
                source,
                marker_position(source),
                &message,
                "flavor-error",
                encoding,
            )]
            .into();
        }
    };
    let parsed = view.parsed(path, source);
    if !parsed.errors.is_empty() {
        return parsed
            .errors
            .iter()
            .map(|e| diagnostic(source, e.position, &e.message, "syntax-error", encoding))
            .collect::<Vec<_>>()
            .into();
    }
    let mut analysis = project_diagnostics(source, path, encoding, snapshot, cache)
        .or_else(|| standalone_diagnostics(source, path, flavor, &parsed.program, encoding))
        .unwrap_or_else(|| type_diagnostics(source, &parsed.program, flavor, encoding));
    analysis
        .diagnostics
        .extend(skip_diagnostics(source, &parsed.program, encoding));
    analysis
}

/// One diagnostic per statically-skipped test case, on the `test` call's own
/// line ([TESTING-SKIP-WARNING-STATIC]): a skipped test must be loud in the
/// editor, so these ride alongside whatever errors the file already has. An
/// unparsable file never reaches here — it reports its syntax error alone.
///
/// A skip that names a reason is a Warning; one that names none is an Error
/// ([TESTING-SKIP-REASON]). Both carry the same `test-skipped` code, because
/// they are the same rule reported at two strengths.
fn skip_diagnostics(
    source: &str,
    program: &Program,
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    crate::testing::collect_tests(program)
        .iter()
        .filter_map(|case| {
            let report = case.skip_diagnostic()?;
            let pos = case.position.unwrap_or(Position { line: 1, column: 0 });
            let build = if report.unexplained {
                diagnostic
            } else {
                warning
            };
            Some(build(
                source,
                pos,
                &report.message,
                "test-skipped",
                encoding,
            ))
        })
        .collect()
}

/// Where to underline a flavor conflict: the `// osprey: flavor=` marker line
/// itself (1-based), since that is the half of the disagreement the author can
/// edit in this buffer. Falls back to line 1 when no marker is present — which
/// `resolve_flavor` only errors without if the marker names an unknown flavor.
fn marker_position(source: &str) -> Position {
    let line = source
        .lines()
        .position(|l| l.trim_start().starts_with("//") && l.contains("osprey: flavor="))
        .and_then(|i| u32::try_from(i + 1).ok())
        .unwrap_or(1);
    Position { line, column: 0 }
}

fn type_diagnostics(
    source: &str,
    program: &Program,
    flavor: osprey_syntax::Flavor,
    encoding: PositionEncoding,
) -> Analysis {
    let diagnostics: Vec<_> = osprey_types::check_program(program)
        .iter()
        .map(|e| {
            diagnostic(
                source,
                e.position.unwrap_or_default(),
                &e.message,
                "type-error",
                encoding,
            )
        })
        .collect();
    let mut analysis = Analysis::from(diagnostics);
    if analysis.diagnostics.is_empty() {
        analysis.add_warnings(source, program, flavor, encoding, &Some);
    }
    analysis
}

/// Single-source assembly for a module-bearing file that no project claims —
/// a standalone script or a file outside the manifest's `source_roots` (test
/// suites in `test/`). Mirrors the CLI's `--check`/`osprey test` path so the
/// editor never reports `unknown identifier` for the file's own modules.
/// `None` for ordinary module-free scripts, which skip assembly entirely.
fn standalone_diagnostics(
    source: &str,
    uri: &str,
    flavor: osprey_syntax::Flavor,
    program: &Program,
    encoding: PositionEncoding,
) -> Option<Analysis> {
    if !osprey_project::needs_assembly(program) {
        return None;
    }
    let file = file_path(uri).unwrap_or_else(|| PathBuf::from(uri));
    let source_file = osprey_project::SourceFile {
        path: file.clone(),
        flavor,
        source: source.to_string(),
        program: program.clone(),
    };
    Some(assembly_diagnostics(
        osprey_project::assemble_one(source_file),
        source,
        &file,
        encoding,
    ))
}

/// Map an assembly outcome to diagnostics for the open `file`: type errors on
/// success, project errors on failure.
fn assembly_diagnostics(
    assembled: Result<AssembledProject, Vec<ProjectError>>,
    source: &str,
    file: &Path,
    encoding: PositionEncoding,
) -> Analysis {
    match assembled {
        Ok(project) => {
            let errors = osprey_types::check_program(&project.program);
            let warnings = if errors.is_empty() {
                crate::warning_actions::Warnings::of(&project.program)
            } else {
                crate::warning_actions::Warnings::none()
            };
            assembled_type_errors(source, file, &project, &errors, &warnings, encoding)
        }
        Err(errors) => project_errors(source, file, &errors, encoding).into(),
    }
}

fn project_diagnostics(
    source: &str,
    uri: &str,
    encoding: PositionEncoding,
    snapshot: Option<Result<crate::project_sources::Sources, Vec<ProjectError>>>,
    cache: &crate::project_cache::ProjectCache,
) -> Option<Analysis> {
    let file = file_path(uri)?;
    let snapshot = match snapshot? {
        Ok(snapshot) => snapshot,
        Err(errors) => return Some(project_load_errors(source, &file, &errors, encoding).into()),
    };
    if !snapshot.errors.is_empty() {
        return Some(project_load_errors(source, &file, &snapshot.errors, encoding).into());
    }
    let config = snapshot.config;
    let sources = snapshot.files;
    let incomplete = snapshot.incomplete;
    if !sources
        .iter()
        .any(|candidate| same_path(&candidate.path, &file))
    {
        return None;
    }
    let shared = cache.checked(&snapshot.root, &config, &sources);
    let mut analysis = match &shared.assembled {
        Ok(project) => assembled_type_errors(
            source,
            &file,
            project,
            &shared.errors,
            &shared.warnings,
            encoding,
        ),
        Err(errors) => project_errors(source, &file, errors, encoding).into(),
    };
    if incomplete {
        // Disk fallback proves errors, but cannot justify edits to live code.
        analysis.fixes.clear();
        analysis
            .diagnostics
            .retain(|diagnostic| diagnostic.severity == Severity::Error);
    }
    Some(analysis)
}

fn assembled_type_errors(
    source: &str,
    file: &Path,
    project: &AssembledProject,
    errors: &[osprey_types::TypeError],
    warnings: &crate::warning_actions::Warnings,
    encoding: PositionEncoding,
) -> Analysis {
    let diagnostics: Vec<_> = errors
        .iter()
        .filter_map(|error| {
            let position = if let Some(global) = error.position {
                let (owner, line) = project.source_at_line(global.line)?;
                same_path(&owner.path, file).then_some(Position {
                    line,
                    column: global.column,
                })?
            } else {
                let is_entry = project
                    .entry()
                    .is_some_and(|entry| same_path(&entry.path, file));
                is_entry.then_some(Position { line: 1, column: 0 })?
            };
            Some(diagnostic(
                source,
                position,
                &error.message,
                "type-error",
                encoding,
            ))
        })
        .collect();
    let mut analysis = Analysis::from(diagnostics);
    if errors.is_empty() {
        // [LSP-MODULE-ADVICE]: retain the compiler's rule and physical owner.
        analysis
            .diagnostics
            .extend(project.warnings.iter().filter_map(|advice| {
                let (owner, line) = project.source_at_line(advice.position.line)?;
                same_path(&owner.path, file).then(|| {
                    warning(
                        source,
                        Position {
                            line,
                            column: advice.position.column,
                        },
                        &advice.message,
                        advice.rule,
                        encoding,
                    )
                })
            }));
        if let Some(metadata) = project
            .sources
            .iter()
            .find(|owner| same_path(&owner.path, file))
        {
            analysis.add_project_warnings(source, warnings, metadata.flavor, encoding, &|global| {
                let (owner, line) = project.source_at_line(global.line)?;
                same_path(&owner.path, file).then_some(Position {
                    line,
                    column: global.column,
                })
            });
        }
    }
    analysis
}

/// Loading failures in closed siblings or the manifest must not clear the
/// open file's diagnostics and make an unanalyzable project appear healthy.
fn project_load_errors(
    source: &str,
    file: &Path,
    errors: &[ProjectError],
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    errors
        .iter()
        .flat_map(|error| {
            if error
                .path
                .as_ref()
                .is_none_or(|owner| same_path(owner, file))
            {
                return project_errors(source, file, std::slice::from_ref(error), encoding);
            }
            let owner = error
                .path
                .as_ref()
                .map_or_else(String::new, |path| path.display().to_string());
            let line = error
                .line
                .map_or_else(String::new, |line| format!(":{line}"));
            let message = format!("cannot analyze project: {owner}{line}: {}", error.message);
            vec![diagnostic(
                source,
                Position { line: 1, column: 0 },
                &message,
                "project-error",
                encoding,
            )]
        })
        .collect()
}

fn project_errors(
    source: &str,
    file: &Path,
    errors: &[ProjectError],
    encoding: PositionEncoding,
) -> Vec<Diagnostic> {
    errors
        .iter()
        .filter(|error| {
            error
                .path
                .as_deref()
                .is_none_or(|path| same_path(path, file))
        })
        .map(|error| {
            let position = Position {
                line: error
                    .line
                    .and_then(|line| u32::try_from(line).ok())
                    .unwrap_or(1),
                column: error
                    .column
                    .and_then(|column| u32::try_from(column).ok())
                    .unwrap_or(0),
            };
            diagnostic(source, position, &error.message, "project-error", encoding)
        })
        .collect()
}

/// Build one error diagnostic spanning the offending line from `pos` onward.
fn diagnostic(
    source: &str,
    pos: Position,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    ranged(source, pos, Severity::Error, message, code, encoding)
}

/// Build one warning diagnostic spanning the offending line from `pos` onward.
pub(crate) fn warning(
    source: &str,
    pos: Position,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    ranged(source, pos, Severity::Warning, message, code, encoding)
}

fn ranged(
    source: &str,
    pos: Position,
    severity: Severity,
    message: &str,
    code: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    let line = pos.line.saturating_sub(1);
    let line_text = nth_line(source, line);
    // `pos.column` is a tree-sitter byte offset; re-measure the line prefix in
    // the selected encoding before it crosses the wire. [LSP-ENCODING]
    let start = byte_col_to_encoding(line_text, pos.column, encoding);
    let end = line_text
        .map_or(0, |l| crate::text::measure(l, encoding))
        .max(start.saturating_add(1));
    Diagnostic::new(severity, message, (line, start, line, end))
        .with_source(SOURCE)
        .with_code(code)
}

/// Zero-based `line`'s text, or `None` if absent.
fn nth_line(source: &str, line: u32) -> Option<&str> {
    usize::try_from(line)
        .ok()
        .and_then(|i| source.lines().nth(i))
}

/// Convert a byte column within `line` into `encoding`'s character units.
fn byte_col_to_encoding(line: Option<&str>, byte_col: u32, encoding: PositionEncoding) -> u32 {
    let Some(line) = line else {
        return byte_col;
    };
    let idx = usize::try_from(byte_col).unwrap_or(usize::MAX);
    line.get(..idx)
        .map_or(byte_col, |prefix| crate::text::measure(prefix, encoding))
}

#[cfg(test)]
pub(crate) fn assert_redundant_annotations(
    actual: &[Diagnostic],
    expected: &[(&str, crate::model::Span)],
) {
    let expected: Vec<_> = expected
        .iter()
        .map(|(message, range)| ("redundant-annotation", *message, *range))
        .collect();
    assert_warnings(actual, &expected);
}

#[cfg(test)]
fn assert_warnings(actual: &[Diagnostic], expected: &[(&str, &str, crate::model::Span)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (diagnostic, (rule, message, range)) in actual.iter().zip(expected) {
        assert_eq!(diagnostic.severity, Severity::Warning, "{diagnostic:?}");
        assert_eq!(diagnostic.code.as_deref(), Some(*rule));
        assert_eq!(diagnostic.source.as_deref(), Some("osprey"));
        assert_eq!(diagnostic.message, *message);
        assert_eq!(diagnostic.range, *range, "{diagnostic:?}");
    }
}

#[cfg(test)]
#[path = "diagnostics/tests.rs"]
mod tests;
