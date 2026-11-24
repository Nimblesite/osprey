//! Owners made on one path only [GC-ARC-PERCEUS]: where a conditional opened
//! in the ledger, so a join can tell what its arms made from what was live
//! before them.
use super::Codegen;

/// The innermost frame's current length — the mark a `match` captures BEFORE
/// generating its arms, so [`move_phi_owners`] can tell arm-produced owners
/// from values that already existed at the join (above all: the scrutinee).
pub(crate) fn frame_mark(cg: &Codegen) -> usize {
    cg.arc.frames.last().map_or(0, Vec::len)
}

/// Record that code emitted from here on runs on only ONE path out of the
/// region, with `mark` owners already live. Pairs with [`close_conditional`]
/// at the join. See [`ArcLedger::cond_floors`].
pub(crate) fn open_conditional(cg: &mut Codegen, mark: usize) {
    let depth = cg.arc.frames.len();
    cg.arc.cond_floors.push((depth, mark));
}

/// Both paths have merged: owners from before the branch are unconditional again.
pub(crate) fn close_conditional(cg: &mut Codegen) {
    let _ = cg.arc.cond_floors.pop();
}

/// The ledger length below which owners predate the innermost conditional open
/// in THIS frame. A frame pushed inside a conditional is wholly contained by
/// it, so its own owners are unconditional within it and the floor is 0.
pub(super) fn conditional_floor(cg: &Codegen) -> usize {
    let depth = cg.arc.frames.len();
    cg.arc
        .cond_floors
        .iter()
        .rev()
        .find(|(d, _)| *d == depth)
        .map_or(0, |(_, mark)| *mark)
}
