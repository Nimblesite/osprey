//! Deterministic project assembly orchestration.

use crate::contribution;
use crate::{collect, imports, resolve, span};
use crate::{AssembledProject, ProjectConfig, ProjectError, SourceFile, SourceMetadata};
use osprey_ast::{Position, Stmt};
use std::path::Path;

pub(crate) fn assemble(
    config: &ProjectConfig,
    sources: &[SourceFile],
) -> Result<AssembledProject, Vec<ProjectError>> {
    if sources.is_empty() {
        return Err(vec![ProjectError {
            message: "the project contains no Osprey source files".to_string(),
            path: None,
            line: None,
            column: None,
        }]);
    }
    let (rebased, metadata, mut errors) = rebase(sources);
    let entry_source = choose_entry(config, &rebased, &metadata, &mut errors);
    let contributions = contribution::extract(config, &rebased);
    let (graph, mut collection_errors) = collect::collect(&contributions, &metadata);
    errors.append(&mut collection_errors);
    let (scopes, mut import_errors) = imports::build(config, &contributions, &graph, &metadata);
    errors.append(&mut import_errors);
    let Some(entry_source) = entry_source else {
        return Err(errors);
    };
    let mut resolution = resolve::flatten(&contributions, &scopes, &graph, entry_source, &metadata);
    errors.append(&mut resolution.errors);
    if errors.is_empty() {
        Ok(AssembledProject {
            program: resolution.program,
            entry_prologue: resolution.entry_prologue,
            entry_source,
            sources: metadata,
            source_name_by_mangled: resolution.source_names,
        })
    } else {
        Err(errors)
    }
}

fn rebase(sources: &[SourceFile]) -> (Vec<SourceFile>, Vec<SourceMetadata>, Vec<ProjectError>) {
    let mut sorted = sources.to_vec();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));
    let mut offset = 0_u32;
    let mut metadata = Vec::with_capacity(sorted.len());
    let mut errors = Vec::new();
    for (index, source) in sorted.iter_mut().enumerate() {
        let line_count = source.source.lines().count().max(1);
        let Ok(line_count) = u32::try_from(line_count) else {
            errors.push(range_error(&source.path));
            continue;
        };
        let Some(start) = offset.checked_add(1) else {
            errors.push(range_error(&source.path));
            continue;
        };
        let Some(end) = offset.checked_add(line_count) else {
            errors.push(range_error(&source.path));
            continue;
        };
        span::offset_program(&mut source.program, offset);
        metadata.push(SourceMetadata {
            index,
            path: source.path.clone(),
            flavor: source.flavor,
            source: source.source.clone(),
            global_line_start: start,
            global_line_end: end,
        });
        offset = end;
    }
    (sorted, metadata, errors)
}

fn range_error(path: &Path) -> ProjectError {
    ProjectError {
        message: "source line range exceeds the compiler position space".to_string(),
        path: Some(path.to_path_buf()),
        line: None,
        column: None,
    }
}

fn choose_entry(
    config: &ProjectConfig,
    sources: &[SourceFile],
    metadata: &[SourceMetadata],
    errors: &mut Vec<ProjectError>,
) -> Option<usize> {
    if let Some(configured) = &config.entry {
        let candidates = metadata
            .iter()
            .filter(|source| source.path == *configured || source.path.ends_with(configured))
            .map(|source| source.index)
            .collect::<Vec<_>>();
        return unique_entry(&candidates, configured.to_string_lossy().as_ref(), errors);
    }
    let mains = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| has_namespace_main(&source.program.statements))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if !mains.is_empty() {
        return unique_entry(&mains, "a namespace-level `main`", errors);
    }
    let executable = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| has_executable(&source.program.statements))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if !executable.is_empty() {
        return unique_entry(&executable, "top-level executable statements", errors);
    }
    if sources.len() == 1 {
        return Some(0);
    }
    errors.push(ProjectError {
        message: "cannot infer an entry source; configure `[project].entry`".to_string(),
        path: None,
        line: None,
        column: None,
    });
    None
}

fn unique_entry(
    candidates: &[usize],
    description: &str,
    errors: &mut Vec<ProjectError>,
) -> Option<usize> {
    match candidates {
        [entry] => Some(*entry),
        [] => {
            errors.push(ProjectError {
                message: format!("entry source `{description}` was not found"),
                path: None,
                line: None,
                column: None,
            });
            None
        }
        _ => {
            errors.push(ProjectError {
                message: format!("entry source `{description}` is ambiguous"),
                path: None,
                line: None,
                column: None,
            });
            None
        }
    }
}

fn has_namespace_main(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Function { name, .. } => name == "main",
        Stmt::Namespace { body, .. } => has_namespace_main(body),
        _ => false,
    })
}

fn has_executable(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Expr { .. } | Stmt::Assignment { .. } | Stmt::Let { mutable: true, .. } => true,
        Stmt::Namespace { body, .. } => has_executable(body),
        _ => false,
    })
}

pub(crate) fn source_error(
    sources: &[SourceMetadata],
    source: usize,
    position: Option<Position>,
    message: impl Into<String>,
) -> ProjectError {
    let message = message.into();
    if let Some(metadata) = sources.get(source) {
        ProjectError::source(metadata, position, message)
    } else {
        ProjectError {
            message,
            path: None,
            line: None,
            column: None,
        }
    }
}
