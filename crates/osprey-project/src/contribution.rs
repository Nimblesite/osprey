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
    for statement in statements {
        match statement {
            Stmt::Namespace { name, body, .. } => {
                out.push(build(source, name.clone(), body.clone()));
            }
            other => unscoped.push(other.clone()),
        }
    }
    if !unscoped.is_empty() {
        out.push(build(source, default_name(config), unscoped));
    }
    out
}

fn build(source: usize, namespace: NamespaceName, statements: Vec<Stmt>) -> Contribution {
    let mut imports = Vec::new();
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
        assert_eq!(found[0].namespace.label(), "billing/api");
    }
}
