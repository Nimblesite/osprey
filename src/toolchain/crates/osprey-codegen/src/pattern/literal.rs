//! Literal pattern lowering.
use super::{close_chain, finish_guarded_arm, finish_phi, match_state, open_guarded_arm};
use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::llty::Value;
use osprey_ast::{Expr, MatchArm, Pattern};

/// Literal/catch-all match: compare-and-branch chain joined by a `phi`.
pub(super) fn gen_literal_match(
    cg: &mut Codegen,
    disc: &Value,
    arms: &[MatchArm],
) -> Result<Value> {
    let (end, mut phi_in, last, mark) = match_state(cg, arms);

    for (i, arm) in arms.iter().enumerate() {
        let Pattern::Literal(lit) = &arm.pattern else {
            close_chain(
                cg,
                &arm.pattern,
                arm,
                disc,
                &mut phi_in,
                "destructuring match arm",
            )?;
            break;
        };
        let cond = gen_eq(cg, disc, lit)?;
        let next_lbl = open_guarded_arm(cg, &cond);
        finish_guarded_arm(cg, arm, &mut phi_in, &next_lbl, i == last, |_| {})?;
    }

    finish_phi(cg, &phi_in, &end, mark)
}

/// Equality test between the discriminant and a literal pattern → the `i1`
/// operand.
pub(super) fn gen_eq(cg: &mut Codegen, disc: &Value, lit: &Expr) -> Result<String> {
    let pat = gen_expr(cg, lit)?;
    Ok(crate::expr::gen_comparison(cg, "==", disc.clone(), pat)?.operand)
}
