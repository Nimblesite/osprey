//! Inferred borrows [MEM-OWNERSHIP-STATIC-BORROWS]
//! (docs/specs/compiler/0018-MemoryManagement.md). Under `--memory=ownership`
//! a `let` that names a value some other binding owns holds no reference of
//! its own: it borrows, and every binding it was read from stays alive for as
//! long as the borrower is read.
use super::{Codegen, FrameKind};
use crate::llty::Value;
use osprey_ast::Expr;
use std::collections::{BTreeMap, BTreeSet};

/// Who lends to whom in the function being lowered.
#[derive(Default)]
pub(crate) struct Borrows {
    /// Borrower name to the bindings its value was read from.
    lenders: BTreeMap<String, BTreeSet<String>>,
    /// Bindings declared `mut`: their value can be replaced, so they never lend.
    rebound: BTreeSet<String>,
}

/// Note the binding `stmt` declares `mut` or assigns.
pub(crate) fn note_rebound(cg: &mut Codegen, stmt: &osprey_ast::Stmt) {
    use osprey_ast::Stmt;
    let (Stmt::Let {
        name,
        mutable: true,
        ..
    }
    | Stmt::Assignment { name, .. }) = stmt
    else {
        return;
    };
    let _ = cg.arc.borrows.rebound.insert(name.clone());
}

/// A `let` bound `v`, the value of `source`, to `name`: give it an owner, or
/// let it borrow when it may.
pub(crate) fn bind_let(cg: &mut Codegen, name: &str, source: &Expr, v: &Value) {
    if !super::managed(cg, v)
        || bind_lent(cg, name, source, v)
        || super::adopt(cg, name, v)
        || lend(cg, name, source)
    {
        return;
    }
    super::bind_counted(cg, name, v);
}

/// `v` is a value `source` constructs that holds values other bindings own:
/// `name` takes `v` and borrows from every binding `source` mentions, which
/// then outlive `name`'s last read.
fn bind_lent(cg: &mut Codegen, name: &str, source: &Expr, v: &Value) -> bool {
    let built = matches!(source, Expr::TypeConstructor { .. });
    if !built || !super::is_lent(cg, v) || !borrow_from(cg, name, source) {
        return false;
    }
    let frame = cg.arc.frames.last_mut();
    let held = frame.and_then(|frame| {
        let at = frame
            .iter()
            .rposition(|e| e.lent && e.operand == v.operand)?;
        Some(frame.remove(at))
    });
    let Some(mut entry) = held else {
        return false;
    };
    entry.name = Some(name.to_owned());
    entry.binding_scope = cg.scope_id();
    super::push_parent(cg, entry);
    true
}

/// Whether the names `source` mentions can lend its value to `name`: record
/// the loan and answer true.
fn lend(cg: &mut Codegen, name: &str, source: &Expr) -> bool {
    cg.ownership.on && !holds_temporaries(cg) && borrow_from(cg, name, source)
}

/// Record `name` as a borrower of every binding `source` mentions. False, with
/// nothing recorded, when one of them can be replaced or none is a binding.
fn borrow_from(cg: &mut Codegen, name: &str, source: &Expr) -> bool {
    let mut mentioned = BTreeSet::new();
    osprey_ast::freevars::free_idents(source, &mut mentioned);
    if mentioned.iter().any(|lender| replaceable(cg, lender)) {
        return false;
    }
    let lenders: BTreeSet<String> = mentioned
        .into_iter()
        .filter(|lender| cg.lookup(lender).is_some())
        .collect();
    if lenders.is_empty() {
        return false;
    }
    cg.arc
        .borrows
        .lenders
        .entry(name.to_owned())
        .or_default()
        .extend(lenders);
    true
}

/// A value the statement itself made dies when the statement ends, so nothing
/// read out of it may be borrowed past that.
fn holds_temporaries(cg: &Codegen) -> bool {
    let statement = cg.arc.kinds.last() == Some(&FrameKind::Statement);
    !statement || cg.arc.frames.last().is_some_and(|frame| !frame.is_empty())
}

/// Whether the value `name` holds can be replaced while a borrower reads it.
fn replaceable(cg: &Codegen, name: &str) -> bool {
    cg.arc.borrows.rebound.contains(name)
        || cg.cell_vars.contains(name)
        || cg.cell_slots.contains_key(name)
        || cg.module_globals.contains_key(name)
}

/// Every binding `borrower` borrows from, through any chain of borrowers.
pub(super) fn lenders_of(cg: &Codegen, borrower: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::from([borrower.to_owned()]);
    keep_lenders(cg, &mut found);
    let _ = found.remove(borrower);
    found
}

/// Every binding a live borrower was read from is live too.
pub(super) fn keep_lenders(cg: &Codegen, live: &mut BTreeSet<String>) {
    let mut pending: Vec<String> = live.iter().cloned().collect();
    while let Some(borrower) = pending.pop() {
        for lender in cg.arc.borrows.lenders.get(&borrower).into_iter().flatten() {
            if live.insert(lender.clone()) {
                pending.push(lender.clone());
            }
        }
    }
}
