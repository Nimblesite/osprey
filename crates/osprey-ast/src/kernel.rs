//! The `kernel` region's own obligation: a body with an empty residual dynamic
//! row.
//!
//! A `kernel` is a handler region like any other — it supplies the STATIC
//! handlers for the device dialects its body uses, and `stage` discharges it by
//! the same rewrite ([STAGE-GPU-KERNEL]). What it adds is the offload boundary:
//! device code cannot leave the device to reach a runtime handler, so a request
//! its arms did not answer is a request nothing can answer. Naming those
//! operations is the whole rule — a kernel that requests only static effects is
//! legal, which is the generalisation [STAGE-GPU-LEGAL] makes over the empty
//! row [GPU-KERNEL-PURE] demanded. Implements [STAGE-GPU-LEGAL], [STAGE-GPU-DIAG].

use crate::stage::{
    body_requirements, effect_declarations, runtime_requirements, EffectDecl, StageError,
    KERNEL_REGION_KEYWORD,
};
use crate::{AstNode, Expr, Position, Program, Stage, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// What one `kernel` boundary is decided against: the program's effect
/// declarations and every function's residual runtime row. Both are
/// whole-program facts, so they are built once and read by every region.
struct Facts {
    effects: BTreeMap<String, EffectDecl>,
    runtime: BTreeMap<String, Vec<String>>,
}

/// Every `kernel` region in `program` whose body still requires a dynamic
/// operation, as one error apiece naming the operations that forced it.
pub(crate) fn legality(program: &Program) -> Vec<StageError> {
    let regions = regions(program);
    if regions.is_empty() {
        return Vec::new();
    }
    let facts = Facts {
        effects: effect_declarations(program),
        runtime: runtime_requirements(program),
    };
    regions
        .iter()
        .filter_map(|region| verdict(&facts, region))
        .collect()
}

/// The rejection for one region, or `None` when its body is stage-legal.
fn verdict(facts: &Facts, region: &Region) -> Option<StageError> {
    let mut required =
        body_requirements(&facts.effects, &region.body, Stage::Dynamic, &facts.runtime);
    if crate::stage_rows::invokes_parameter(&region.body, &region.unresolved) {
        required.push("<unknown>".to_owned());
    }
    if required.is_empty() {
        return None;
    }
    required.sort();
    required.dedup();
    Some(StageError::new(
        format!(
            "{KERNEL_REGION_KEYWORD} body is not stage-legal; it requires dynamic effects: {}",
            required.join(", ")
        ),
        region.position,
    ))
}

/// A kernel's body and the enclosing callable parameters whose effect rows
/// have not been published yet.
struct Region {
    body: Expr,
    position: Option<Position>,
    unresolved: BTreeSet<String>,
}

fn regions(program: &Program) -> Vec<Region> {
    let mut found = Vec::new();
    let mut pending: Vec<_> = program
        .statements
        .iter()
        .map(|statement| (AstNode::Statement(statement), BTreeSet::new()))
        .collect();
    while let Some((node, mut unresolved)) = pending.pop() {
        match node {
            AstNode::Statement(Stmt::Function {
                parameters, body, ..
            }) => {
                unresolved.extend(parameters.iter().map(|parameter| parameter.name.clone()));
                unresolved = crate::stage_rows::parameter_aliases(body, &unresolved);
            }
            AstNode::Statement(Stmt::Let {
                value: Expr::Lambda {
                    parameters, body, ..
                },
                ..
            }) => {
                unresolved.extend(parameters.iter().map(|parameter| parameter.name.clone()));
                unresolved = crate::stage_rows::parameter_aliases(body, &unresolved);
            }
            AstNode::Expression(Expr::Lambda {
                parameters, body, ..
            }) => {
                unresolved.extend(parameters.iter().map(|parameter| parameter.name.clone()));
                unresolved = crate::stage_rows::parameter_aliases(body, &unresolved);
            }
            _ => {}
        }
        if let AstNode::Expression(
            expression @ Expr::Handler {
                stage: Stage::Kernel,
                position,
                ..
            },
        ) = node
        {
            found.push(Region {
                body: expression.clone(),
                position: *position,
                unresolved: unresolved.clone(),
            });
        }
        node.for_each_child(|child| pending.push((child, unresolved.clone())));
    }
    found
}
