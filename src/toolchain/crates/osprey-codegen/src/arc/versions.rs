//! Persistent collections under compile-time ownership
//! [MEM-OWNERSHIP-STATIC-SHARING] (docs/specs/compiler/0018-MemoryManagement.md).
//!
//! An operation that derives a new version of a list or map shares the
//! source's nodes and elements with that version. Two live versions would give
//! each of them two owners, so the source is taken by move: a fresh value or a
//! binding at its last use gives its one reference to the operation, and its
//! release follows the call at once, handing every shared node and element to
//! the new version. A source read again, or one the function only borrows, is
//! refused. The collector and reference counting keep their own path.
use super::{Codegen, Need};
use crate::llty::Value;

/// The source of a derived version, and whether its reference was taken.
pub(crate) struct Source {
    pub(crate) value: Value,
    taken: bool,
}

/// `source` is about to give a new version of itself: under ownership take its
/// one reference, or refuse it as a second live version.
pub(crate) fn take_source(cg: &mut Codegen, source: Value) -> Source {
    if !cg.ownership.on || !super::managed(cg, &source) {
        return Source {
            value: source,
            taken: false,
        };
    }
    let taken = super::consume_into_store(cg, &source.operand);
    if !taken {
        super::second_owner(cg, &source.operand, None, Need::Versioned);
    }
    Source {
        value: source,
        taken,
    }
}

/// The new version exists: release the source it was derived from, whose
/// reference was taken, so its nodes and elements have one owner again.
pub(crate) fn release_source(cg: &mut Codegen, source: &Source) {
    if source.taken {
        let pointer = super::as_i8ptr(cg, &source.value);
        super::release_operand(cg, &pointer);
    }
}
