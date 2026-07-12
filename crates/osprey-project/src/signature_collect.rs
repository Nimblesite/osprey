//! Signature lookup, conformance, export, and opacity finalization.

use crate::collect::Collector;
use crate::collect_support::{kind_matches, signature_name_kind};
use crate::model::{ModuleInfo, SymbolKey};
use crate::ProjectError;
use osprey_ast::{Position, SignatureItem, Visibility};
use std::collections::BTreeSet;

impl Collector<'_> {
    pub(crate) fn finish_signatures(&mut self) {
        let module_keys = self.graph.modules.keys().cloned().collect::<Vec<_>>();
        for module_key in module_keys {
            let Some(module) = self.graph.modules.get(&module_key).cloned() else {
                continue;
            };
            let Some(ascription) = &module.signature else {
                continue;
            };
            let Some(items) = self.find_signature(&module_key, &ascription.path.segments) else {
                self.error(
                    module.source,
                    module.position,
                    format!("unknown signature `{}`", ascription.path),
                );
                continue;
            };
            self.apply_signature(&module_key, &module, &items, ascription.allow_extra);
        }
    }

    fn find_signature(&self, module: &SymbolKey, path: &[String]) -> Option<Vec<SignatureItem>> {
        let direct = SymbolKey::new(module.namespace.clone(), path.to_vec());
        if let Some(items) = self.graph.signatures.get(&direct) {
            return Some(items.clone());
        }
        let mut relative = module.parent_path().to_vec();
        relative.extend_from_slice(path);
        self.graph
            .signatures
            .get(&SymbolKey::new(module.namespace.clone(), relative))
            .cloned()
    }

    fn apply_signature(
        &mut self,
        module_key: &SymbolKey,
        module: &ModuleInfo,
        items: &[SignatureItem],
        allow_extra: bool,
    ) {
        let mut exports = BTreeSet::new();
        for item in items {
            self.apply_item(module_key, module, item, &mut exports);
        }
        let extras = module
            .explicit_exports
            .difference(&exports)
            .cloned()
            .collect::<Vec<_>>();
        for extra in extras {
            if allow_extra {
                let _ = exports.insert(extra);
            } else {
                self.error(
                    module.source,
                    module.position,
                    format!("extra export `{extra}` requires `+ extra`"),
                );
            }
        }
        if let Some(info) = self.graph.modules.get_mut(module_key) {
            info.exports = exports;
        }
    }

    fn apply_item(
        &mut self,
        module_key: &SymbolKey,
        module: &ModuleInfo,
        item: &SignatureItem,
        exports: &mut BTreeSet<String>,
    ) {
        let (name, expected) = signature_name_kind(item);
        let declaration = module_key.child(name);
        let Some(info) = self.graph.declarations.get(&declaration).cloned() else {
            self.error(
                module.source,
                module.position,
                format!("signature item `{name}` has no implementation"),
            );
            return;
        };
        if !kind_matches(info.kind, expected) {
            self.error(
                module.source,
                module.position,
                format!("signature item `{name}` has the wrong declaration kind"),
            );
            return;
        }
        if info.state_owner {
            self.error(
                module.source,
                module.position,
                format!("state cell `{name}` cannot be exported by a signature"),
            );
            return;
        }
        if let Some(implementation) = self.graph.implementations.get(&declaration) {
            for message in crate::contract::errors(implementation, item) {
                self.error(module.source, module.position, message);
            }
        }
        if let Some(info) = self.graph.declarations.get_mut(&declaration) {
            info.visibility = Visibility::Exported;
            info.opaque |= crate::contract::hides_type(item);
        }
        let _ = exports.insert(name.to_string());
        let _ = self.graph.annotations.insert(declaration, item.clone());
    }

    pub(crate) fn error(
        &mut self,
        source: usize,
        position: Option<Position>,
        message: impl Into<String>,
    ) {
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
