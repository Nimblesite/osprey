//! Moving a named owner at its last use [GC-ARC-PERCEUS] (Perceus TR §2.2,
//! "ownership transfer at the last use"; docs/specs/compiler/0018-MemoryManagement.md).
//!
//! A binding no path of a statement reads twice, and nothing reads after it,
//! gives up its reference to the record, list, closure or binding that takes
//! the value there. Nothing is counted and nothing is released: the reference
//! the binding held becomes the taker's.
use super::reads::Reads;
use super::{Codegen, Entry, FrameKind, OtherUse};
use osprey_ast::{Expr, Position, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// The bindings the statement being lowered may move, and where it started.
#[derive(Default)]
pub(crate) struct Movable {
    names: BTreeSet<String>,
    /// Movable names whose one read runs on only some paths. Their owner stays
    /// in the ledger: the paths that never take the value still release it.
    sometimes: BTreeSet<String>,
    scope: Option<usize>,
    depth: usize,
    conditionals: usize,
    /// Why each binding the statement reads keeps its value, for diagnostics.
    kept: BTreeMap<String, OtherUse>,
}

/// The value expression a statement evaluates, if it is one that can move.
fn statement_value(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Expr { value, .. } => Some(value),
        _ => None,
    }
}

/// The reads `value` makes, with each read of a borrower counted against every
/// binding it borrows from: storing the borrower stores their value.
fn reads_of(cg: &Codegen, value: &Expr) -> Reads {
    let mut reads = Reads::of(value);
    let borrowed: Vec<(String, u8)> = reads
        .counted()
        .flat_map(|(name, many)| {
            let count = if many { 2 } else { 1 };
            super::borrows::lenders_of(cg, name)
                .into_iter()
                .map(move |lender| (lender, count))
        })
        .collect();
    for (lender, count) in borrowed {
        reads.add(&lender, count);
    }
    reads
}

/// Why each name `reads` mentions keeps its value: some path reads it twice,
/// or it is still read after the statement (`live`).
fn kept(reads: &Reads, live: &BTreeSet<String>) -> BTreeMap<String, OtherUse> {
    reads
        .counted()
        .filter_map(|(name, many)| {
            let other = if many {
                OtherUse::ReadAgainHere
            } else if live.contains(name) {
                OtherUse::ReadLater(None)
            } else {
                return None;
            };
            Some((name.clone(), other))
        })
        .collect()
}

/// `value` is about to be lowered with `live` still read after it in its block:
/// note the bindings it may move. A binding moves only from the scope that made
/// it ([`take`]), so the rest of that block is all that can read it again.
/// The enclosing statement's state waits on a stack for [`end`] to put back.
pub(crate) fn begin(cg: &mut Codegen, value: &Expr, live: &BTreeSet<String>) {
    super::lends::note_lent_closures(cg, value);
    let reads = reads_of(cg, value);
    // What the enclosing statement knows about a binding still holds inside it.
    let mut known = cg.arc.movable.kept.clone();
    if cg.ownership.on {
        known.extend(kept(&reads, live));
    }
    let names: BTreeSet<String> = reads
        .counted()
        .filter(|(name, many)| !many && !live.contains(*name))
        .map(|(name, _)| name.clone())
        .collect();
    let inner = Movable {
        sometimes: names
            .iter()
            .filter(|name| reads.sometimes(name))
            .cloned()
            .collect(),
        names,
        scope: cg.scope_id(),
        depth: cg.arc.frames.len(),
        conditionals: cg.arc.cond_floors.len(),
        kept: known,
    };
    let outer = std::mem::replace(&mut cg.arc.movable, inner);
    cg.arc.enclosing.push(outer);
}

/// What else uses `name`, when the statement being lowered could not move it.
pub(super) fn other_use(cg: &Codegen, name: &str) -> Option<OtherUse> {
    cg.arc.movable.kept.get(name).copied()
}

/// [`begin`] for a block statement followed by `rest` and the block's `tail`.
/// A statement with no value to lower moves nothing.
pub(crate) fn begin_statement<S: std::borrow::Borrow<Stmt>>(
    cg: &mut Codegen,
    stmt: &Stmt,
    rest: &[S],
    tail: Option<(&Expr, Option<Position>)>,
) {
    let Some(value) = statement_value(stmt) else {
        let outer = std::mem::take(&mut cg.arc.movable);
        cg.arc.enclosing.push(outer);
        return;
    };
    let live = super::last_use::live_after(cg, rest, tail.map(|(value, _)| value));
    begin(cg, value, &live);
    place_later_reads(cg, rest);
    if let Some((value, at)) = tail {
        let mut read = BTreeSet::new();
        osprey_ast::freevars::free_idents(value, &mut read);
        place_reads(cg, &read, at);
    }
}

/// Give each binding read later the position of the first statement of `rest`
/// that reads it.
fn place_later_reads<S: std::borrow::Borrow<Stmt>>(cg: &mut Codegen, rest: &[S]) {
    for stmt in rest {
        let mut read = BTreeSet::new();
        osprey_ast::freevars::free_idents_of_stmts(std::slice::from_ref(stmt), None, &mut read);
        place_reads(cg, &read, crate::stmt::stmt_position(stmt.borrow()));
    }
}

/// Every binding read later and not yet placed that `read` names is read `at`.
fn place_reads(cg: &mut Codegen, read: &BTreeSet<String>, at: Option<Position>) {
    for (name, other) in &mut cg.arc.movable.kept {
        if *other == OtherUse::ReadLater(None) && read.contains(name) {
            *other = OtherUse::ReadLater(at);
        }
    }
}

/// The statement is lowered: the enclosing one resumes.
pub(crate) fn end(cg: &mut Codegen) {
    cg.arc.movable = cg.arc.enclosing.pop().unwrap_or_default();
}

impl Movable {
    /// Whether the lowering is still where this statement started: in its
    /// scope, at most its own statement frame deep, and inside no conditional
    /// opened since.
    fn on_every_path(&self, cg: &Codegen) -> bool {
        cg.scope_id() == self.scope
            && cg.arc.frames.len() <= self.depth + 1
            && cg.arc.cond_floors.len() == self.conditionals
    }

    /// Whether the lowering is still inside this statement, on whatever path:
    /// no loop body or other scope frame has opened above it.
    fn encloses(&self, cg: &Codegen) -> bool {
        let above = cg.arc.kinds.get(self.depth..).unwrap_or(&[]);
        above.iter().all(|kind| *kind == FrameKind::Statement)
    }

    /// Where the ledger holds the binding this statement may move whose value
    /// is `operand`, and its name. A binding's owner is in the innermost scope
    /// frame open when the statement began ([`super::push_parent`]) and moves
    /// only from the scope that made it.
    fn find(&self, cg: &Codegen, operand: &str) -> Option<(Place, String)> {
        let scope = self.scope?;
        let below = cg.arc.kinds.get(..self.depth)?;
        let frame = below.iter().rposition(|kind| *kind == FrameKind::Scope)?;
        let at = cg.arc.frames.get(frame)?.iter().rposition(|e| {
            e.operand == operand
                && e.binding_scope == Some(scope)
                && e.name
                    .as_ref()
                    .is_some_and(|name| self.names.contains(name))
        })?;
        let name = cg.arc.frames.get(frame)?.get(at)?.name.clone()?;
        Some(((frame, at), name))
    }

    /// [`Movable::find`] for a path only some runs of this statement take. The
    /// innermost statement gives up there only a binding it reads on such a
    /// path; an enclosing one gives up anything it may move, since everything
    /// nested in it counts as one read of it.
    fn find_on_some_path(
        &self,
        cg: &Codegen,
        operand: &str,
        innermost: bool,
    ) -> Option<(Place, String)> {
        let found = self.find(cg, operand).filter(|_| self.encloses(cg))?;
        (!innermost || self.sometimes.contains(&found.1)).then_some(found)
    }
}

/// A ledger entry's frame and its index there.
type Place = (usize, usize);

/// Take the ledger entry of the movable binding whose value is `operand`, on
/// a path every run of the statement takes.
pub(super) fn take(cg: &mut Codegen, operand: &str) -> Option<Entry> {
    let movable = &cg.arc.movable;
    let (place, name) = movable.find(cg, operand)?;
    if movable.sometimes.contains(&name) || !movable.on_every_path(cg) {
        return None;
    }
    let _ = cg.arc.movable.names.remove(&name);
    Some(cg.arc.frames.get_mut(place.0)?.remove(place.1))
}

/// The statement, innermost first, that may give up the binding whose value
/// is `operand` on the path being lowered: `None` for the innermost, else its
/// index among the enclosing ones.
fn giver(cg: &Codegen, operand: &str) -> Option<(Option<usize>, Place, String)> {
    if let Some((place, name)) = cg.arc.movable.find_on_some_path(cg, operand, true) {
        return Some((None, place, name));
    }
    let enclosing = cg.arc.enclosing.iter().enumerate().rev();
    enclosing.into_iter().find_map(|(level, movable)| {
        let (place, name) = movable.find_on_some_path(cg, operand, false)?;
        Some((Some(level), place, name))
    })
}

/// The slot of the movable binding whose value is `operand`, for the caller to
/// clear: the binding gives its reference up here. On a path only some runs
/// take, its owner stays in the ledger, so the other paths still release it
/// and this one releases the cleared slot, which is nothing.
pub(super) fn give_up(cg: &mut Codegen, operand: &str) -> Option<String> {
    if let Some(entry) = take(cg, operand) {
        return Some(entry.slot);
    }
    let (level, place, name) = giver(cg, operand)?;
    let movable = match level {
        Some(level) => cg.arc.enclosing.get_mut(level)?,
        None => &mut cg.arc.movable,
    };
    let _ = movable.names.remove(&name);
    Some(cg.arc.frames.get(place.0)?.get(place.1)?.slot.clone())
}
