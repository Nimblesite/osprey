//! Repairs are offered only after the edited live project checks.
//! Implements [LSP-CODE-ACTIONS-MODULES].

mod candidates;

use crate::model::{CodeAction, Span, TextChange};
use crate::project_cache::ProjectCache;
use crate::workspace::{file_path, same_path};
use lspkit_server::{Diagnostic, Severity};
use lspkit_vfs::{DocumentUri, Vfs};
use osprey_project::{ProjectConfig, SourceFile};
use std::ops::Range;

struct Candidate {
    title: &'static str,
    bytes: Range<usize>,
    selection: Range<usize>,
    text: String,
}

struct Inputs {
    config: ProjectConfig,
    current: SourceFile,
    siblings: Vec<SourceFile>,
}

struct Request<'a> {
    vfs: &'a Vfs,
    uri: &'a DocumentUri,
    selection: Span,
    diagnostics: &'a [Diagnostic],
}

pub(crate) fn actions(
    vfs: &Vfs,
    uri: &DocumentUri,
    selection: Span,
    cache: &ProjectCache,
    diagnostics: &[Diagnostic],
) -> Vec<CodeAction> {
    if !diagnostics
        .iter()
        .any(|finding| finding.severity == Severity::Error)
    {
        return Vec::new();
    }
    let Some(inputs) = inputs(vfs, uri, cache) else {
        return Vec::new();
    };
    let request = Request {
        vfs,
        uri,
        selection,
        diagnostics,
    };
    candidates::candidates(&inputs.current)
        .into_iter()
        .filter_map(|candidate| evaluate(&request, &inputs, candidate))
        .collect()
}

fn evaluate(request: &Request<'_>, inputs: &Inputs, candidate: Candidate) -> Option<CodeAction> {
    let source = &inputs.current.source;
    let encoding = request.vfs.encoding();
    let selection = crate::warning_actions::byte_span(source, &candidate.selection, encoding)?;
    if !crate::code_actions::overlaps(request.selection, selection) || !proven(&candidate, inputs) {
        return None;
    }
    let range = crate::warning_actions::byte_span(source, &candidate.bytes, encoding)?;
    action(request, candidate, range)
}

fn action(request: &Request<'_>, candidate: Candidate, range: Span) -> Option<CodeAction> {
    Some(CodeAction {
        title: candidate.title,
        kind: "quickfix",
        uri: request.uri.as_str().to_string(),
        version: request.vfs.version(request.uri)?.get(),
        diagnostics: request
            .diagnostics
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .cloned()
            .collect(),
        edits: vec![TextChange {
            range,
            new_text: candidate.text,
        }],
    })
}

fn inputs(vfs: &Vfs, uri: &DocumentUri, cache: &ProjectCache) -> Option<Inputs> {
    let path = file_path(uri.as_str())?;
    if let Some(result) = cache.sources(uri.as_str(), Some(vfs), None) {
        let snapshot = result.ok()?;
        if snapshot.incomplete || !snapshot.errors.is_empty() {
            return None;
        }
        if let Some(inputs) = project_input(snapshot, &path) {
            return Some(inputs);
        }
    }
    let current = osprey_project::parse_text(path.clone(), vfs.text(uri)?, None).ok()?;
    Some(Inputs {
        config: ProjectConfig::for_root(path.parent()?),
        current,
        siblings: Vec::new(),
    })
}

fn project_input(
    snapshot: crate::project_sources::Sources,
    path: &std::path::Path,
) -> Option<Inputs> {
    let current = snapshot
        .files
        .iter()
        .find(|file| same_path(&file.path, path))?
        .clone();
    let siblings = snapshot
        .files
        .into_iter()
        .filter(|file| !same_path(&file.path, path))
        .collect();
    Some(Inputs {
        config: snapshot.config,
        current,
        siblings,
    })
}

fn proven(candidate: &Candidate, inputs: &Inputs) -> bool {
    let Some(edited) = edited_source(candidate, &inputs.current) else {
        return false;
    };
    let mut sources = inputs.siblings.clone();
    sources.push(edited);
    osprey_project::assemble(&inputs.config, &sources)
        .is_ok_and(|project| osprey_types::check_program(&project.program).is_empty())
}

fn edited_source(candidate: &Candidate, file: &SourceFile) -> Option<SourceFile> {
    let prefix = file.source.get(..candidate.bytes.start)?;
    let suffix = file.source.get(candidate.bytes.end..)?;
    let source = format!("{prefix}{}{suffix}", candidate.text);
    osprey_project::parse_text(file.path.clone(), source, Some(file.flavor)).ok()
}
