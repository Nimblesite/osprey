//! Compiler-resolved source dependencies, including erased constant reads.
//! Implements [PACKAGE-EDITOR-USAGE-GRAPH]. No source text is scanned.
use std::path::PathBuf;

use crate::model::SymbolKey;
use crate::resolve::{Context, Resolver};

/// One resolved declaration reference crossing physical source files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceDependency {
    /// Source whose checked code or imports require the declaration.
    pub source: PathBuf,
    /// Source that owns the resolved declaration.
    pub target: PathBuf,
    /// Fully qualified source identity, preserved before mangling/inlining.
    pub symbol: String,
}

impl Resolver<'_> {
    pub(crate) fn rewrite_method_name(&mut self, name: &mut String, context: &Context) {
        let mut references = std::mem::take(&mut self.source_dependencies);
        self.rewrite_value_name(name, context, false);
        std::mem::swap(&mut self.source_dependencies, &mut references);
        self.method_candidates.extend(references);
    }

    pub(crate) fn tracked(&mut self, key: SymbolKey, context: &Context) -> SymbolKey {
        self.record_reference(&key, context);
        key
    }

    fn record_reference(&mut self, key: &SymbolKey, context: &Context) {
        if key.path.is_empty() {
            let graph = self.graph;
            for &owner in graph.namespaces.get(&key.namespace).into_iter().flatten() {
                self.record_source(key, owner, context);
            }
        } else if let Some(owner) = self.graph.declarations.get(key) {
            self.record_source(key, owner.source, context);
        }
    }

    fn record_source(&mut self, key: &SymbolKey, owner: usize, context: &Context) {
        if context.source == owner {
            return;
        }
        let Some(source) = self.sources.get(context.source) else {
            return;
        };
        let Some(target) = self.sources.get(owner) else {
            return;
        };
        let _inserted = self.source_dependencies.insert(SourceDependency {
            source: source.path.clone(),
            target: target.path.clone(),
            symbol: key.source_name(),
        });
    }

    pub(crate) fn record_imports(&mut self, context: &Context) {
        let keys: Vec<_> = self
            .scopes
            .get(context.contribution)
            .into_iter()
            .flat_map(super::imports::ImportScope::requirements)
            .cloned()
            .collect();
        for key in keys {
            self.record_reference(&key, context);
        }
    }
}
