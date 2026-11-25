use super::{managed, mark_pure_scalar, own, Codegen, Entry, Value};

/// Promote mixed joins only when every incoming handle can carry ownership and
/// at least one arm already produced its reference inside this conditional.
pub(crate) fn borrowed_phi_arms(cg: &Codegen, incoming: &[Value], mark: usize) -> Vec<bool> {
    // When every arm that is not a new value can take its field out of an
    // owned temporary, the join owns what it yields [MEM-OWNERSHIP-STATIC-PATHS].
    // An arm that yields a literal yields no reference and needs no owner
    // (`literal_needs_no_owner`, [MEM-OWNERSHIP-STATIC-MODEL]).
    let counted = |value: &&Value| value.operand.starts_with('%') && !cg.is_rodata(&value.operand);
    let gives = |value: &Value| {
        arm_owned(cg, value, mark)
            || super::takes::can_take_field(cg, &value.operand)
            || super::moves::can_give_up(cg, &value.operand)
    };
    let every_arm_gives = cg.ownership.on
        && incoming.iter().any(|value| counted(&value))
        && incoming
            .iter()
            .filter(counted)
            .all(|value| managed(cg, value) && gives(value));
    // Under ownership an arm that can't give its value leaves the join
    // borrowing: the arms that made a value keep it for the statement
    // ([`note_mixed`]). Counting backends give that arm a reference instead.
    let counts = !cg.ownership.on
        && incoming.iter().all(|value| managed(cg, value))
        && incoming.iter().any(|value| arm_owned(cg, value, mark));
    let promote = every_arm_gives || counts;
    incoming
        .iter()
        .map(|value| promote && counted(&value) && !arm_owned(cg, value, mark))
        .collect()
}

/// Arms that meet must agree on the owner (`join_needs_one_owner`,
/// [MEM-OWNERSHIP-STATIC-MODEL]), so every arm hands the join one reference.
/// Each deferred exit acquires exactly one reference: fresh arm owners transfer
/// theirs, while a borrowed arm takes its own before joining. Outer owners stay
/// live on every path and are never consumed by this promotion.
pub(crate) fn promote_phi_arm(cg: &mut Codegen, value: &Value, borrowed: bool) {
    if !borrowed {
        return;
    }
    // An arm that yields a binding at its last use gives that reference to the
    // join; the other arms still release the binding [GC-ARC-PERCEUS].
    if let Some(slot) = super::moves::give_up(cg, &value.operand) {
        cg.emit(format!("store i8* null, i8** {slot}"));
        own(cg, value);
        return;
    }
    if super::takes::take_field(cg, &value.operand) {
        own(cg, value);
        return;
    }
    {
        let pointer = super::as_i8ptr(cg, value);
        super::second_owner_of(cg, value, None, super::Need::Joined);
        super::count_call(cg, &pointer);
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
    // An arm that yields a literal or an immortal singleton yields no reference:
    // only the other arms need an owner to hand to the join, and releasing the
    // join on that arm's path releases nothing.
    let counted: Vec<String> = incoming
        .iter()
        .filter(|operand| operand.starts_with('%') && !cg.is_rodata(operand))
        .cloned()
        .collect();
    if counted.is_empty() {
        // A join of literals is as static as each of them.
        cg.mark_rodata(&out.operand);
        return;
    }
    let every_arm_counted = counted.len() == incoming.len();
    let incoming = counted.as_slice();
    let owned = !incoming.is_empty() && incoming_owners_match(cg, incoming, mark, |_| true);
    if !managed(cg, out) || !owned {
        note_mixed(cg, incoming, out, mark);
        return;
    }
    // The unique release is for a block the join alone owns, never a literal.
    let all_pure =
        every_arm_counted && incoming_owners_match(cg, incoming, mark, |e| e.pure_scalar);
    for slot in take_phi_slots(cg, incoming, mark) {
        cg.emit(format!("store i8* null, i8** {slot}"));
    }
    own(cg, out);
    if all_pure {
        mark_pure_scalar(cg, out);
    }
}

/// Some arms of the join `out` made their value and others only borrow one
/// [MEM-OWNERSHIP-STATIC-PATHS]: under ownership the join borrows, and the
/// values the arms made stay with the statement. Remember them, so a `let`
/// that names the join can keep them for as long as the name is read
/// ([`take_mixed`], [`super::borrows`]). In the ledger model this is `take_on_one_path`: each
/// path frees the value it made once, and the borrowed path frees nothing.
fn note_mixed(cg: &mut Codegen, incoming: &[String], out: &Value, mark: usize) {
    if !cg.ownership.on {
        return;
    }
    // An arm that is itself such a join makes this one a join of the kind.
    let made = |operand: &String| {
        incoming_owners_match(cg, std::slice::from_ref(operand), mark, |_| true)
            || cg.arc.mixed.contains(operand)
    };
    if incoming.iter().any(made) {
        let _ = cg.arc.mixed.insert(out.operand.clone());
    }
}

/// Whether `operand` is a join that borrows on some paths, asked once.
pub(super) fn take_mixed(cg: &mut Codegen, operand: &str) -> bool {
    let key = super::aliases::root(cg, operand).to_owned();
    cg.arc.mixed.remove(&key)
}

fn take_phi_slots(cg: &mut Codegen, incoming: &[String], mark: usize) -> Vec<String> {
    let Some(frame) = cg.arc.frames.last_mut() else {
        return Vec::new();
    };
    let (moved, kept): (Vec<_>, Vec<_>) = frame
        .split_off(mark.min(frame.len()))
        .into_iter()
        .partition(|e| e.name.is_none() && !e.lent && incoming.contains(&e.operand));
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
                .any(|e| e.operand == *op && e.name.is_none() && !e.lent && accepts(e))
        })
    })
}
