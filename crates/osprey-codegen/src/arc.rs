//! Perceus dup/drop insertion — the ownership ledger threaded through the
//! AST walk. Implements [MEM-OWNERSHIP] and [GC-ARC-PERCEUS]
//! (docs/specs/compiler/0018-MemoryManagement.md):
//! every producer owns (+1), stores dup, owners drop at region end, returns
//! transfer (+1). The reference algorithm is Reinking, Xie, de Moura & Leijen,
//! *Perceus: Garbage Free Reference Counting with Reuse*, MSR-TR-2020-42.
//!
//! Mechanism: each owned value is spilled to a null-initialized `alloca i8*`
//! slot hoisted into the function's entry block; a region-end drop loads the
//! slot and calls `@osp_release`, then re-nulls it. Because slots dominate
//! every block and untaken paths leave them null (release(null) is a no-op),
//! drops are correct across match arms, guard diamonds and loops with no
//! dominance analysis. The default/GC backends define `osp_retain`/
//! `osp_release` as no-ops, so one IR serves every backend.

use crate::builder::Codegen;
use crate::llty::Value;

const RETAIN_DECL: &str = "declare void @osp_retain(i8*)";
const RELEASE_DECL: &str = "declare void @osp_release(i8*)";
/// The proved-unique drop: `allockind("free")`/`allocptr` pair it with
/// `osp_alloc`/`osp_alloc_tagged` (same `alloc-family`), so -O2 deletes a
/// non-escaping alloc+release outright — the [MEM-OWNERSHIP] static free.
/// Sound ONLY where codegen proves rc == 1 (see [`consume_fresh`]): the
/// attributes let LLVM treat the pointee as dead after the call.
const RELEASE_UNIQUE_DECL: &str = "declare void @osp_release_unique(i8* allocptr noundef) allockind(\"free\") mustprogress nounwind willreturn \"alloc-family\"=\"osprey\"";

/// One owned value: the SSA operand it was registered under (for move
/// detection at `bind`), the binding name once `let`-bound (for last-use
/// drops, M6), the entry-block spill slot holding it, and whether the block
/// provably holds no managed references (a scalar-payload Result with no
/// heap errmsg — eligible for [`consume_fresh`]).
struct Entry {
    operand: String,
    name: Option<String>,
    /// Unique lexical scope that introduced `name`; absent for temporaries.
    binding_scope: Option<usize>,
    slot: String,
    pure_scalar: bool,
}

/// The per-function ownership ledger: a stack of frames (function body,
/// statements, loop bodies). Swapped wholesale across `enter_nested_fn`.
pub(crate) struct ArcLedger {
    frames: Vec<Vec<Entry>>,
    /// What each frame in `frames` delimits, kept in step with it.
    kinds: Vec<FrameKind>,
    /// Open conditional regions as (frame depth, ledger length at the branch).
    /// [`consume_fresh`] retires an owner AT ITS USE and removes its ledger
    /// entry, which is only sound when that use is on every path out of the
    /// region. Inside a conditional it is not: the sibling path reaches region
    /// end with the entry already gone and nothing releases the object. Each
    /// entry recorded here is the high-water mark of owners that existed BEFORE
    /// the branch — those must survive to region end. [GC-ARC-PERCEUS]
    cond_floors: Vec<(usize, usize)>,
}

impl ArcLedger {
    pub(crate) fn new() -> ArcLedger {
        ArcLedger {
            frames: vec![Vec::new()],
            kinds: vec![FrameKind::Scope],
            cond_floors: Vec::new(),
        }
    }
}

impl Default for ArcLedger {
    fn default() -> Self {
        ArcLedger::new()
    }
}

/// Whether `v` is a candidate managed pointer: a register-carried heap
/// handle/string. Literals and `null` are rodata (registry probe-miss), and so
/// is a register the builder materialised from a `private constant` global —
/// `rodata_regs` is what lets a string literal skip the dup/drop calls
/// entirely instead of paying a locked probe to learn they are no-ops.
fn managed(cg: &Codegen, v: &Value) -> bool {
    // An erased-`any` box is an ordinary tagged allocation; excluding it here
    // was exactly #208 — the epilogue could neither move an erasing return's
    // owner out nor retain a borrowed one, so the frame freed the referent.
    v.ty.is_managed_ptr() && v.operand.starts_with('%') && !cg.is_rodata(&v.operand)
}

/// `v`'s operand as a plain `i8*`, bitcasting a typed block pointer. A flat
/// list-literal handle (`[]…` owner) travels as its raw `{ i64, i8* }*`
/// register even though its [`LType`] spells `i8*` — undo that here.
pub(crate) fn as_i8ptr(cg: &mut Codegen, v: &Value) -> String {
    let spelled = if v.osp_ty.as_deref().is_some_and(|o| o.starts_with("[]")) {
        "{ i64, i8* }*".to_string()
    } else {
        v.llvm_ty()
    };
    if spelled == "i8*" {
        v.operand.clone()
    } else {
        cg.emit_reg(format!("bitcast {spelled} {} to i8*", v.operand))
    }
}

/// Register `v` as owned (+1) by the innermost open region. Producers call
/// this: fresh aggregates, list literals, Result blocks, C-runtime string /
/// container returns, user-call results (callee epilogues transfer +1).
pub(crate) fn own(cg: &mut Codegen, v: &Value) {
    register(cg, v, false);
}

/// [`own`], but in the region that OUTLIVES the closing statement. A
/// cell-backed `mut` needs this: a handler env only DUPs the cell pointer, so
/// owning the cell in its own defining statement's region would free it at the
/// statement end and leave the env — and every arm that writes through it —
/// holding a dangling pointer.
pub(crate) fn own_beyond_stmt(cg: &mut Codegen, v: &Value) {
    register(cg, v, true);
}

fn register(cg: &mut Codegen, v: &Value, beyond_stmt: bool) {
    if !managed(cg, v) {
        return;
    }
    let ptr = as_i8ptr(cg, v);
    let slot = cg.hoist_arc_slot();
    cg.emit(format!("store i8* {ptr}, i8** {slot}"));
    let entry = Entry {
        operand: v.operand.clone(),
        name: None,
        binding_scope: None,
        slot,
        pure_scalar: false,
    };
    if beyond_stmt {
        push_parent(cg, entry);
    } else if let Some(frame) = cg.arc.frames.last_mut() {
        frame.push(entry);
    }
}

/// Mark the just-owned `v` as holding no managed references (its producer
/// proved a scalar payload and a non-heap errmsg), enabling [`consume_fresh`].
pub(crate) fn mark_pure_scalar(cg: &mut Codegen, v: &Value) {
    if let Some(e) = cg
        .arc
        .frames
        .last_mut()
        .and_then(|f| f.last_mut())
        .filter(|e| e.operand == v.operand)
    {
        e.pure_scalar = true;
    }
}

/// Consume a freshly produced, never-bound owner at its immediate unwrap:
/// pop the newest innermost-frame entry when it is `v` itself, unnamed and
/// pure-scalar, and drop it via `@osp_release_unique` — whose free-pair
/// attributes let -O2 delete the whole non-escaping alloc+release. The gates
/// make the rc == 1 proof: same SSA register ⇒ the creation dominates and
/// nothing was rebound; newest entry + unnamed ⇒ no store or `let` aliased
/// it; pure-scalar ⇒ eliding the pair drops no interior references either.
pub(crate) fn consume_fresh(cg: &mut Codegen, v: &Value) {
    let fresh = cg
        .arc
        .frames
        .last()
        .and_then(|f| f.last())
        .is_some_and(|e| e.operand == v.operand && e.name.is_none() && e.pure_scalar);
    if !fresh {
        return;
    }
    // The owner must also have been created INSIDE the innermost open
    // conditional. Retiring one from before the branch drops its ledger entry
    // on this path only, so the sibling path leaks it — `(1 % 0) * 5` leaked
    // one Result per evaluation exactly this way: the propagating-arithmetic
    // guard unwraps its operand in the success arm, and the error arm then had
    // nothing left to release. [GC-ARC-PERCEUS]
    if cg.arc.frames.last().map_or(0, Vec::len) <= conditional_floor(cg) {
        return;
    }
    if let Some(e) = cg.arc.frames.last_mut().and_then(Vec::pop) {
        let ptr = as_i8ptr(cg, v);
        release_unique(cg, &ptr);
        cg.emit(format!("store i8* null, i8** {}", e.slot));
    }
}

/// Drop the `i8*` block `ptr`, which codegen proves is the only reference:
/// default frees it, ARC releases it and tracing GC leaves it to the
/// collector [MEM-BACKENDS].
pub(crate) fn release_unique(cg: &mut Codegen, ptr: &str) {
    cg.add_extern(RELEASE_UNIQUE_DECL);
    cg.emit(format!("call void @osp_release_unique(i8* {ptr})"));
}

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
fn conditional_floor(cg: &Codegen) -> usize {
    let depth = cg.arc.frames.len();
    cg.arc
        .cond_floors
        .iter()
        .rev()
        .find(|(d, _)| *d == depth)
        .map_or(0, |(_, mark)| *mark)
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
fn release_entries(cg: &mut Codegen, entries: &[Entry]) {
    for e in entries.iter().rev() {
        cg.add_extern(RELEASE_DECL);
        let live = cg.emit_reg(format!("load i8*, i8** {}", e.slot));
        cg.emit(format!("call void @osp_release(i8* {live})"));
        cg.emit(format!("store i8* null, i8** {}", e.slot));
    }
}

/// A `let` bound `v` to `name`: transfer ownership out of the closing
/// statement region into its parent. A value the statement owns is moved; a
/// borrow (field load, unwrap payload, alias, phi of arm-owned values) is
/// retained so the binding survives its source. Non-candidates are untouched.
pub(crate) fn bind_owned(cg: &mut Codegen, name: &str, v: &Value) {
    if !managed(cg, v) {
        return;
    }
    if let Some(mut e) = take_entry(cg, &v.operand) {
        e.name = Some(name.to_string());
        e.binding_scope = cg.scope_id();
        push_parent(cg, e);
        return;
    }
    retain_val(cg, v);
    let ptr = as_i8ptr(cg, v);
    let slot = cg.hoist_arc_slot();
    cg.emit(format!("store i8* {ptr}, i8** {slot}"));
    push_parent(
        cg,
        Entry {
            operand: v.operand.clone(),
            name: Some(name.to_string()),
            binding_scope: cg.scope_id(),
            slot,
            pure_scalar: false,
        },
    );
}

/// Drop the ledger entry for `v` from whichever open region holds it, so the
/// epilogue neither dups nor drops it. Returns false when `v` is a borrow (a
/// parameter, a field load, a phi) the function never owned — the caller then
/// falls back to dup-on-return.
fn take_owner_anywhere(cg: &mut Codegen, v: &Value) -> bool {
    if !managed(cg, v) {
        return false;
    }
    for frame in &mut cg.arc.frames {
        if let Some(at) = frame.iter().rposition(|e| e.operand == v.operand) {
            let _ = frame.remove(at);
            return true;
        }
    }
    false
}

/// Remove and return the innermost-frame entry registered under `operand`.
fn take_entry(cg: &mut Codegen, operand: &str) -> Option<Entry> {
    let frame = cg.arc.frames.last_mut()?;
    let at = frame.iter().rposition(|e| e.operand == operand)?;
    Some(frame.remove(at))
}

/// Push `entry` into the innermost scope frame: the region that outlives the
/// closing statement. A binding made in a loop body belongs to the iteration,
/// not the whole function: with frames `[function, loop]` and no statement
/// frame between them, choosing "the frame beneath the innermost" put a record
/// captured by the loop body's closure in the FUNCTION frame, where the loop
/// overwrote its slot each iteration and leaked every value but the last.
fn push_parent(cg: &mut Codegen, entry: Entry) {
    let at = cg
        .arc
        .kinds
        .iter()
        .rposition(|kind| *kind == FrameKind::Scope)
        .unwrap_or(0);
    if let Some(frame) = cg.arc.frames.get_mut(at) {
        frame.push(entry);
    }
}

/// Emit `osp_retain` on a raw `i8*` operand — the mirror of
/// [`release_operand`]. Unlike [`dup_store`] this does not inspect the operand
/// form, so a caller that pairs it with a release cannot go out of balance for
/// a value spelled as a constant. Both hooks are null-safe and ignore pointers
/// the heap never registered (rodata), so an unconditional pair is correct.
pub(crate) fn retain_operand(cg: &mut Codegen, operand: &str) {
    cg.add_extern(RETAIN_DECL);
    cg.emit(format!("call void @osp_retain(i8* {operand})"));
}

/// Emit `osp_release` on a raw `i8*` operand (a loaded old cell value, a
/// runtime-held env whose structural end has been reached).
pub(crate) fn release_operand(cg: &mut Codegen, operand: &str) {
    cg.add_extern(RELEASE_DECL);
    cg.emit(format!("call void @osp_release(i8* {operand})"));
}

/// Emit `osp_retain` on a candidate value (dup).
fn retain_val(cg: &mut Codegen, v: &Value) {
    if !managed(cg, v) {
        return;
    }
    let ptr = as_i8ptr(cg, v);
    cg.add_extern(RETAIN_DECL);
    cg.emit(format!("call void @osp_retain(i8* {ptr})"));
}

/// Dup-on-store: retain `operand` when it is a register-carried pointer being
/// stored into a heap slot spelled `slot_ty` (the owning block's drop mask
/// releases it, so the store is a new reference). [GC-ARC-PERCEUS]
pub(crate) fn dup_store(cg: &mut Codegen, slot_ty: &str, operand: &str) {
    if slot_ty.ends_with('*') && operand.starts_with('%') {
        cg.add_extern(RETAIN_DECL);
        let ptr = if slot_ty == "i8*" {
            operand.to_string()
        } else {
            cg.emit_reg(format!("bitcast {slot_ty} {operand} to i8*"))
        };
        cg.emit(format!("call void @osp_retain(i8* {ptr})"));
    }
}

/// Function epilogue at the single `ret`: retain the escaping return first
/// (transfer, +1 to the caller), then drop every owner in every frame. The
/// retain-before-release order keeps the returned value's count positive
/// throughout. [GC-ARC-PERCEUS]
pub(crate) fn epilogue(cg: &mut Codegen, ret: Option<&Value>) {
    // Perceus return transfer (TR Fig. 6): when the returned value is itself
    // one of this function's owners, MOVE it out of the ledger rather than
    // dup-it-then-drop-it — the +1 the caller receives is the one the function
    // already holds. Saves a retain/release pair on every value-returning call.
    if let Some(v) = ret.filter(|v| !take_owner_anywhere(cg, v)) {
        retain_val(cg, v);
    }
    let frames = std::mem::take(&mut cg.arc.frames);
    for frame in frames.iter().rev() {
        release_entries(cg, frame);
    }
    cg.arc.frames = vec![Vec::new()];
    cg.arc.kinds = vec![FrameKind::Scope];
}

/// A candidate value is about to escape boxed as an `i64` across a fiber /
/// effect / channel boundary where the pointer-ness is erased: dup it so the
/// receiving side owns +1 (the boundary's structural release lands in M5).
pub(crate) fn escape_retain(cg: &mut Codegen, v: &Value) {
    retain_val(cg, v);
}

mod constructors;
mod last_use;
mod unwind;
pub(crate) use constructors::consume_into_store;
pub(crate) use last_use::release_dead_after;
pub(crate) use unwind::instrument as instrument_unwind;
