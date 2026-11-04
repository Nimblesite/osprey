//! Capability names, resolved for the effect proof. A `perform`, `handler` or
//! `mask` that names a capability is, to the proof, the capability's effect at
//! an identity that includes the capability's site: the checker already
//! appended that identity to the published arguments, so renaming the target
//! to its effect lets ordinary discharge tell capabilities apart.
//! Implements [EFFECTS-INSTANCE].

use osprey_ast::mutate::{children_mut, statement_children_mut};
use osprey_ast::{Expr, Position, Program};
use std::collections::HashMap;

/// Renaming preserves traversal order but cloning changes every node address.
/// Keep pointer-keyed checker evidence attached to the corresponding node.
pub(super) fn relocated_sites(original: &Program, renamed: &Program) -> HashMap<usize, usize> {
    fn sites(program: &Program) -> Vec<usize> {
        #[derive(Default)]
        struct Sites(Vec<usize>);
        impl osprey_ast::AstVisitor for Sites {
            fn expression(&mut self, expression: &Expr) {
                self.0.push(super::expression_site(expression));
            }
        }
        let mut visitor = Sites::default();
        osprey_ast::walk_program(program, &mut visitor);
        visitor.0
    }
    sites(original).into_iter().zip(sites(renamed)).collect()
}

pub(super) fn relocated_instances(
    instances: &super::Instances,
    sites: &HashMap<usize, usize>,
) -> super::Instances {
    let mut relocated = instances.clone();
    relocated.expression_types =
        relocate_map(relocated.expression_types.into_inner(), sites).into();
    relocated.methods = relocate_map(relocated.methods, sites);
    relocated.instantiations = relocate_map(relocated.instantiations, sites);
    relocated.instantiation_types = relocate_map(relocated.instantiation_types, sites);
    relocated.signal_queries = relocate_map(relocated.signal_queries.into_inner(), sites).into();
    relocated.field_row_violations = std::cell::RefCell::default();
    relocated
}

pub(super) fn relocate_map<T>(
    values: HashMap<usize, T>,
    sites: &HashMap<usize, usize>,
) -> HashMap<usize, T> {
    values
        .into_iter()
        .map(|(site, value)| (sites.get(&site).copied().unwrap_or(site), value))
        .collect()
}

/// `program` with every capability target renamed to its effect, or `None`
/// when no site names a capability.
pub(super) fn resolved(
    program: &Program,
    capabilities: &HashMap<(u32, u32), String>,
) -> Option<Program> {
    if capabilities.is_empty() {
        return None;
    }
    let mut copy = program.clone();
    for statement in &mut copy.statements {
        statement_children_mut(statement, &mut |expression| {
            rename(expression, capabilities);
        });
    }
    Some(copy)
}

fn rename(expression: &mut Expr, capabilities: &HashMap<(u32, u32), String>) {
    if let Some((effect, position)) = target(expression) {
        if let Some(resolved) = position.and_then(|at| capabilities.get(&(at.line, at.column))) {
            effect.clone_from(resolved);
        }
    }
    children_mut(expression, &mut |child| rename(child, capabilities));
}

fn target(expression: &mut Expr) -> Option<(&mut String, Option<Position>)> {
    match expression {
        Expr::Perform {
            effect, position, ..
        }
        | Expr::Handler {
            effect, position, ..
        }
        | Expr::Mask {
            effect, position, ..
        } => Some((effect, *position)),
        _ => None,
    }
}
