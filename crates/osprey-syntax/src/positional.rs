//! `[TYPE-UNION-POSITIONAL]` — the shared positional-constructor table.
//!
//! A variant declared `Node(Tree, Tree)` (Default) / `Node Tree Tree` (ML) has
//! no field names to supply, so its saturated application is a *construction*,
//! not a call. Both frontends collect the same table from their own CST and
//! fold through the same [`construct`], so the two spellings emit the identical
//! [`Expr::TypeConstructor`] required by [FLAVOR-IR-EQUIV].
//!
//! The table holds only constructors declared in the compilation unit being
//! lowered; an imported constructor is absent and keeps the named form, so it
//! is never silently mis-lowered.

use osprey_ast::{Expr, FieldAssignment};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static POSITIONAL_CTORS: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

/// Replace the table with the constructors of the unit about to be lowered.
pub(crate) fn install(entries: impl Iterator<Item = (String, usize)>) {
    POSITIONAL_CTORS.with(|s| {
        let mut table = s.borrow_mut();
        table.clear();
        table.extend(entries);
    });
}

/// Whether a declared variant's fields came from a positional payload, given
/// its field names in declaration order.
pub(crate) fn declares_slots<'f>(mut names: impl Iterator<Item = &'f str>) -> bool {
    names.next().is_some_and(osprey_ast::is_positional_field)
}

/// Fold a saturated application of a positionally-declared constructor into the
/// construction node. An unsaturated or over-applied spine yields `None` and is
/// left as written — constructors do not curry, so the arity mistake is
/// reported by the checker against the call the author actually wrote.
pub(crate) fn construct(name: &str, args: Vec<Expr>) -> Option<Expr> {
    let arity = POSITIONAL_CTORS.with(|s| s.borrow().get(name).copied())?;
    if arity != args.len() {
        return None;
    }
    Some(Expr::TypeConstructor {
        name: name.to_owned(),
        type_args: Vec::new(),
        fields: args
            .into_iter()
            .enumerate()
            .map(|(slot, value)| FieldAssignment {
                name: osprey_ast::positional_field_name(slot),
                value,
            })
            .collect(),
    })
}
