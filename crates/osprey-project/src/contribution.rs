//! Conversion from per-file syntax trees into open namespace contributions.

use crate::{ProjectConfig, SourceFile};
use osprey_ast::{ImportDecl, NamespaceName, Stmt};

/// One file's declarations contributed to one logical namespace.
#[derive(Debug, Clone)]
pub(crate) struct Contribution {
    pub source: usize,
    pub namespace: NamespaceName,
    pub imports: Vec<ImportDecl>,
    pub statements: Vec<Stmt>,
}

pub(crate) fn extract(config: &ProjectConfig, sources: &[SourceFile]) -> Vec<Contribution> {
    sources
        .iter()
        .enumerate()
        .flat_map(|(index, source)| from_file(config, index, &source.program.statements))
        .collect()
}

fn from_file(config: &ProjectConfig, source: usize, statements: &[Stmt]) -> Vec<Contribution> {
    let mut out = Vec::new();
    let mut unscoped = Vec::new();
    let file_imports: Vec<ImportDecl> = statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Import(import) => Some(import.clone()),
            _ => None,
        })
        .collect();
    for statement in statements {
        match statement {
            Stmt::Namespace { name, body, .. } => {
                out.push(build(source, name.clone(), body.clone(), &file_imports));
            }
            Stmt::Import(_) => {}
            other => unscoped.push(other.clone()),
        }
    }
    if !unscoped.is_empty() {
        out.push(build(source, default_name(config), unscoped, &file_imports));
    }
    out
}

fn build(
    source: usize,
    namespace: NamespaceName,
    statements: Vec<Stmt>,
    file_imports: &[ImportDecl],
) -> Contribution {
    let mut imports = file_imports.to_vec();
    let mut declarations = Vec::new();
    for statement in statements {
        match statement {
            Stmt::Import(import) => imports.push(import),
            other => declarations.push(other),
        }
    }
    Contribution {
        source,
        namespace,
        imports,
        statements: declarations,
    }
}

fn default_name(config: &ProjectConfig) -> NamespaceName {
    NamespaceName::Identifier(
        config
            .default_namespace
            .clone()
            .unwrap_or_else(|| config.name.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use osprey_ast::Program;
    use osprey_syntax::Flavor;
    use std::path::{Path, PathBuf};

    #[test]
    fn explicit_namespaces_are_independent_of_paths() {
        // Implements [MODULES-NAMESPACE], [MODULES-PATH-INDEPENDENCE].
        let source = SourceFile {
            path: PathBuf::from("deep/wrong/place.osp"),
            flavor: Flavor::Default,
            source: String::new(),
            program: Program {
                statements: vec![Stmt::Namespace {
                    name: NamespaceName::Quoted("billing/api".into()),
                    body: Vec::new(),
                    file_scoped: true,
                    position: None,
                }],
            },
        };
        let found = extract(&ProjectConfig::for_root(Path::new("app")), &[source]);
        assert!(found
            .first()
            .is_some_and(|item| item.namespace.label() == "billing/api"));
    }
}
