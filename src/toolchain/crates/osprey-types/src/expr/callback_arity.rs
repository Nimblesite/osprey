//! A lambda passed where its callee takes a function of another arity. The error
//! names the callee and both counts, which is also the fix for a `use` line,
//! whose binders are that lambda's parameters. Implements [BLOCK-USE-REJECT].
use crate::check::Checker;
use crate::error::TypeError;
use crate::ty::Type;
use osprey_ast::Expr;

impl Checker {
    /// Reports and returns `true` when a lambda argument takes a different
    /// number of parameters than the function the callee declares in its slot.
    pub(super) fn callback_arity_mismatch(
        &mut self,
        callee_name: Option<&str>,
        callee: &Type,
        arguments: &[Expr],
    ) -> bool {
        let Type::Fun { params: slots, .. } = self.ctx.prune(callee) else {
            return false;
        };
        let found = slots.iter().zip(arguments).find_map(|(slot, argument)| {
            let Expr::Lambda {
                parameters,
                position,
                ..
            } = argument
            else {
                return None;
            };
            match self.ctx.prune(slot) {
                Type::Fun { params, .. } if params.len() != parameters.len() => {
                    Some((params.len(), parameters.len(), *position))
                }
                _ => None,
            }
        });
        let Some((passed, taken, position)) = found else {
            return false;
        };
        let callee = callee_name.map_or_else(|| "this call".to_owned(), |name| format!("`{name}`"));
        self.errors.push(
            TypeError::new(format!(
                "{callee} passes {passed} {} to its callback, but this callback takes {taken}: take {passed}, or bind {passed} with `use`",
                values(passed)
            ))
            .with_pos(position),
        );
        true
    }
}

fn values(count: usize) -> &'static str {
    match count {
        1 => "value",
        _ => "values",
    }
}
