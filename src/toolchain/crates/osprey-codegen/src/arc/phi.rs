use super::{managed, mark_pure_scalar, own, Codegen, Entry, Value};

/// Promote mixed joins only when every incoming handle can carry ownership and
/// at least one arm already produced its reference inside this conditional.
pub(crate) fn borrowed_phi_arms(cg: &Codegen, incoming: &[Value], mark: usize) -> Vec<bool> {
    let promote = incoming.iter().all(|value| managed(cg, value))
        && incoming.iter().any(|value| arm_owned(cg, value, mark));
    incoming
        .iter()
        .map(|value| promote && !arm_owned(cg, value, mark))
        .collect()
}

/// Each deferred exit acquires exactly one reference: fresh arm owners transfer
/// theirs, while a borrowed arm takes its own before joining. Outer owners stay
/// live on every path and are never consumed by this promotion.
pub(crate) fn promote_phi_arm(cg: &mut Codegen, value: &Value, borrowed: bool) {
    if borrowed {
        let pointer = super::as_i8ptr(cg, value);
        super::retain_operand(cg, &pointer);
        own(cg, value);
    }
}

fn arm_owned(cg: &Codegen, value: &Value, mark: usize) -> bool {
    incoming_owners_match(cg, std::slice::from_ref(&value.operand), mark, |_| true)
}

/// Perceus phi ownership merge (TR §2.3, join points): when EVERY arm of a
/// `match` produced a fresh owner — an unnamed innermost-frame entry registered
/// AFTER `mark` (i.e. inside its own arm) — the merged phi result *is* that
/// single owner. Exactly one arm runs on any path, untaken arms never allocate
/// their value, so exactly one of those +1s is live and the phi selects it.
/// Drop the per-arm entries and re-own the phi: the value moves through the
/// join with no dup and no per-arm release, so `fn make(d) = match d { …
/// Node{…} }` returns by transfer instead of retain-then-release.
///
/// The `mark` gate is what makes this sound: an entry below it existed before
/// the arms — the scrutinee, an outer temporary — and lives on EVERY path, so
/// consuming it through the phi would leak it on the arms that yield something
/// else. Borrows (parameters, field loads) were never registered and fail the
/// gate, keeping the caller's dup/drop. Clear transferred slots so abandonment
/// cannot release both an incoming owner and its phi. [GC-ARC-PERCEUS]
pub(crate) fn move_phi_owners(cg: &mut Codegen, incoming: &[String], out: &Value, mark: usize) {
    if !managed(cg, out) || !incoming_owners_match(cg, incoming, mark, |_| true) {
        return;
    }
    let all_pure = incoming_owners_match(cg, incoming, mark, |e| e.pure_scalar);
    for slot in take_phi_slots(cg, incoming, mark) {
        cg.emit(format!("store i8* null, i8** {slot}"));
    }
    own(cg, out);
    if all_pure {
        mark_pure_scalar(cg, out);
    }
}

fn take_phi_slots(cg: &mut Codegen, incoming: &[String], mark: usize) -> Vec<String> {
    let Some(frame) = cg.arc.frames.last_mut() else {
        return Vec::new();
    };
    let (moved, kept): (Vec<_>, Vec<_>) = frame
        .split_off(mark.min(frame.len()))
        .into_iter()
        .partition(|e| e.name.is_none() && incoming.contains(&e.operand));
    frame.extend(kept);
    moved.into_iter().map(|e| e.slot).collect()
}

fn incoming_owners_match(
    cg: &Codegen,
    incoming: &[String],
    mark: usize,
    accepts: impl Fn(&Entry) -> bool,
) -> bool {
    cg.arc.frames.last().is_some_and(|f| {
        incoming.iter().all(|op| {
            f.iter()
                .skip(mark)
                .any(|e| e.operand == *op && e.name.is_none() && accepts(e))
        })
    })
}
