//! Or-pattern binding. Each alternative is typed against the discriminant as
//! if it were the only pattern there, and every alternative binds the same
//! names at one type. Implements [PATTERN-OR-TYPE] and [PATTERN-OR-BINDINGS].

use super::pattern_binders_where;
use crate::check::Checker;
use crate::env::TypeEnv;
use crate::error::TypeError;
use crate::ty::Type;
use osprey_ast::Pattern;

impl Checker {
    /// Bind the first alternative into `local`; bind each later one from the
    /// same outer scope, then hold its binders to the first's names and types.
    pub(super) fn bind_alternatives(
        &mut self,
        alternatives: &[Pattern],
        disc: &Type,
        local: &mut TypeEnv,
    ) {
        let Some((first, rest)) = alternatives.split_first() else {
            return;
        };
        let outer = local.clone();
        self.bind_pattern(first, disc, local);
        let chosen = self.alternative_binders(first);
        for alternative in rest {
            let mut own = outer.clone();
            self.bind_pattern(alternative, disc, &mut own);
            self.agree_with_first(&chosen, alternative, local, &own);
        }
    }

    /// The names an alternative binds: not `_`, not a payload-free variant.
    fn alternative_binders(&self, pattern: &Pattern) -> Vec<String> {
        pattern_binders_where(pattern, &|name| name != "_" && !self.names_variant(name))
    }

    /// A name one side binds and the other does not is rejected, naming the
    /// alternative; a name both bind unifies its two types, so a mismatch is
    /// an ordinary type error at the later binder.
    fn agree_with_first(
        &mut self,
        chosen: &[String],
        alternative: &Pattern,
        first: &TypeEnv,
        own: &TypeEnv,
    ) {
        let theirs = self.alternative_binders(alternative);
        let shown = self.render_pattern(alternative);
        for name in chosen.iter().filter(|name| !theirs.contains(name)) {
            self.errors.push(TypeError::new(format!(
                "`{name}` is not bound by the alternative `{shown}`; every alternative of an or-pattern binds the same names"
            )));
        }
        for name in theirs.iter().filter(|name| !chosen.contains(name)) {
            self.errors.push(TypeError::new(format!(
                "`{name}` is bound only by the alternative `{shown}`; every alternative of an or-pattern binds the same names"
            )));
        }
        for name in theirs.iter().filter(|name| chosen.contains(name)) {
            if let (Some(expected), Some(found)) = (first.get(name), own.get(name)) {
                let (expected, found) = (expected.ty.clone(), found.ty.clone());
                self.push_unify(&expected, &found);
            }
        }
    }
}
