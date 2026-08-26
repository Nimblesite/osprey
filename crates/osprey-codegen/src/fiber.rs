//! Fibers, channels, and `yield`, lowered to the same C fiber runtime
//! every compiled Osprey program links (`fiber_runtime.c` in
//! `libfiber_runtime.a`). `spawn e` lowers `e` as a zero-parameter closure
//! (`crate::closure`): the thunk takes the closure cell as its env, reloads the
//! captures `e` closes over, and is handed to `fiber_spawn_env_owned` together
//! with its per-spawn heap cell — so two in-flight spawns from one site never share
//! capture state (the runtime restores the spawner's effect-handler snapshot
//! inside the fiber, so `perform` works there). `await`/`fiberDone` map to
//! `fiber_await`/`fiber_done`, the non-blocking probe a foreground loop can
//! animate against while the fiber works. Channels are
//! `channel_create`/`channel_send`/`channel_recv`, with `channel_cleanup`
//! releasing whatever a program never received; channel ids and fiber ids
//! draw from the runtime's one shared counter. `yield e` performs the runtime's
//! cooperative hand-off (`fiber_yield`) and evaluates to its operand.
//! Implements [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-CHANNEL], and
//! [CONCURRENCY-YIELD].

use crate::builder::Codegen;
use crate::conv::{as_i64, box_to_i64};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, MatchArm};

/// The thunk's value-ABI signature: no parameters, returns the boxed `i64`
/// fiber result.
const THUNK_SIG: (LType, Option<LType>) = (LType::I64, None);

/// `spawn e` — lower `e` as a zero-parameter closure and start it on a real
/// fiber via `fiber_spawn_env_owned(thunk, cell, result_managed)`.
pub(crate) fn gen_spawn(cg: &mut Codegen, e: &Expr) -> Result<Value> {
    cg.lowered.fibers = true;
    let id = cg.next_lambda_id();
    let thunk = format!("__fiber_thunk_{id}");
    let caps = crate::closure::capture_list(cg, &[], e);
    let cell_ty = crate::closure::cell_struct_ty(&caps);
    let saved = cg.enter_nested_fn();
    crate::closure::reload_captures(cg, &cell_ty, &caps);
    let elem = thunk_body(cg, e);
    cg.exit_nested_fn(saved, "i64", &thunk, &[(LType::Ptr, String::from("__env"))]);
    let elem = elem?;
    let sig = (Vec::new(), THUNK_SIG.0, THUNK_SIG.1, None);
    let cell = crate::closure::cell_value(cg, id, &thunk, &cell_ty, &caps, &sig);
    let result_managed = i64::from(elem.ty.is_managed_ptr()).to_string();
    let r = cg.call(
        "i64",
        "fiber_spawn_env_owned",
        "i64 (i8*)*, i8*, i64",
        &[&format!("@{thunk}"), &cell.operand, &result_managed],
    );
    // Tag the handle with the fiber's element type so `await` recovers it.
    Ok(Value::new(r, LType::I64).with_fiber_elem(&elem))
}

/// Lower the spawned expression into the thunk and box its result to the
/// uniform `i64` fiber-result ABI; returns the element type so the spawn site
/// can tag the handle for `await` to unbox.
fn thunk_body(cg: &mut Codegen, e: &Expr) -> Result<Value> {
    let v = gen_expr(cg, e)?;
    let elem = v.clone();
    // The result escapes boxed across the fiber boundary: dup it before the
    // thunk's owners drop, so the runtime's completed-result slot holds +1
    // until main's fiber cleanup [GC-ARC-PERCEUS].
    crate::arc::escape_retain(cg, &v);
    let b = box_to_i64(cg, v);
    crate::arc::epilogue(cg, None);
    cg.emit(format!("ret i64 {}", b.operand));
    Ok(elem)
}

/// `await(fiber)` — block on the C runtime until the fiber completes, then
/// unbox its `i64` result back to the fiber's element type (a string/handle
/// result is a pointer, recovered via `inttoptr`).
pub(crate) fn gen_await(cg: &mut Codegen, e: &Expr) -> Result<Value> {
    let f = gen_expr(cg, e)?;
    // Same rule as `recv`, and for the same reason: see `gen_recv` below. A
    // fiber whose result type did not reach here cannot be unboxed, and
    // guessing the wire word answers a wrong value in silence.
    let Some(elem) = f.fiber_elem else {
        return Err(CodegenError::invalid(
            "this fiber's result type did not reach `await`: it was read out of \
             a field or structure whose declared type is a type variable, which \
             carries no result type. Give the field the fiber's concrete type",
        ));
    };
    let owner = f.fiber_elem_owner.clone();
    let result_inner = f.fiber_elem_result_inner;
    let payload_owner = f.fiber_elem_payload_owner.clone();
    let id = as_i64(cg, f)?;
    let r = cg.call("i64", "fiber_await", "i64", &[&id.operand]);
    let mut out = crate::effects::unbox_coro_value(cg, &r, elem, result_inner).with_owner(owner);
    out.payload_owner = payload_owner;
    // The runtime retains a fresh caller reference on every await. Register
    // that +1 in this ARC region just like an ordinary function return; the
    // runtime keeps and later drops its separate completed-result root.
    // [GC-ARC-PERCEUS] [MEM-FIBER-ISOLATION]
    crate::arc::own(cg, &out);
    Ok(out)
}

/// `yield e` / `yield` — drive the runtime's cooperative hand-off, then evaluate
/// to the operand. `fiber_yield` donates the CPU to the scheduler in concurrent
/// mode (a no-op under deterministic execution) and forwards its argument
/// unchanged, so the Osprey value is preserved with its original type.
pub(crate) fn gen_yield(cg: &mut Codegen, e: Option<&Expr>) -> Result<Value> {
    let value = match e {
        Some(inner) => gen_expr(cg, inner)?,
        None => Value::unit(),
    };
    let boxed = box_to_i64(cg, value.clone());
    let _ = cg.call("i64", "fiber_yield", "i64", &[&boxed.operand]);
    Ok(value)
}

/// `send(channel, value)` — `channel_send` on the C runtime (blocks when full).
pub(crate) fn gen_send(cg: &mut Codegen, channel: &Expr, value: &Expr) -> Result<Value> {
    cg.lowered.channels = true;
    let ch = gen_expr(cg, channel)?;
    let id = as_i64(cg, ch)?;
    let v = gen_expr(cg, value)?;
    if v.result_inner.is_some() {
        return Err(crate::error::CodegenError::unsupported(
            "Result-valued channels are not yet represented losslessly; handle the Result before sending",
        ));
    }
    // The channel wire word is a uniform `i64`. Record what the element really
    // is on the channel BINDING so `recv` can unbox it back: without this a
    // received list, map or string arrives as an integer and every later read
    // of it reads a pointer's bits as a value — a `Channel<List<List<int>>>`
    // came back as `List<int>` and every nested access answered its fallback
    // ([CONCURRENCY-CHANNEL]).
    // A channel send is an ESCAPE like a function return or a record field: the
    // receiver sees only `Channel<List<T>>` and reads the value as an
    // `OspreyList`, so a flat `{ length, data }` literal must be materialized
    // here or its header is misread ([`crate::listlit::escaping`]). Without
    // this `send(ch, [1, 2, 3])` put a literal on the wire and every read of
    // the received value ran off the end of a 16-byte header.
    let v = crate::listlit::escaping(cg, v);
    // Tag AFTER materializing: the binding must record what actually went on
    // the wire, not the literal shape it started as.
    if let Expr::Identifier(name) = channel {
        let elem = v.clone();
        cg.retag(name, |held| held.with_fiber_elem(&elem));
    }
    // The sent value escapes boxed into the channel buffer: the receiver's
    // side owns +1 [GC-ARC-PERCEUS] [MEM-FIBER-ISOLATION].
    // The channel owns that reference until a `recv` adopts it, so it has to
    // know the value is managed: a send the runtime rejects releases it again,
    // and `channel_cleanup` releases whatever was never received.
    let managed = i64::from(v.ty.is_managed_ptr()).to_string();
    crate::arc::escape_retain(cg, &v);
    let v = box_to_i64(cg, v);
    let r = cg.call(
        "i64",
        "channel_send",
        "i64, i64, i64",
        &[&id.operand, &v.operand, &managed],
    );
    Ok(Value::new(r, LType::I64))
}

/// `recv(channel)` — `channel_recv` on the C runtime (blocks when empty).
pub(crate) fn gen_recv(cg: &mut Codegen, channel: &Expr) -> Result<Value> {
    cg.lowered.channels = true;
    let ch = gen_expr(cg, channel)?;
    // A MISSING element type is a hard error, never a default. It used to fall
    // back to the raw `i64` wire word, and that answered a plausible WRONG
    // VALUE: a `Channel<List<List<int>>>` read out of a generic field came back
    // readable only as its wire word, so `listLength(listGet(m, 0))` printed
    // `0` where the answer was `3` — exit 0, no diagnostic, nothing to notice.
    // The tag does not travel through a field whose declared type is a type
    // VARIABLE (`type Box<t> = Box { slot: t }`), because the declaration is
    // all the field read has to go on. Rejecting is the honest answer until it
    // does travel [CONCURRENCY-CHANNEL].
    let Some(elem) = ch.fiber_elem else {
        return Err(CodegenError::invalid(
            "this channel's element type did not reach `recv`: it was read out \
             of a field or structure whose declared type is a type variable, \
             which carries no element type. Give the field the channel's \
             concrete type",
        ));
    };
    let owner = ch.fiber_elem_owner.clone();
    let result_inner = ch.fiber_elem_result_inner;
    let payload_owner = ch.fiber_elem_payload_owner.clone();
    let id = as_i64(cg, ch)?;
    let r = cg.call("i64", "channel_recv", "i64", &[&id.operand]);
    let mut out = crate::effects::unbox_coro_value(cg, &r, elem, result_inner).with_owner(owner);
    out.payload_owner = payload_owner;
    // `send` retained a reference for the receiver; register it here so this
    // region drops it [GC-ARC-PERCEUS] [MEM-FIBER-ISOLATION].
    crate::arc::own(cg, &out);
    Ok(out)
}

/// `select` reaching code generation is a hard error, not a silent choice.
///
/// The typed compiler rejects every such node first ([CONCURRENCY-SELECT-REJECT]),
/// so this arm is only reachable by compiling an AST directly. It used to lower
/// the FIRST arm's body — a plausible-looking wrong answer for any program that
/// bypassed the checker, and one that would have masked the real multiplexing
/// primitive's absence once channel operations landed in the arms. Failing here
/// is [plan 0007](../../../docs/plans/0007-fiber-select.md)'s "direct AST
/// compilation cannot silently choose the first arm" acceptance bar; the arms are
/// unused because no arm shape survives to lowering yet.
pub(crate) fn gen_select(_cg: &mut Codegen, _arms: &[MatchArm]) -> Result<Value> {
    Err(CodegenError::unsupported(
        "`select` is not supported; use explicit `send`, `recv`, and `await`",
    ))
}

/// Fiber/channel builtins reached as ordinary calls. Returns `None` when `name`
/// is not one of them.
pub(crate) fn gen_builtin(cg: &mut Codegen, name: &str, args: &[Expr]) -> Result<Option<Value>> {
    let v = match name {
        // `Channel(capacity)` — a real C-runtime channel; its id comes from the
        // same counter as fiber ids.
        "Channel" => {
            let cap = match args.first() {
                Some(a) => {
                    let v = gen_expr(cg, a)?;
                    as_i64(cg, v)?.operand
                }
                None => String::from("0"),
            };
            // `_checked`, not the bare constructor: `Channel` has no failure
            // channel in the language, so a runtime that cannot honour the
            // capacity must stop the program rather than answer a negative code
            // the rest of the lowering would treat as a handle
            // [CONCURRENCY-CHANNEL].
            let r = cg.call("i64", "channel_create_checked", "i64", &[&cap]);
            Value::new(r, LType::I64)
        }
        // `fiber_yield(v)` called as an ordinary function shares `yield`'s
        // lowering — the same runtime hand-off, forwarding `v`.
        "fiber_yield" => gen_yield(cg, args.first())?,
        // `fiberDone(f)` — the C runtime's non-blocking completion probe
        // [CONCURRENCY-YIELD].
        "fiberDone" => {
            let Some(a) = args.first() else {
                return Err(crate::error::CodegenError::invalid(
                    "fiberDone needs a fiber argument",
                ));
            };
            let v = gen_expr(cg, a)?;
            let id = as_i64(cg, v)?;
            let r = cg.call("i64", "fiber_done", "i64", &[&id.operand]);
            Value::new(r, LType::I64)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}
