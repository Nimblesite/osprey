//! Canonical names and declaration metadata used while assembling a project.

use osprey_ast::{
    Expr, ModuleKind, Position, SignatureAscription, SignatureItem, Stmt, Visibility,
};
use std::collections::{BTreeMap, BTreeSet};

/// Stable logical identity of a declaration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SymbolKey {
    pub namespace: String,
    pub path: Vec<String>,
}

impl SymbolKey {
    pub fn new(namespace: impl Into<String>, path: Vec<String>) -> Self {
        Self {
            namespace: namespace.into(),
            path,
        }
    }

    pub fn child(&self, name: impl Into<String>) -> Self {
        let mut path = self.path.clone();
        path.push(name.into());
        Self::new(self.namespace.clone(), path)
    }

    pub fn parent_path(&self) -> &[String] {
        self.path
            .get(..self.path.len().saturating_sub(1))
            .unwrap_or_default()
    }

    pub fn source_name(&self) -> String {
        std::iter::once(self.namespace.as_str())
            .chain(self.path.iter().map(String::as_str))
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("::")
    }

    pub fn mangled(&self) -> String {
        let mut out = String::from("__osp");
        push_hex_segment(&mut out, &self.namespace);
        for segment in &self.path {
            push_hex_segment(&mut out, segment);
        }
        out
    }
}

fn push_hex_segment(out: &mut String, segment: &str) {
    use std::fmt::Write as _;
    let _ = write!(out, "_{}x", segment.len());
    for byte in segment.as_bytes() {
        let _ = write!(out, "{byte:02x}");
    }
}

/// Declaration category needed for import/signature validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclKind {
    Function,
    Extern,
    Value,
    Type,
    Constructor,
    Effect,
    Module,
}

/// One declaration registered in the namespace graph.
#[derive(Debug, Clone)]
pub(crate) struct DeclInfo {
    pub kind: DeclKind,
    pub visibility: Visibility,
    pub owner: Vec<String>,
    pub state_owner: bool,
    pub opaque: bool,
}

impl DeclInfo {
    pub fn visible_from(&self, same_namespace: bool, module: &[String]) -> bool {
        self.visibility == Visibility::Exported
            || (same_namespace && module.starts_with(&self.owner))
    }
}

/// Metadata for one closed module boundary.
#[derive(Debug, Clone)]
pub(crate) struct ModuleInfo {
    pub kind: ModuleKind,
    pub signature: Option<SignatureAscription>,
    pub exports: BTreeSet<String>,
    pub explicit_exports: BTreeSet<String>,
    pub state_cells: BTreeSet<String>,
    pub effects: BTreeSet<String>,
    pub source: usize,
    pub position: Option<Position>,
}

/// One immutable declaration eligible for compile-time inlining.
#[derive(Debug, Clone)]
pub(crate) struct ConstantInfo {
    pub value: Expr,
    pub source: usize,
    pub contribution: usize,
    pub module: Vec<String>,
    pub position: Option<Position>,
}

/// Collected declarations and explicit signatures for a project.
#[derive(Debug, Default)]
pub(crate) struct ProjectGraph {
    pub declarations: BTreeMap<SymbolKey, DeclInfo>,
    pub modules: BTreeMap<SymbolKey, ModuleInfo>,
    pub signatures: BTreeMap<SymbolKey, Vec<SignatureItem>>,
    pub constants: BTreeMap<SymbolKey, ConstantInfo>,
    pub annotations: BTreeMap<SymbolKey, SignatureItem>,
    pub namespaces: BTreeSet<String>,
    pub implementations: BTreeMap<SymbolKey, Stmt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangling_is_path_sensitive_and_identifier_safe() {
        // Implements [MODULES-ABI].
        let one = SymbolKey::new("billing/api", vec!["Tax".into(), "add".into()]);
        let two = SymbolKey::new("billing", vec!["api".into(), "Tax".into(), "add".into()]);
        assert_ne!(one.mangled(), two.mangled());
        assert!(one
            .mangled()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'));
        assert_eq!(one.source_name(), "billing/api::Tax::add");
    }
}
