//! The ML surface of the `kernel` region.
//!
//! `kernel` is a handler region, so it needs no node of its own: the region is
//! parsed here and desugared straight into the nested [`MlExpr::Handle`] regions
//! it means, with the OUTERMOST carrying [`Stage::Kernel`] so the offload
//! obligation is checked once, at the boundary the source wrote. Both flavors
//! therefore reach the canonical AST through the same shape rather than through
//! two — the requirement of [FLAVOR-BOUNDARY]. Implements [STAGE-GPU-KERNEL].

use osprey_ast::stage::KERNEL_REGION_KEYWORD;
use osprey_ast::{Position, Stage};

use crate::kernel::{group_by_effect, is_boundary};

use super::cst::{MlExpr, MlHandleArm};
use super::parser::Parser;
use super::token::TokKind;

/// The grouped arms as the nested regions they mean, outermost first, with the
/// OUTERMOST marked [`Stage::Kernel`] so the offload obligation is checked once.
fn nest(grouped: Vec<(String, Vec<MlHandleArm>)>, body: MlExpr, pos: Position) -> MlExpr {
    let mut region = body;
    for (index, (effect, arms)) in grouped.into_iter().enumerate().rev() {
        region = MlExpr::Handle {
            stage: region_stage(index),
            effect,
            arms,
            body: Box::new(region),
            pos,
        };
    }
    region
}

/// The stage of the region at `index` of a kernel's nested regions.
const fn region_stage(index: usize) -> Stage {
    if is_boundary(index) {
        Stage::Kernel
    } else {
        Stage::Static
    }
}

impl Parser<'_> {
    /// Whether the current token opens a `kernel` region. Contextual on the
    /// same terms as the `static` stage marker: only the indented arm block
    /// that must follow makes `kernel` a region rather than an ordinary name.
    pub(super) fn at_kernel_region(&self) -> bool {
        matches!(self.peek(), TokKind::Ident(word) if word == KERNEL_REGION_KEYWORD)
            && *self.peek_at(1) == TokKind::Indent
    }

    /// `kernel` + indented `Effect op param* => body` arms + `in body`.
    pub(super) fn kernel_expr(&mut self) -> MlExpr {
        let pos = self.pos();
        self.advance(); // `kernel`
        let grouped = self.kernel_arms();
        self.skip_separators();
        if !self.eat(&TokKind::KwIn) {
            self.error("expected 'in' after kernel arms");
        }
        nest(grouped, self.body_after_eq(), pos)
    }

    /// The region's arms grouped by the effect each names, in the order the
    /// source first names it — one kernel answers several device dialects where
    /// a `handle` answers exactly one.
    fn kernel_arms(&mut self) -> Vec<(String, Vec<MlHandleArm>)> {
        group_by_effect(self.kernel_arm_lines())
    }

    /// The region's `Effect op param* => body` lines, in source order.
    fn kernel_arm_lines(&mut self) -> Vec<(String, MlHandleArm)> {
        let mut lines = Vec::new();
        if !self.eat(&TokKind::Indent) {
            return lines;
        }
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            let before = self.position_index();
            lines.push((self.ident().unwrap_or_default(), self.handle_arm()));
            if self.position_index() == before {
                self.recover();
            }
        }
        let _ = self.eat(&TokKind::Dedent);
        lines
    }
}
