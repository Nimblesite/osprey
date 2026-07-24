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
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static POSITIONAL_CTORS: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Holds the table alive for one program lowering and tears it down after, so
/// a later unrelated unit never folds against a stale constructor.
pub(crate) struct Scope {
    outermost: bool,
}

impl Drop for Scope {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        if self.outermost {
            POSITIONAL_CTORS.with(|s| s.borrow_mut().clear());
        }
    }
}

/// Install the constructors of the unit about to be lowered, for as long as the
/// returned [`Scope`] lives.
///
/// Only the *outermost* lowering installs; a nested one inherits the table it
/// finds. Lowering re-enters itself: a Default interpolation fragment is
/// re-parsed as a whole mini-program mid-lowering (`parse_fragment`), and that
/// fragment declares no types — so letting it install would clear the table the
/// enclosing program is still being lowered against, and `"${Node(l, r)}"`
/// would fold differently from the identical expression outside the string.
pub(crate) fn install(entries: impl Iterator<Item = (String, usize)>) -> Scope {
    let outermost = DEPTH.with(|d| {
        let depth = d.get();
        d.set(depth.saturating_add(1));
        depth == 0
    });
    if outermost {
        POSITIONAL_CTORS.with(|s| {
            let mut table = s.borrow_mut();
            table.clear();
            table.extend(entries);
        });
    }
    Scope { outermost }
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
