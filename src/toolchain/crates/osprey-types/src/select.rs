//! Type checking of `select` over channel operations [CONCURRENCY-SELECT].

use crate::check::Checker;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::Type;
use osprey_ast::{MatchArm, SelectOperation};

impl Checker {
    /// Each operation checks like its plain form, a received value binds at the
    /// channel's element type, and the arms join as a `match`'s do.
    pub(crate) fn infer_select(
        &mut self,
        operations: &[SelectOperation],
        arms: &[MatchArm],
        env: &TypeEnv,
    ) -> Type {
        self.check_select_shape(operations);
        let bodies = operations
            .iter()
            .zip(arms)
            .map(|(operation, arm)| {
                let received = self.infer_select_operation(operation, env);
                self.infer_arm(arm, &received, env)
            })
            .collect::<Vec<_>>();
        let result = self.ctx.fresh();
        self.match_result(result, &bodies)
    }

    fn infer_select_operation(&mut self, operation: &SelectOperation, env: &TypeEnv) -> Type {
        match operation {
            SelectOperation::Recv(channel) => self.infer_recv(channel, env),
            SelectOperation::Send { channel, value } => self.infer_send(channel, value, env),
            SelectOperation::Timeout(milliseconds) => {
                let ty = self.infer_expr(milliseconds, env);
                self.push_unify(&Type::int(), &ty);
                Type::unit()
            }
            SelectOperation::Default => Type::unit(),
        }
    }

    fn check_select_shape(&mut self, operations: &[SelectOperation]) {
        let waits = operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation,
                    SelectOperation::Timeout(_) | SelectOperation::Default
                )
            })
            .count();
        if waits == operations.len() {
            self.errors.push(
                TypeError::new("`select` needs at least one `send` or `recv` arm")
                    .with_pos(self.expr_position),
            );
        }
        if waits > 1 {
            self.errors.push(
                TypeError::new(
                    "`select` takes at most one `timeout` or `_` arm: they both decide what happens when no channel is ready",
                )
                .with_pos(self.expr_position),
            );
        }
    }
}
