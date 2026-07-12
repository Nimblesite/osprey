//! Import validation and contribution-local lookup scopes.
//! Implements [MODULES-IMPORT] and its wildcard/ambiguity policy.

use crate::contribution::Contribution;
use crate::model::{ProjectGraph, SymbolKey};
use crate::{ProjectConfig, ProjectError, SourceMetadata};
use osprey_ast::{ImportDecl, ImportMember, ImportSelection, ModuleKind, Position};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub(crate) struct ImportScope {
    aliases: BTreeMap<String, SymbolKey>,
    members: BTreeMap<String, SymbolKey>,
    ambiguous: BTreeSet<String>,
}

impl ImportScope {
    pub fn alias(&self, name: &str) -> Option<&SymbolKey> {
        self.aliases.get(name)
    }

    pub fn member(&self, name: &str) -> Option<&SymbolKey> {
        self.members.get(name)
    }

    pub fn is_ambiguous(&self, name: &str) -> bool {
        self.ambiguous.contains(name)
    }

    pub fn aliases_for(&self, target: &SymbolKey) -> Vec<String> {
        self.aliases
            .iter()
            .filter(|(_, candidate)| *candidate == target)
            .map(|(name, _)| name.clone())
            .collect()
    }
}

pub(crate) fn build(
    config: &ProjectConfig,
    contributions: &[Contribution],
    graph: &ProjectGraph,
    sources: &[SourceMetadata],
) -> (Vec<ImportScope>, Vec<ProjectError>) {
    let mut builder = Builder {
        config,
        graph,
        sources,
        errors: Vec::new(),
    };
    let scopes = contributions
        .iter()
        .map(|contribution| builder.scope(contribution))
        .collect();
    (scopes, builder.errors)
}

struct Builder<'a> {
    config: &'a ProjectConfig,
    graph: &'a ProjectGraph,
    sources: &'a [SourceMetadata],
    errors: Vec<ProjectError>,
}

impl Builder<'_> {
    fn scope(&mut self, contribution: &Contribution) -> ImportScope {
        let mut scope = ImportScope::default();
        for import in &contribution.imports {
            self.import(contribution, import, &mut scope);
        }
        for name in scope
            .aliases
            .keys()
            .filter(|name| scope.members.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>()
        {
            let _ = scope.ambiguous.insert(name);
        }
        for name in &scope.ambiguous {
            self.error(
                contribution.source,
                None,
                format!("ambiguous import binding `{name}`; add an explicit alias"),
            );
        }
        scope
    }

    fn import(
        &mut self,
        contribution: &Contribution,
        import: &ImportDecl,
        scope: &mut ImportScope,
    ) {
        let target = SymbolKey::new(
            import.target.namespace.label(),
            import.target.path.segments.clone(),
        );
        if !self.target_exists(&target) {
            self.error(
                contribution.source,
                import.position,
                format!("unknown import target `{}`", target.source_name()),
            );
            return;
        }
        if !self.target_visible(&target) {
            self.error(
                contribution.source,
                import.position,
                format!("module `{}` is private", target.source_name()),
            );
            return;
        }
        match &import.selection {
            ImportSelection::Whole => {
                let Some(alias) = whole_alias(import) else {
                    self.error(
                        contribution.source,
                        import.position,
                        "a quoted namespace import requires `as Alias`",
                    );
                    return;
                };
                insert_binding(&mut scope.aliases, &mut scope.ambiguous, alias, target);
            }
            ImportSelection::Members(members) => {
                for member in members {
                    self.member(contribution, import, &target, member, scope);
                }
            }
            ImportSelection::Wildcard => self.wildcard(contribution, import, &target, scope),
        }
    }

    fn member(
        &mut self,
        contribution: &Contribution,
        import: &ImportDecl,
        target: &SymbolKey,
        member: &ImportMember,
        scope: &mut ImportScope,
    ) {
        let key = target.child(&member.name);
        if !self.member_visible(target, &key) {
            let description = if self.graph.declarations.contains_key(&key) {
                "private"
            } else {
                "unknown"
            };
            self.error(
                contribution.source,
                import.position,
                format!("{description} imported member `{}`", key.source_name()),
            );
            return;
        }
        insert_binding(
            &mut scope.members,
            &mut scope.ambiguous,
            member.alias.clone().unwrap_or_else(|| member.name.clone()),
            key,
        );
    }

    fn wildcard(
        &mut self,
        contribution: &Contribution,
        import: &ImportDecl,
        target: &SymbolKey,
        scope: &mut ImportScope,
    ) {
        if !self.config.allow_wildcard_imports {
            self.error(
                contribution.source,
                import.position,
                "wildcard imports are disabled by `[modules].allow_wildcard_imports`",
            );
            return;
        }
        let target_is_state = self
            .graph
            .modules
            .get(target)
            .is_some_and(|module| module.kind == ModuleKind::State);
        let includes_state = self.direct_visible_members(target).iter().any(|member| {
            self.graph
                .modules
                .get(member)
                .is_some_and(|module| module.kind == ModuleKind::State)
        });
        if target_is_state || includes_state {
            self.error(
                contribution.source,
                import.position,
                "wildcard imports from state modules are forbidden",
            );
            return;
        }
        for key in self.direct_visible_members(target) {
            let Some(name) = key.path.last().cloned() else {
                continue;
            };
            insert_binding(&mut scope.members, &mut scope.ambiguous, name, key);
        }
    }

    fn target_exists(&self, target: &SymbolKey) -> bool {
        if target.path.is_empty() {
            self.graph.namespaces.contains(&target.namespace)
        } else {
            self.graph.modules.contains_key(target)
        }
    }

    fn target_visible(&self, target: &SymbolKey) -> bool {
        target.path.is_empty()
            || (1..=target.path.len()).all(|length| {
                let prefix = target.path.get(..length).unwrap_or_default().to_vec();
                let key = SymbolKey::new(target.namespace.clone(), prefix);
                self.graph
                    .declarations
                    .get(&key)
                    .is_some_and(|declaration| {
                        declaration.visibility == osprey_ast::Visibility::Exported
                    })
            })
    }

    fn member_visible(&self, target: &SymbolKey, member: &SymbolKey) -> bool {
        let Some(declaration) = self.graph.declarations.get(member) else {
            return false;
        };
        if target.path.is_empty() {
            return declaration.owner.is_empty();
        }
        self.graph.modules.get(target).is_some_and(|module| {
            member
                .path
                .last()
                .is_some_and(|name| module.exports.contains(name))
        })
    }

    fn direct_visible_members(&self, target: &SymbolKey) -> Vec<SymbolKey> {
        self.graph
            .declarations
            .keys()
            .filter(|key| key.namespace == target.namespace)
            .filter(|key| key.parent_path() == target.path)
            .filter(|key| self.member_visible(target, key))
            .cloned()
            .collect()
    }

    fn error(&mut self, source: usize, position: Option<Position>, message: impl Into<String>) {
        if let Some(metadata) = self.sources.get(source) {
            self.errors
                .push(ProjectError::source(metadata, position, message));
        } else {
            self.errors.push(ProjectError {
                message: message.into(),
                path: None,
                line: None,
                column: None,
            });
        }
    }
}

fn whole_alias(import: &ImportDecl) -> Option<String> {
    if let Some(alias) = &import.alias {
        return Some(alias.clone());
    }
    if let Some(last) = import.target.path.last() {
        return Some(last.to_string());
    }
    if import.target.namespace.is_quoted() {
        None
    } else {
        Some(import.target.namespace.label().to_string())
    }
}

fn insert_binding(
    bindings: &mut BTreeMap<String, SymbolKey>,
    ambiguous: &mut BTreeSet<String>,
    name: String,
    target: SymbolKey,
) {
    if let Some(previous) = bindings.get(&name) {
        if previous != &target {
            let _ = ambiguous.insert(name);
        }
    } else {
        let _ = bindings.insert(name, target);
    }
}
