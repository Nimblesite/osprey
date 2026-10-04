//! Type checking: statements.
use super::{unify, unify_assignable, Checker, Discard, Position, Stmt, Type, TypeEnv, TypeError};

impl Checker {
    pub(super) fn record_err(&mut self, e: TypeError, pos: Option<Position>) {
        self.errors.push(e.with_pos(pos));
    }

    /// Unify and record any failure. Shared by the expr/pattern modules.
    pub(crate) fn push_unify(&mut self, a: &Type, b: &Type) {
        if let Err(e) = unify(&mut self.ctx, a, b) {
            self.errors.push(e);
        }
    }

    /// Directional assignment-site unification, recording failures. This may
    /// wrap a bare value in Success but never erase a Result error channel.
    pub(crate) fn push_assign(&mut self, expected: &Type, actual: &Type) {
        if let Err(e) = unify_assignable(&mut self.ctx, expected, actual) {
            self.errors.push(e);
        }
    }

    /// Check a statement appearing inside a block expression, threading new
    /// bindings into the block's local scope.
    pub(crate) fn infer_block_stmt(&mut self, s: &Stmt, env: &mut TypeEnv) {
        match s {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
                position,
                ..
            } => self.check_let(name, *mutable, ty.as_ref(), value, env, *position),
            Stmt::Assignment {
                name,
                value,
                position,
            } => self.check_assignment(name, value, env, *position),
            Stmt::Expr {
                value, position, ..
            } => {
                // A statement is evaluated for its effects and its value is
                // thrown away, so only `Unit` may stand here. Judging that
                // needs the final substitution — a body-local variable can be
                // constrained further down — so the type is banked for
                // [`Checker::validate_discards`]. Implements [BLOCK-DISCARD].
                let inferred = self.infer_expr(value, env);
                self.discards.push(Discard::implicit(inferred, *position));
            }
            _ => {}
        }
    }
}
