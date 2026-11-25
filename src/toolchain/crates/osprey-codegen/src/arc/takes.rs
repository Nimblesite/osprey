//! Taking a field out of a value [MEM-OWNERSHIP-STATIC-PATHS]
//! (docs/specs/compiler/0018-MemoryManagement.md). Under `--memory=ownership`
//! a value this function made and never named has one owner, so what it holds
//! has one owner too. A field read out of it, as in `make(n).name`, can then
//! take the reference the value held: the field's slot is cleared, and the
//! value is freed later without that field. This is the owned projection of
//! Counting Immutable Beans §5 and the drop specialization of Perceus §2.3.
//!
//! In the ledger model [MEM-OWNERSHIP-STATIC-MODEL] a take is `move` from the
//! field's slot. Proved there: the taker becomes the one owner
//! (`take_moves_owner`), the holder may not free through the slot it gave up
//! (`taken_field_not_freed`, hence the cleared slot), nothing may still borrow
//! the value (`take_needs_no_loan`, hence the last-read checks), and it is
//! freed once (`taken_freed_once`). A part taken out of a named holder
//! outlives it (`part_outlives_holder`), and one taken on some paths only is
//! freed once on each (`take_on_one_path`).
use super::{Codegen, FrameKind};
use std::collections::BTreeMap;

/// Where a register's value was read from.
#[derive(Clone)]
pub(crate) struct FieldRead {
    /// The value that holds it, by the register the ledger knows it under.
    container: String,
    /// The address of the slot it was loaded from, and that slot's type.
    slot: String,
    ty: String,
}

/// Field reads of the function being lowered, by loaded register.
pub(crate) type FieldReads = BTreeMap<String, FieldRead>;

/// `loaded` was read from the slot `slot`, of type `ty`, of `container`.
pub(crate) fn note_field(cg: &mut Codegen, loaded: &str, container: &str, slot: &str, ty: &str) {
    if !cg.ownership.on {
        return;
    }
    let read = FieldRead {
        container: super::aliases::root(cg, container).to_owned(),
        slot: slot.to_owned(),
        ty: ty.to_owned(),
    };
    let _ = cg.arc.fields.insert(loaded.to_owned(), read);
    // A word read from a slot is followed through its unboxing.
    super::same_value(cg, loaded, loaded);
}

/// Whether nothing reads the binding that names `operand` again: it has no
/// name, or the statement being lowered holds its one remaining read.
fn at_last_read(cg: &Codegen, operand: &str) -> bool {
    cg.name_of(operand).is_none_or(|name| last_read(cg, &name))
}

/// Whether nothing reads the binding `name` again: the statement being lowered
/// holds its one remaining read.
fn last_read(cg: &Codegen, name: &str) -> bool {
    let movable = &cg.arc.movable;
    movable.splits_it(name)
        || (movable.names_it(name) && super::moves::other_use(cg, name).is_none())
}

/// Whether the ledger holds `operand` as a value this function owns outright
/// and nothing reads again. One it never named is in the region that runs once
/// per pass through here: no loop body has opened since. One it named, a
/// binding or an owned parameter, is at its last read.
fn owned_at_last_read(cg: &Codegen, operand: &str) -> bool {
    let scope = cg
        .arc
        .kinds
        .iter()
        .rposition(|kind| *kind == FrameKind::Scope);
    let inner = scope.unwrap_or(0);
    let frames = cg.arc.frames.iter().enumerate();
    frames.into_iter().any(|(depth, frame)| {
        frame.iter().any(|e| {
            e.operand == operand
                && !e.lent
                && match &e.name {
                    Some(name) => last_read(cg, name),
                    None => depth >= inner,
                }
        })
    })
}

/// The read that `operand` came from, when the field can be taken: every value
/// between it and an owned temporary is at its last read.
fn takeable(cg: &Codegen, operand: &str) -> Option<(String, FieldRead)> {
    let key = super::aliases::root(cg, operand).to_owned();
    let read = cg.arc.fields.get(&key)?.clone();
    if !cg.ownership.on || !at_last_read(cg, operand) || !at_last_read(cg, &key) {
        return None;
    }
    let mut container = read.container.as_str();
    while let Some(outer) = cg.arc.fields.get(container) {
        if !at_last_read(cg, container) {
            return None;
        }
        container = outer.container.as_str();
    }
    owned_at_last_read(cg, container).then_some((key, read))
}

/// [`take_field`] for the value a function returns. Its body's tail is already
/// lowered, so what that tail may move is put back for the question.
pub(super) fn take_returned(cg: &mut Codegen, operand: &str) -> bool {
    std::mem::swap(&mut cg.arc.movable, &mut cg.arc.ended);
    let taken = take_field(cg, operand);
    std::mem::swap(&mut cg.arc.movable, &mut cg.arc.ended);
    taken
}

/// Whether [`take_field`] would succeed for `operand`.
pub(super) fn can_take_field(cg: &Codegen, operand: &str) -> bool {
    takeable(cg, operand).is_some()
}

/// Give the caller the reference the holder of `operand` had for it: clear the
/// slot it was read from, so the holder no longer frees it. False, with
/// nothing emitted, when `operand` was not read from an owned temporary.
pub(super) fn take_field(cg: &mut Codegen, operand: &str) -> bool {
    let Some((key, read)) = takeable(cg, operand) else {
        return false;
    };
    let _ = cg.arc.fields.remove(&key);
    let empty = if read.ty.ends_with('*') { "null" } else { "0" };
    cg.emit(format!("store {0} {empty}, {0}* {1}", read.ty, read.slot));
    true
}
