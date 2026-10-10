//! Literal pattern lowering.
use super::{finish_guarded_arm, finish_phi, match_state, open_guarded_arm, take_catch_all};
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
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
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                take_catch_all(cg, arm, disc, &mut phi_in)?;
                break;
            }
            Pattern::Literal(lit) => {
                let cond = gen_eq(cg, disc, lit)?;
                let next_lbl = open_guarded_arm(cg, &cond);
                finish_guarded_arm(cg, arm, &mut phi_in, &next_lbl, i == last, |_| {})?;
            }
            _ => return Err(CodegenError::unsupported("destructuring match arm")),
        }
    }

    finish_phi(cg, &phi_in, &end, mark)
}

/// Equality test between the discriminant and a literal pattern → the `i1`
/// operand.
fn gen_eq(cg: &mut Codegen, disc: &Value, lit: &Expr) -> Result<String> {
    let pat = gen_expr(cg, lit)?;
    Ok(crate::expr::gen_comparison(cg, "==", disc.clone(), pat)?.operand)
}
