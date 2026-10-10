//! Non-fatal project advice and the resolved state ownership inventory.
//! Implements [MODULES-STYLE] and [MODULES-STATE-INVENTORY].

use crate::contribution::Contribution;
use crate::model::ProjectGraph;
use crate::{ProjectConfig, SourceMetadata};
use osprey_ast::{ModuleKind, Position};
use std::collections::{BTreeMap, BTreeSet};

/// A compiler warning in the assembled program's source-position space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWarning {
    /// Stable rule shared by the CLI and language server.
    pub rule: &'static str,
    /// Source-level explanation; never changes program acceptance.
    pub message: String,
    /// Declaration that owns this warning, before local-file mapping.
    pub position: Position,
}

/// One resolved state owner, including owners behind private module boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateBoundary {
    /// Fully qualified source name of the owner.
    pub name: String,
    /// Declaration position in the assembled program.
    pub position: Position,
    /// Number of private cells, without revealing names or initializers.
    pub private_cells: usize,
    /// Publicly exported effects owned by this module.
    pub effects: Vec<String>,
}

impl StateBoundary {
    /// Identical ownership explanation for diagnostics and generated docs.
    #[must_use]
    pub fn summary(&self) -> String {
        let effects = if self.effects.is_empty() {
            "none".to_string()
        } else {
            self.effects.join(", ")
        };
        format!(
            "state boundary `{}`: {} private cells; exported effects: {effects}",
            self.name, self.private_cells
        )
    }
}

pub(crate) fn boundaries(graph: &ProjectGraph, sources: &[SourceMetadata]) -> Vec<StateBoundary> {
    graph
        .modules
        .iter()
        .filter(|(_, module)| module.kind == ModuleKind::State)
        .map(|(key, module)| StateBoundary {
            name: key.source_name(),
            position: position(module.position, module.source, sources),
            private_cells: module.state_cells.len(),
            effects: module
                .effects
                .intersection(&module.exports)
                .map(|name| key.child(name).source_name())
                .collect(),
        })
        .collect()
}

fn namespace_warnings(
    config: &ProjectConfig,
    contributions: &[Contribution],
    sources: &[SourceMetadata],
) -> Vec<ProjectWarning> {
    let folders = namespace_folders(contributions, sources);
    let mut seen = BTreeSet::new();
    contributions
        .iter()
        .filter(|item| seen.insert((item.source, item.namespace.label())))
        .flat_map(|item| {
            namespace_advice(
                config,
                item,
                folders.get(item.namespace.label()).map_or(0, BTreeSet::len),
                sources,
            )
        })
        .collect()
}

pub(crate) fn warnings(
    config: &ProjectConfig,
    contributions: &[Contribution],
    graph: &ProjectGraph,
    sources: &[SourceMetadata],
    boundaries: &[StateBoundary],
) -> Vec<ProjectWarning> {
    let mut warnings = namespace_warnings(config, contributions, sources);
    if !config.published_library {
        warnings.extend(graph.modules.iter().filter(|(key, _)| key.path.len() > 3)
            .map(|(key, module)| ProjectWarning {
                rule: "module-deep-hierarchy",
                message: format!("module `{}` is nested {} levels deep; prefer shallow application boundaries", key.source_name(), key.path.len()),
                position: position(module.position, module.source, sources),
            }));
    }
    warnings.extend(boundaries.iter().map(|owner| ProjectWarning {
        rule: "state-boundary",
        message: owner.summary(),
        position: owner.position,
    }));
    warnings.sort_by_key(|warning| (warning.position.line, warning.position.column, warning.rule));
    warnings
}

fn namespace_folders<'a>(
    contributions: &'a [Contribution],
    sources: &'a [SourceMetadata],
) -> BTreeMap<&'a str, BTreeSet<&'a std::path::Path>> {
    let mut folders: BTreeMap<&str, BTreeSet<&std::path::Path>> = BTreeMap::new();
    for contribution in contributions {
        if let Some(parent) = sources
            .get(contribution.source)
            .and_then(|source| source.path.parent())
        {
            let _ = folders
                .entry(contribution.namespace.label())
                .or_default()
                .insert(parent);
        }
    }
    folders
}

fn namespace_advice(
    config: &ProjectConfig,
    item: &Contribution,
    folders: usize,
    sources: &[SourceMetadata],
) -> Vec<ProjectWarning> {
    let name = item.namespace.label();
    let at = position(item.position, item.source, sources);
    let mut warnings = Vec::new();
    if folders > 1 {
        warnings.push(ProjectWarning { rule: "namespace-folder-drift", position: at,
            message: format!("namespace `{name}` spans {folders} folders; source paths do not change its identity") });
    }
    if !config.published_library && reverse_domain(name) {
        warnings.push(ProjectWarning { rule: "namespace-reverse-domain", position: at,
            message: format!("namespace `{name}` uses a reverse-domain label in an application; reserve this convention for published libraries") });
    }
    warnings
}

fn reverse_domain(name: &str) -> bool {
    let parts: Vec<_> = name.split('.').collect();
    matches!(
        parts.as_slice(),
        ["com" | "org" | "net" | "io" | "dev" | "edu", _, _, ..]
    ) && parts.iter().all(|part| !part.is_empty())
}

fn position(at: Option<Position>, source: usize, sources: &[SourceMetadata]) -> Position {
    match at {
        Some(at) => at,
        None => Position {
            line: sources
                .get(source)
                .map_or(1, |source| source.global_line_start),
            column: 0,
        },
    }
}
