//! Perceus dup/drop insertion — the ownership ledger threaded through the
//! AST walk. Implements [GC-ARC-PERCEUS] (docs/plans/0011 phase 2, M3):
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
//! `osp_release` as no-ops (plan 0011 M0), so one IR serves every backend.

use crate::builder::Codegen;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Stmt};

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
    slot: String,
    pure_scalar: bool,
}

/// The per-function ownership ledger: a stack of frames (function body,
/// statements, loop bodies). Swapped wholesale across `enter_nested_fn`.
pub(crate) struct ArcLedger {
    frames: Vec<Vec<Entry>>,
}

impl ArcLedger {
    pub(crate) fn new() -> ArcLedger {
        ArcLedger {
            frames: vec![Vec::new()],
        }
    }
}

impl Default for ArcLedger {
    fn default() -> Self {
        ArcLedger::new()
    }
}

/// Whether `v` is a candidate managed pointer: a register-carried heap
/// handle/string. Literals and `null` are rodata (registry probe-miss).
fn managed(v: &Value) -> bool {
    matches!(v.ty, LType::Str | LType::Ptr) && v.operand.starts_with('%')
}

/// `v`'s operand as a plain `i8*`, bitcasting a typed block pointer. A flat
/// list-literal handle (`[]…` owner) travels as its raw `{ i64, i8* }*`
/// register even though its [`LType`] spells `i8*` — undo that here.
fn as_i8ptr(cg: &mut Codegen, v: &Value) -> String {
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
    if !managed(v) {
        return;
    }
    let ptr = as_i8ptr(cg, v);
    let slot = cg.hoist_arc_slot();
    cg.emit(format!("store i8* {ptr}, i8** {slot}"));
    if let Some(frame) = cg.arc.frames.last_mut() {
        frame.push(Entry {
            operand: v.operand.clone(),
            name: None,
            slot,
            pure_scalar: false,
        });
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
    if let Some(e) = cg.arc.frames.last_mut().and_then(Vec::pop) {
        cg.add_extern(RELEASE_UNIQUE_DECL);
        let ptr = as_i8ptr(cg, v);
        cg.emit(format!("call void @osp_release_unique(i8* {ptr})"));
        cg.emit(format!("store i8* null, i8** {}", e.slot));
    }
}

/// Open a region: statement, loop body.
pub(crate) fn push_frame(cg: &mut Codegen) {
    cg.arc.frames.push(Vec::new());
}

/// Close the innermost region: drop every owner it still holds.
pub(crate) fn pop_frame(cg: &mut Codegen) {
    if let Some(frame) = cg.arc.frames.pop() {
        release_entries(cg, &frame);
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
    if !managed(v) {
        return;
    }
    if let Some(mut e) = take_entry(cg, &v.operand) {
        e.name = Some(name.to_string());
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
            slot,
            pure_scalar: false,
        },
    );
}

/// Remove and return the innermost-frame entry registered under `operand`.
fn take_entry(cg: &mut Codegen, operand: &str) -> Option<Entry> {
    let frame = cg.arc.frames.last_mut()?;
    let at = frame.iter().rposition(|e| e.operand == operand)?;
    Some(frame.remove(at))
}

/// Push `entry` into the frame beneath the innermost (the region that
/// outlives the closing statement); the innermost when it is the only one.
fn push_parent(cg: &mut Codegen, entry: Entry) {
    let n = cg.arc.frames.len();
    let at = if n >= 2 { n - 2 } else { n - 1 };
    if let Some(frame) = cg.arc.frames.get_mut(at) {
        frame.push(entry);
    }
}

/// Emit `osp_release` on a raw `i8*` operand (a loaded old cell value, a
/// runtime-held env whose structural end has been reached).
pub(crate) fn release_operand(cg: &mut Codegen, operand: &str) {
    cg.add_extern(RELEASE_DECL);
    cg.emit(format!("call void @osp_release(i8* {operand})"));
}

/// Emit `osp_retain` on a candidate value (dup).
pub(crate) fn retain_val(cg: &mut Codegen, v: &Value) {
    if !managed(v) {
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
    if let Some(v) = ret {
        retain_val(cg, v);
    }
    let frames = std::mem::take(&mut cg.arc.frames);
    for frame in frames.iter().rev() {
        release_entries(cg, frame);
    }
    cg.arc.frames = vec![Vec::new()];
}

/// A candidate value is about to escape boxed as an `i64` across a fiber /
/// effect / channel boundary where the pointer-ness is erased: dup it so the
/// receiving side owns +1 (the boundary's structural release lands in M5).
pub(crate) fn escape_retain(cg: &mut Codegen, v: &Value) {
    retain_val(cg, v);
}

/// Drop the owners of `let` names the continuation of the current block no
/// longer references (M6 last-use precision, TR Fig. 6). Runs ONLY at
/// function level (ledger depth 1): a nested block / loop / statement region
/// computes liveness for its own continuation, which says nothing about
/// later uses in the enclosing scope — releasing function-frame names from
/// there would be a use-after-free. A tail-position block IS the enclosing
/// continuation, so the depth gate composes through nested tail blocks.
pub(crate) fn release_dead_after<S: std::borrow::Borrow<Stmt>>(
    cg: &mut Codegen,
    rest: &[S],
    value: Option<&Expr>,
) {
    if cg.arc.frames.len() != 1 {
        return;
    }
    let mut live = std::collections::BTreeSet::new();
    crate::freevars::free_idents_of_stmts(rest, value, &mut live);
    release_dead(cg, &live);
}

fn release_dead(cg: &mut Codegen, live: &std::collections::BTreeSet<String>) {
    let mut dead = Vec::new();
    if let Some(frame) = cg.arc.frames.first_mut() {
        let mut kept = Vec::new();
        for e in frame.drain(..) {
            match &e.name {
                Some(n) if !live.contains(n) => dead.push(e),
                _ => kept.push(e),
            }
        }
        *frame = kept;
    }
    release_entries(cg, &dead);
}
