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
use crate::{walk_program, AstVisitor, Expr, Position, Program, Stage};
use std::collections::BTreeMap;

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
    let mut regions = Regions::default();
    walk_program(program, &mut regions);
    if regions.0.is_empty() {
        return Vec::new();
    }
    let facts = Facts {
        effects: effect_declarations(program),
        runtime: runtime_requirements(program),
    };
    regions
        .0
        .iter()
        .filter_map(|(body, position)| verdict(&facts, body, *position))
        .collect()
}

/// The rejection for one region, or `None` when its body is stage-legal.
fn verdict(facts: &Facts, body: &Expr, position: Option<Position>) -> Option<StageError> {
    let required = body_requirements(&facts.effects, body, Stage::Dynamic, &facts.runtime);
    if required.is_empty() {
        return None;
    }
    Some(StageError::new(
        format!(
            "{KERNEL_REGION_KEYWORD} body is not stage-legal; it requires dynamic effects: {}",
            required.join(", ")
        ),
        position,
    ))
}

/// The body of every `kernel` region, with the region's position.
#[derive(Default)]
struct Regions(Vec<(Expr, Option<Position>)>);

impl AstVisitor for Regions {
    fn expression(&mut self, expression: &Expr) {
        if let Expr::Handler {
            stage: Stage::Kernel,
            body,
            position,
            ..
        } = expression
        {
            self.0.push(((**body).clone(), *position));
        }
    }
}
