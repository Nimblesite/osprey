//! State validation path identities, diagnostics, patterns, and cell injection.

use crate::model::SymbolKey;
use crate::{ProjectError, SourceMetadata};
use osprey_ast::{Expr, Pattern, Position, Stmt};
use std::collections::BTreeSet;

pub(crate) fn pattern_names(pattern: &Pattern, names: &mut BTreeSet<String>) {
    match pattern {
        Pattern::Constructor {
            fields,
            sub_patterns,
            ..
        } => {
            names.extend(fields.iter().cloned());
            for nested in sub_patterns {
                pattern_names(nested, names);
            }
        }
        Pattern::TypeAnnotated { name, .. } | Pattern::Binding(name) => {
            let _ = names.insert(name.clone());
        }
        Pattern::Structural { fields } => names.extend(fields.iter().cloned()),
        Pattern::List { elements, rest } => {
            for nested in elements {
                pattern_names(nested, names);
            }
            names.extend(rest.iter().cloned());
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

pub(crate) fn inject(body: &mut Expr, cells: &[Stmt]) {
    if let Expr::Block { statements, .. } = body {
        let mut prefixed = cells.to_vec();
        prefixed.append(statements);
        *statements = prefixed;
    } else {
        let original = std::mem::replace(body, Expr::Bool(false));
        *body = Expr::Block {
            statements: cells.to_vec(),
            value: Some(Box::new(original)),
        };
    }
}

pub(crate) fn owned_effect_names(
    module: &SymbolKey,
    effects: &BTreeSet<String>,
    aliases: &[String],
) -> BTreeSet<String> {
    let relative_prefix = module.path.join("::");
    let absolute_prefix = absolute_prefix(module);
    let mut names = BTreeSet::new();
    for effect in effects {
        let _ = names.insert(effect.clone());
        let _ = names.insert(format!("{relative_prefix}::{effect}"));
        let _ = names.insert(format!("{absolute_prefix}::{effect}"));
        names.extend(aliases.iter().map(|alias| format!("{alias}::{effect}")));
    }
    names
}

pub(crate) fn owned_cell_names(
    module: &SymbolKey,
    cells: &BTreeSet<String>,
    aliases: &[String],
) -> BTreeSet<String> {
    let relative_prefix = module.path.join("::");
    let absolute_prefix = absolute_prefix(module);
    let mut names = BTreeSet::new();
    for cell in cells {
        let _ = names.insert(format!("{relative_prefix}::{cell}"));
        let _ = names.insert(format!("{absolute_prefix}::{cell}"));
        names.extend(aliases.iter().map(|alias| format!("{alias}::{cell}")));
    }
    names
}

fn absolute_prefix(module: &SymbolKey) -> String {
    std::iter::once(module.namespace.as_str())
        .chain(module.path.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("::")
}

pub(crate) fn push_error(
    errors: &mut Vec<ProjectError>,
    sources: &[SourceMetadata],
    source: usize,
    position: Option<Position>,
    message: impl Into<String>,
) {
    if let Some(metadata) = sources.get(source) {
        errors.push(ProjectError::source(metadata, position, message));
    } else {
        errors.push(ProjectError {
            message: message.into(),
            path: None,
            line: None,
            column: None,
        });
    }
}
