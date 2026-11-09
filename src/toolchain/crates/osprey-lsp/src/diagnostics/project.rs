//! Diagnostics for a file that belongs to a project: assembly, sibling and
//! manifest failures reported on the open file.
use super::diagnostic;
use super::warning;
use crate::warning_actions::Analysis;
use crate::workspace::file_path;
use crate::workspace::same_path;
use lspkit::vfs::PositionEncoding;
use lspkit_server::Diagnostic;
use lspkit_server::Severity;
use osprey_ast::Position;
use osprey_project::AssembledProject;
use osprey_project::ProjectError;
use std::path::Path;

pub(super) fn project_diagnostics(
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

pub(super) fn assembled_type_errors(
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
pub(super) fn project_load_errors(
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

pub(super) fn project_errors(
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
