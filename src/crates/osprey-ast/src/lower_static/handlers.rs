//! Handler forms under the static rewrite. A runtime handler shadows the
//! regions it answers; a compile-time handler is discharged into its body,
//! followed by its return clause and cleanup; a `mask` may not skip a
//! compile-time activation, because discharge leaves nothing to skip.
//! Implements [STAGE-LOWER-ORDER], [STAGE-RESIDUE], [EFFECTS-FINALIZATION] and
//! [EFFECTS-MASK].

use super::region::{answered, shadowed_regions, Region};
use super::Lowering;
use crate::effect_name::covers;
use crate::mutate::children_mut;
use crate::stage::StageError;
use crate::{Expr, HandlerArm};

impl Lowering {
    /// Rewrite under a runtime handler. Its arms and clauses see the enclosing
    /// regions unchanged, since they execute outside their own activation. Its
    /// body sees the same effect's regions with only the supplied operations
    /// shadowed: an uncovered operation still reaches the enclosing static
    /// interpretation ([STAGE-LOWER-ORDER]).
    pub(super) fn enter_runtime_handler(
        &mut self,
        expression: &mut Expr,
        regions: &mut Vec<Region>,
    ) {
        let Expr::Handler {
            effect,
            arms,
            body,
            return_clause,
            finally,
            ..
        } = expression
        else {
            return;
        };
        for arm in arms.iter_mut() {
            self.rewrite(&mut arm.body, regions);
        }
        for clause in return_clause.iter_mut().chain(finally.iter_mut()) {
            self.rewrite(clause, regions);
        }
        let shadows = regions.iter().any(|region| covers(effect, &region.effect));
        let mut visible = shadowed_regions(regions, effect, arms);
        self.shadowed.extend(shadows.then(|| effect.clone()));
        self.rewrite(body, &mut visible);
        if shadows {
            let _ = self.shadowed.pop();
        }
    }

    /// Discharge one region: its body is rewritten with the region pushed, and
    /// the handler node is replaced by that body under its clauses.
    pub(super) fn enter_region(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        let Expr::Handler {
            effect,
            arms,
            body,
            return_clause,
            finally,
            ..
        } = expression
        else {
            return;
        };
        self.regions = self.regions.saturating_add(1);
        let region = Region {
            effect: effect.clone(),
            arms: self.validated_arms(effect, arms, regions),
            id: self.regions,
        };
        let mut discharged = (**body).clone();
        regions.push(region);
        self.rewrite(&mut discharged, regions);
        let _ = regions.pop();
        let clauses = [return_clause.take(), finally.take()];
        *expression = self.completed(discharged, clauses, regions);
    }

    /// A region's arms, each rewritten and validated in the enclosing scope:
    /// unused arms are checked too, and an arm resolves its own requests
    /// outward rather than recursively selecting itself.
    fn validated_arms(
        &mut self,
        effect: &str,
        arms: &[HandlerArm],
        regions: &mut Vec<Region>,
    ) -> Vec<HandlerArm> {
        let mut selected = arms.to_vec();
        for arm in &mut selected {
            self.rewrite(&mut arm.body, regions);
            let required = self.arm_requirements(&arm.body);
            if !required.is_empty() {
                self.errors.push(StageError::new(
                    format!("static handler arm `{effect}.{}` requires runtime effects: {}; arm operations must be statically discharged", arm.operation, required.join(", ")),
                    arm.position,
                ));
            }
        }
        selected
    }

    /// The discharged body under its return clause, then its cleanup.
    fn completed(
        &mut self,
        discharged: Expr,
        clauses: [Option<Box<Expr>>; 2],
        regions: &mut Vec<Region>,
    ) -> Expr {
        let [return_clause, cleanup] = clauses;
        let answered = match return_clause {
            Some(mut clause) => {
                self.rewrite(&mut clause, regions);
                Expr::Call {
                    function: clause,
                    arguments: vec![discharged],
                    named_arguments: Vec::new(),
                }
            }
            None => discharged,
        };
        let Some(mut cleanup) = cleanup else {
            return answered;
        };
        self.rewrite(&mut cleanup, regions);
        crate::handler_value::finalized(answered, *cleanup)
    }

    /// A `mask` whose skipped activation would be a compile-time one is
    /// rejected: discharge removes that activation before the program runs.
    pub(super) fn enter_mask(&mut self, expression: &mut Expr, regions: &mut Vec<Region>) {
        if let Expr::Mask {
            effect, position, ..
        } = expression
        {
            let crossed = answered(regions, effect)
                || self
                    .shadowed
                    .iter()
                    .any(|shadowed| covers(effect, shadowed));
            if crossed {
                self.errors.push(StageError::new(
                    format!("`mask {effect}` would skip a compile-time handler of `{effect}`, which discharge removes before the program runs; mask only runtime handlers"),
                    *position,
                ));
            }
        }
        children_mut(expression, &mut |child| self.rewrite(child, regions));
    }
}
