//! Ledger regions [GC-ARC-PERCEUS]: a statement or a scope opens a frame, and
//! the owners still in it are released when it closes.
use super::{Codegen, Entry, RELEASE_DECL};

/// What a ledger frame delimits. A statement's owners die when the statement
/// closes; a scope — the function body, a loop body, a kernel or comparator
/// body — is where a binding made inside it lives. [GC-ARC-PERCEUS]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameKind {
    Statement,
    Scope,
}

/// Open a region of the given kind.
pub(crate) fn push_frame(cg: &mut Codegen, kind: FrameKind) {
    cg.arc.frames.push(Vec::new());
    cg.arc.kinds.push(kind);
}

/// Close the innermost region: drop every owner it still holds.
pub(crate) fn pop_frame(cg: &mut Codegen) {
    let _ = cg.arc.kinds.pop();
    if let Some(frame) = cg.arc.frames.pop() {
        release_entries(cg, &frame);
    }
}

/// Drop the innermost region's owners on an edge that LEAVES the region early,
/// without closing it — a `filter` rejecting an element branches straight to
/// the loop increment, past the fall-through region close, so the values the
/// map stages produced this iteration would otherwise be abandoned. Safe to
/// pair with the fall-through drop of the same entries: `release_entries`
/// re-nulls each slot and `osp_release(null)` is a no-op. [GC-ARC-PERCEUS]
pub(crate) fn drop_frame_inline(cg: &mut Codegen) {
    if let Some(frame) = cg.arc.frames.pop() {
        release_entries(cg, &frame);
        cg.arc.frames.push(frame);
    }
}

/// Emit slot-based drops for `entries`, newest first, re-nulling each slot
/// (loop bodies reuse slots across iterations).
pub(super) fn release_entries(cg: &mut Codegen, entries: &[Entry]) {
    for e in entries.iter().rev() {
        cg.add_extern(RELEASE_DECL);
        let live = cg.emit_reg(format!("load i8*, i8** {}", e.slot));
        cg.emit(format!("call void @osp_release(i8* {live})"));
        cg.emit(format!("store i8* null, i8** {}", e.slot));
    }
}
