//! Types of the binders that are neither a `let` nor a named function's
//! parameter: match-arm pattern binders and lambda parameters. Patterns carry
//! no source position of their own, so each binder is keyed by its name and the
//! arm or lambda that introduces it, which opens the binder's lexical region.
//! Implements [LSP-HOVER-BINDERS].

use crate::ty::Type;
use osprey_ast::Position;

/// The arm or lambda that introduces a binder, the binder's name and its type.
type Binder = (Position, String, Type);

/// Every recorded binder, in inference order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Binders(Vec<Binder>);

impl Binders {
    pub(crate) fn record(&mut self, region: Position, name: &str, ty: &Type) {
        self.0.push((region, name.to_owned(), ty.clone()));
    }

    /// The same binders with every type rewritten by `resolve` (the final
    /// substitution).
    pub(crate) fn resolved(&self, mut resolve: impl FnMut(&Type) -> Type) -> Self {
        Self(
            self.0
                .iter()
                .map(|(region, name, ty)| (*region, name.clone(), resolve(ty)))
                .collect(),
        )
    }

    /// The type of the binder `name` introduced nearest before `declared`
    /// and no earlier than `region`; a body inferred twice answers with its
    /// final pass.
    fn get(&self, name: &str, region: Position, declared: Position) -> Option<&Type> {
        let key = |at: &Position| (at.line, at.column);
        self.0
            .iter()
            .filter(|(at, binder, _)| {
                binder == name && (key(&region)..=key(&declared)).contains(&key(at))
            })
            .max_by_key(|(at, _, _)| key(at))
            .map(|(_, _, ty)| ty)
    }
}

impl crate::info::ProgramTypes {
    /// The inferred type of the pattern binder or lambda parameter `name`
    /// written at `declared`, whose lexical region — the match arm or lambda
    /// that introduces it — starts at `region`.
    /// Implements [LSP-HOVER-BINDERS].
    #[must_use]
    pub fn binder_type(&self, name: &str, region: Position, declared: Position) -> Option<&Type> {
        self.binders.get(name, region, declared)
    }
}
