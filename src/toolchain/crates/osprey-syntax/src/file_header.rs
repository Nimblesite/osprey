//! Leading `osprey: key=value` directives and the namespace/module
//! contribution they give a file, for the flavors whose host syntax has no
//! spelling for either. Implements [FLAVOR-PYTHON-INTEROP], [FLAVOR-TS-INTEROP].
use crate::SyntaxError;
use osprey_ast::{
    stmt_meta, DocComment, ModuleItem, ModuleKind, NamespaceName, Position, Stmt, SymbolPath,
    Visibility,
};
use std::collections::BTreeMap;

/// The directives read from a file's leading comments.
#[derive(Default)]
pub(crate) struct Directives {
    pub(crate) namespace: Option<(String, Position)>,
    pub(crate) module: Option<(String, Position)>,
    seen: BTreeMap<String, Position>,
}

fn error(position: Position, message: impl Into<String>) -> SyntaxError {
    SyntaxError {
        message: message.into(),
        position,
    }
}

pub(crate) fn identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_alphabetic() || ch == '_')
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

impl Directives {
    /// Record one `key=value` directive written at `position`.
    pub(crate) fn directive(
        &mut self,
        directive: &str,
        position: Position,
    ) -> Result<(), SyntaxError> {
        let Some((key, value)) = directive.split_once('=') else {
            return Err(error(position, "expected `osprey: key=value`"));
        };
        let (key, value) = (key.trim(), value.trim());
        if self.seen.insert(key.into(), position).is_some() {
            return Err(error(position, format!("duplicate {key} directive")));
        }
        let slot = match key {
            "flavor" => {
                return value
                    .parse::<crate::Flavor>()
                    .map(|_| ())
                    .map_err(|message| error(position, message))
            }
            "namespace" if identifier(value) => &mut self.namespace,
            "module" if value.split('.').all(identifier) => &mut self.module,
            _ => return Err(error(position, "unknown or malformed Osprey directive")),
        };
        *slot = Some((value.into(), position));
        Ok(())
    }

    /// A module is a member of a namespace, so it cannot be declared alone.
    pub(crate) fn check(&self, position: Position) -> Result<(), SyntaxError> {
        if self.module.is_some() && self.namespace.is_none() {
            return Err(error(
                position,
                "a module directive requires a namespace directive",
            ));
        }
        Ok(())
    }

    pub(crate) fn namespace_contribution(&self, statements: Vec<Stmt>) -> Vec<Stmt> {
        match &self.namespace {
            Some((name, position)) => vec![Stmt::Namespace {
                name: NamespaceName::Identifier(name.clone()),
                body: statements,
                file_scoped: true,
                doc: None,
                inner_doc: None,
                position: Some(*position),
            }],
            None => statements,
        }
    }

    /// Wrap the file's declarations in its module, leaving imports outside.
    pub(crate) fn module_contribution(
        &self,
        statements: Vec<Stmt>,
        exported: impl Fn(&str) -> bool,
        inner_doc: Option<DocComment>,
    ) -> Vec<Stmt> {
        let Some((path, position)) = &self.module else {
            return statements;
        };
        let (mut result, declarations): (Vec<_>, Vec<_>) = statements
            .into_iter()
            .partition(|statement| matches!(statement, Stmt::Import(_)));
        let body = declarations
            .into_iter()
            .map(|statement| module_item(statement, &exported))
            .collect();
        result.push(Stmt::Module {
            path: SymbolPath::new(path.split('.')),
            kind: ModuleKind::Plain,
            signature: None,
            body,
            doc: None,
            inner_doc,
            position: Some(*position),
        });
        result
    }
}

fn module_item(statement: Stmt, exported: &impl Fn(&str) -> bool) -> ModuleItem {
    let visibility = if stmt_meta::declaration_name(&statement).is_some_and(exported) {
        Visibility::Exported
    } else {
        Visibility::Private
    };
    ModuleItem {
        visibility,
        opaque: false,
        declaration: Box::new(statement),
    }
}
