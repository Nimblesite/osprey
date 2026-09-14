//! The Default surface of the `kernel` region.
//!
//! `kernel` is a handler region, so it needs no node of its own past the parse:
//! the region is lowered straight into the nested static handler regions it
//! means, with the OUTERMOST carrying [`Stage::Kernel`] so the offload
//! obligation is checked once, at the boundary the source wrote. The ML surface
//! lowers the same shape from its own syntax, which is how one axis serves both
//! languages ([FLAVOR-BOUNDARY]). Implements [STAGE-GPU-KERNEL].

use super::lower::Lowerer;
use crate::kernel::{group_by_effect, is_boundary};
use osprey_ast::{Expr, HandlerArm, Stage};
use tree_sitter::Node;

/// The stage of the region at `index` of a kernel's nested regions.
const fn region_stage(index: usize) -> Stage {
    if is_boundary(index) {
        Stage::Kernel
    } else {
        Stage::Static
    }
}

impl Lowerer<'_> {
    /// `kernel A op => …  B op => … in body` as the regions it means.
    pub(crate) fn lower_kernel(&self, node: Node<'_>) -> Expr {
        let position = Some(self.pos(node));
        let mut region = self.lower_expr_field(node, "body");
        for (index, (effect, arms)) in self.kernel_arms(node).into_iter().enumerate().rev() {
            region = Expr::Handler {
                stage: region_stage(index),
                effect,
                arms,
                body: Box::new(region),
                position,
            };
        }
        region
    }

    /// A kernel's arms grouped by the effect each names, in the order the source
    /// first names it — one kernel answers several device dialects where a
    /// `handle` answers exactly one.
    fn kernel_arms(&self, node: Node<'_>) -> Vec<(String, Vec<HandlerArm>)> {
        group_by_effect(
            self.named_of_kind(node, "kernel_arm")
                .into_iter()
                .map(|arm| (self.field_text(arm, "effect"), self.lower_arm(arm))),
        )
    }
}
