//! Fibers, channels, and `yield`, lowered to the same C fiber runtime
//! every compiled Osprey program links (`fiber_runtime.c` in
//! `libfiber_runtime.a`). `spawn e` lowers `e` as a zero-parameter closure
//! (`crate::closure`): the thunk takes the closure cell as its env, reloads the
//! captures `e` closes over, and is handed to `fiber_spawn_env_owned` together
//! with its per-spawn heap cell — so two in-flight spawns from one site never share
//! capture state (the runtime restores the spawner's effect-handler snapshot
//! inside the fiber, so `perform` works there). `await`/`fiberDone` map to
//! `fiber_await`/`fiber_done`, the non-blocking probe a foreground loop can
//! animate against while the fiber works. A `Fiber<T>` is a managed handle
//! the memory backend reclaims like any value, and the fiber dies with the
//! last of its handle, its runner and its awaits
//! ([CONCURRENCY-FIBER-LIFETIME]). Channels are
//! `channel_create`/`channel_send`/`channel_recv` on a managed handle
//! of the same kind, whose channel dies with it and releases whatever a
//! program never received ([CONCURRENCY-CHANNEL-LIFETIME]).
//! `yield e` performs the runtime's
//! cooperative hand-off (`fiber_yield`) and evaluates to its operand.
//! Implements [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-CHANNEL], and
//! [CONCURRENCY-YIELD].

use crate::builder::Codegen;
use crate::conv::{as_i64, box_to_i64};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, MatchArm, Pattern, SelectOperation};

/// The thunk's value-ABI signature: no parameters, returns the boxed `i64`
/// fiber result.
const THUNK_SIG: (LType, Option<LType>) = (LType::I64, None);
/// The `Error` message of a `Channel(capacity)` the runtime refused.
const CHANNEL_REFUSED: &str = "Channel capacity must be positive and fit in memory";

/// `spawn e` — lower `e` as a zero-parameter closure and start it on a real
/// fiber via `fiber_spawn_env_owned(thunk, cell, result_managed)`.
pub(crate) fn gen_spawn(cg: &mut Codegen, e: &Expr) -> Result<Value> {
    cg.lowered.fibers = true;
    let id = cg.next_lambda_id();
    let thunk = format!("__fiber_thunk_{id}");
    let caps = crate::closure::capture_list(cg, &[], e);
    let cell_ty = crate::closure::cell_struct_ty(&caps);
    let saved = cg.enter_nested_body(e);
    crate::closure::reload_captures(cg, &cell_ty, &caps);
    let elem = thunk_body(cg, e);
    cg.exit_nested_fn(saved, "i64", &thunk, &[(LType::Ptr, String::from("__env"))]);
    let elem = elem?;
    let sig = (Vec::new(), THUNK_SIG.0, THUNK_SIG.1, None, None, None);
    let cell = crate::closure::cell_value(cg, id, &thunk, &cell_ty, &caps, &sig);
    let result_managed = i64::from(elem.ty.is_managed_ptr()).to_string();
    let r = cg.call(
        "i8*",
        "fiber_spawn_env_owned",
        "i64 (i8*)*, i8*, i64",
        &[&format!("@{thunk}"), &cell.operand, &result_managed],
    );
    // Tag the handle with the fiber's element type so `await` recovers it. The
    // runtime hands the handle over at +1, like a constructor.
    let handle = Value::new(r, LType::Ptr).with_fiber_elem(&elem);
    crate::arc::own(cg, &handle);
    Ok(handle)
}

/// Lower the spawned expression into the thunk and box its result to the
/// uniform `i64` fiber-result ABI; returns the element type so the spawn site
/// can tag the handle for `await` to unbox.
fn thunk_body(cg: &mut Codegen, e: &Expr) -> Result<Value> {
    let v = crate::expr::gen_body(cg, e)?;
    // Await restores the runtime list ABI, so materialize an inlined generic
    // call's literal result before its pointer crosses the fiber boundary.
    let v = crate::listlit::escaping(cg, v);
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
    // guessing the wire word answers a wrong value in silence. The runtime
    // retains a fresh caller reference on every await and keeps, then later
    // drops, its separate completed-result root.
    let wire = handle_word(cg, &f);
    adopt_from_runtime(
        cg,
        f,
        ("fiber_await", "i8*", &wire),
        "this fiber's result type did not reach `await`: it was read out of \
         a field or structure whose declared type is a type variable, which \
         carries no result type. Give the field the fiber's concrete type",
    )
}

/// A fiber's spawn number, what interpolating it shows.
pub(crate) fn fiber_id(cg: &mut Codegen, handle: &Value) -> Value {
    let wire = handle_word(cg, handle);
    let r = cg.call("i64", "fiber_id", "i8*", &[&wire]);
    Value::new(r, LType::I64)
}

/// A fiber or channel handle as the `i8*` the runtime's entry points take.
fn handle_word(cg: &mut Codegen, handle: &Value) -> String {
    crate::arc::as_i8ptr(cg, handle)
}

/// The `i64` word the runtime call `(runtime, parameter type, wire operand)`
/// answers for the handle `handle`, unboxed back to the element type the
/// handle carries, with the +1 reference the runtime hands over registered in
/// this ARC region like an ordinary function return [GC-ARC-PERCEUS]
/// [MEM-FIBER-ISOLATION]. A handle whose element type did not travel here is
/// the hard error `missing`.
fn adopt_from_runtime(
    cg: &mut Codegen,
    handle: Value,
    (runtime, parameter, wire): (&str, &str, &str),
    missing: &str,
) -> Result<Value> {
    if handle.fiber_elem.is_none() {
        return Err(CodegenError::invalid(missing));
    }
    let r = cg.call("i64", runtime, parameter, &[wire]);
    adopt_word(cg, handle, &r, missing)
}

/// Unbox a wire word received for `handle` and register its +1 reference.
fn adopt_word(cg: &mut Codegen, handle: Value, word: &str, missing: &str) -> Result<Value> {
    let Some(elem) = handle.fiber_elem else {
        return Err(CodegenError::invalid(missing));
    };
    let mut out = crate::effects::unbox_coro_value(cg, word, elem, handle.fiber_elem_result_inner)
        .with_owner(handle.fiber_elem_owner);
    out.payload_owner = handle.fiber_elem_payload_owner;
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
    let wire = send_wire(cg, channel, value)?;
    let r = cg.call(
        "i64",
        "channel_send",
        "i8*, i64, i64",
        &[&wire.id, &wire.word, &wire.managed],
    );
    Ok(Value::new(r, LType::I64))
}

/// A sent value on the wire, its reference already handed to the channel.
struct SendWire {
    id: String,
    word: String,
    managed: String,
}

/// Evaluate a send's operands into the wire form both `send` and `select` hand the
/// runtime.
fn send_wire(cg: &mut Codegen, channel: &Expr, value: &Expr) -> Result<SendWire> {
    cg.lowered.channels = true;
    let ch = gen_expr(cg, channel)?;
    let id = handle_word(cg, &ch);
    let v = gen_expr(cg, value)?;
    if v.result_inner.is_some() {
        return Err(CodegenError::unsupported(
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
    // and the channel releases whatever was never received when it is freed.
    let managed = i64::from(v.ty.is_managed_ptr()).to_string();
    crate::arc::escape_retain(cg, &v);
    let v = box_to_i64(cg, v);
    Ok(SendWire {
        id,
        word: v.operand,
        managed,
    })
}

const RECV_WITHOUT_ELEMENT: &str =
    "this channel's element type did not reach `recv`: it was read out of a field or structure \
     whose declared type is a type variable, which carries no element type. Give the field the \
     channel's concrete type";

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
    // `send` retained a reference for the receiver; adopting it here makes
    // this region drop it.
    let id = handle_word(cg, &ch);
    adopt_from_runtime(cg, ch, ("channel_recv", "i8*", &id), RECV_WITHOUT_ELEMENT)
}

/// `select` — one `channel_select` call over the channel arms, then a switch on
/// the arm it performed. A `timeout` or `_` arm runs when the call answers `count`.
/// [CONCURRENCY-SELECT]
pub(crate) fn gen_select(
    cg: &mut Codegen,
    operations: &[SelectOperation],
    arms: &[MatchArm],
) -> Result<Value> {
    cg.lowered.channels = true;
    let mut channels = Vec::new();
    let mut waits = None;
    let mut fallback = None;
    for (operation, arm) in operations.iter().zip(arms) {
        match operation {
            SelectOperation::Recv(channel) => {
                let handle = gen_expr(cg, channel)?;
                let id = handle_word(cg, &handle);
                channels.push(SelectChannel::recv(arm, id, handle));
            }
            SelectOperation::Send { channel, value } => {
                let wire = send_wire(cg, channel, value)?;
                channels.push(SelectChannel::send(arm, wire));
            }
            SelectOperation::Timeout(milliseconds) => {
                let waited = gen_expr(cg, milliseconds)?;
                waits = Some(as_i64(cg, waited)?.operand);
                fallback = Some(arm);
            }
            SelectOperation::Default => {
                waits = Some("0".to_owned());
                fallback = Some(arm);
            }
        }
    }
    let (chosen, received) = select_call(cg, &channels, waits.as_deref().unwrap_or("-1"));
    let (end, mut phi_in, mark) = crate::pattern::select_state(cg);
    let labels = channels
        .iter()
        .map(|_| cg.fresh_label())
        .collect::<Vec<_>>();
    let none = cg.fresh_label();
    let cases = labels
        .iter()
        .enumerate()
        .map(|(position, label)| format!("i64 {position}, label %{label}"))
        .collect::<Vec<_>>()
        .join(" ");
    cg.emit(format!("switch i64 {chosen}, label %{none} [ {cases} ]"));
    for (channel, label) in channels.iter().zip(&labels) {
        cg.start_block(label);
        let handle = channel.received_from.clone();
        crate::pattern::select_arm(cg, channel.arm, &mut phi_in, |cg| {
            let Some(handle) = handle else {
                return Ok(());
            };
            let word = cg.emit_reg(format!("load i64, i64* {received}"));
            let value = adopt_word(cg, handle, &word, RECV_WITHOUT_ELEMENT)?;
            if let Pattern::Binding(name) = &channel.arm.pattern {
                cg.bind(name.clone(), value);
            }
            Ok(())
        })?;
    }
    cg.start_block(&none);
    match fallback {
        Some(arm) => crate::pattern::select_arm(cg, arm, &mut phi_in, |_| Ok(()))?,
        None => cg.emit("unreachable"),
    }
    crate::pattern::select_finish(cg, &phi_in, &end, mark)
}

/// One channel arm of a `select`, with its operands already evaluated.
struct SelectChannel<'a> {
    arm: &'a MatchArm,
    kind: &'static str,
    id: String,
    word: String,
    received_from: Option<Value>,
}

impl<'a> SelectChannel<'a> {
    fn recv(arm: &'a MatchArm, id: String, handle: Value) -> Self {
        Self {
            arm,
            kind: "0",
            id,
            word: "0".to_owned(),
            received_from: Some(handle),
        }
    }

    fn send(arm: &'a MatchArm, wire: SendWire) -> Self {
        Self {
            arm,
            kind: if wire.managed == "1" { "2" } else { "1" },
            id: wire.id,
            word: wire.word,
            received_from: None,
        }
    }
}

/// Fill the runtime's arrays and call it; the chosen index and the received slot.
/// The arrays are entry-block slots: claimed where the `select` is written they
/// were dynamic stack allocations, which the symbolic engine's root pass refuses
/// once the function is inlined ([SYMEX-CORPUS]).
fn select_call(cg: &mut Codegen, channels: &[SelectChannel<'_>], waits: &str) -> (String, String) {
    cg.add_extern("declare i64 @channel_select(i64, i64*, i8**, i64*, i64*, i64)");
    let count = channels.len();
    let array = |cg: &mut Codegen, of: &str, slot: &dyn Fn(&SelectChannel<'_>) -> String| {
        let base = cg.hoist_slot(&format!("[{count} x {of}]"));
        for (position, channel) in channels.iter().enumerate() {
            let at = cg.emit_reg(format!(
                "getelementptr [{count} x {of}], [{count} x {of}]* {base}, i64 0, i64 {position}"
            ));
            cg.emit(format!("store {of} {}, {of}* {at}", slot(channel)));
        }
        cg.emit_reg(format!(
            "getelementptr [{count} x {of}], [{count} x {of}]* {base}, i64 0, i64 0"
        ))
    };
    let kinds = array(cg, "i64", &|channel| channel.kind.to_owned());
    let ids = array(cg, "i8*", &|channel| channel.id.clone());
    let words = array(cg, "i64", &|channel| channel.word.clone());
    let received = cg.hoist_slot("i64");
    let chosen = cg.emit_reg(format!(
        "call i64 @channel_select(i64 {count}, i64* {kinds}, i8** {ids}, i64* {words}, i64* {received}, i64 {waits})"
    ));
    (chosen, received)
}

/// Fiber/channel builtins reached as ordinary calls. Returns `None` when `name`
/// is not one of them.
pub(crate) fn gen_builtin(cg: &mut Codegen, name: &str, args: &[Expr]) -> Result<Option<Value>> {
    let v = match name {
        // `Channel(capacity)` — a real C-runtime channel behind the managed
        // handle the runtime hands over at +1, like a constructor.
        "Channel" => {
            let cap = match args.first() {
                Some(a) => {
                    let v = gen_expr(cg, a)?;
                    as_i64(cg, v)?.operand
                }
                None => String::from("0"),
            };
            // A capacity the runtime refuses answers NULL, which is the
            // `Error` arm: the program decides, nothing aborts
            // [CONCURRENCY-CHANNEL] [FAILURE-EXPLICIT].
            let r = cg.call("i8*", "channel_create", "i64", &[&cap]);
            crate::arc::own(cg, &Value::new(r.clone(), LType::Ptr));
            let failed = format!("icmp eq i8* {r}, null");
            crate::result::result_unless(cg, &r, LType::Ptr, &failed, Some(CHANNEL_REFUSED), None)?
        }
        // `fiber_yield(v)` called as an ordinary function shares `yield`'s
        // lowering — the same runtime hand-off, forwarding `v`.
        "fiber_yield" => gen_yield(cg, args.first())?,
        // `fiberDone(f)` — the C runtime's non-blocking completion probe
        // [CONCURRENCY-YIELD].
        "fiberDone" => {
            let Some(a) = args.first() else {
                return Err(CodegenError::invalid("fiberDone needs a fiber argument"));
            };
            let v = gen_expr(cg, a)?;
            let wire = handle_word(cg, &v);
            let r = cg.call("i64", "fiber_done", "i8*", &[&wire]);
            Value::new(r, LType::I64)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}
