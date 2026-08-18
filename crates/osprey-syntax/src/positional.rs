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

#[cfg(test)]
mod tests {
    use osprey_ast::{Expr, InterpolatedPart, Program, Stmt};

    /// Slot names of the `Node` construction bound to `name`, looking through
    /// one level of `${…}` interpolation. `None` when the binding is absent or
    /// its value is not a construction at all — the shape a lost table
    /// produces, since an unfolded spine stays an [`Expr::Call`].
    fn node_slots<'p>(program: &'p Program, name: &str) -> Option<Vec<&'p str>> {
        let bound = program.statements.iter().find_map(|stmt| match stmt {
            Stmt::Let {
                name: declared,
                value,
                ..
            } if declared == name => Some(value),
            _ => None,
        })?;
        let expr = match bound {
            Expr::InterpolatedStr(parts) => parts.iter().find_map(|part| match part {
                InterpolatedPart::Expr(expr) => Some(expr),
                InterpolatedPart::Text(_) => None,
            })?,
            other => other,
        };
        match expr {
            Expr::TypeConstructor {
                name: ctor, fields, ..
            } if ctor == "Node" => Some(fields.iter().map(|f| f.name.as_str()).collect()),
            _ => None,
        }
    }

    /// Lowering re-enters itself: a `${…}` fragment is re-parsed as a whole
    /// mini-program, and that program declares no types. Without
    /// [`super::install`]'s outermost guard the fragment's empty table would
    /// replace the enclosing unit's, so the identical `Node(Leaf, Leaf)` would
    /// fold to a construction outside a string and stay a call inside one —
    /// and, because the fragment's [`super::Scope`] clears on drop, stay a call
    /// for every statement after it too.
    #[test]
    fn an_interpolated_fragment_folds_against_the_enclosing_units_constructors() {
        let parsed = crate::parse_program(concat!(
            "type Tree = Leaf | Node(Tree, Tree)\n",
            "let before = Node(Leaf, Leaf)\n",
            "let inside = \"${Node(Leaf, Leaf)}\"\n",
            "let after = Node(Leaf, Leaf)\n",
        ));
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let slots = Some(vec!["0", "1"]);
        assert_eq!(node_slots(&parsed.program, "before"), slots, "before");
        assert_eq!(node_slots(&parsed.program, "inside"), slots, "inside");
        assert_eq!(node_slots(&parsed.program, "after"), slots, "after");
    }

    /// The other half of the scope: the table belongs to the unit being
    /// lowered, not to the thread. Both parses run on this one test thread, so
    /// a table that outlived its [`super::Scope`] would fold the second unit's
    /// undeclared `Node` against the first unit's declaration.
    #[test]
    fn a_units_table_does_not_outlive_its_lowering() {
        let declared = crate::parse_program(concat!(
            "type Tree = Leaf | Node(Tree, Tree)\n",
            "let t = Node(Leaf, Leaf)\n",
        ));
        assert_eq!(node_slots(&declared.program, "t"), Some(vec!["0", "1"]));
        let undeclared = crate::parse_program("let t = Node(Leaf, Leaf)\n");
        assert_eq!(node_slots(&undeclared.program, "t"), None, "stale table");
    }
}
