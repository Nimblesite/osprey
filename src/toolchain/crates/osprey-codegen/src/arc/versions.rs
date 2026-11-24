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
//! In the ledger model [MEM-OWNERSHIP-STATIC-MODEL] the source is moved, and
//! `given_is_gone` and `moved_once` prove it can't be used or derived from again.
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

/// `source` is about to give a list that holds some of its elements, in nodes
/// of its own: `elem` is how those elements are spelled, when known. Elements
/// that are plain numbers have no owner, so nothing is shared; any other
/// element would have two owners while the source lives, so the source is
/// taken as in [`take_source`].
pub(crate) fn take_element_source(cg: &mut Codegen, source: &Value, elem: Option<&str>) -> Source {
    let counted = elem.is_none_or(|elem| crate::llty::elem_of_spelling(elem).0.is_managed_ptr());
    if counted {
        return take_source(cg, source.clone());
    }
    Source {
        value: source.clone(),
        taken: false,
    }
}

/// `literal` is about to be copied into the runtime layout: under ownership a
/// literal the statement made gives its reference to the copy, so each element
/// has one owner again before anything is derived from the copy. A literal
/// something else owns is left alone.
pub(crate) fn take_literal(cg: &mut Codegen, literal: &Value) -> Source {
    let taken = cg.ownership.on
        && super::managed(cg, literal)
        && super::consume_into_store(cg, &literal.operand);
    Source {
        value: literal.clone(),
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
