//! Iterator builtins: integer `range`, the stream-fused higher-order operations
//! (`map`/`filter`/`forEach`/`fold`) and the eager list operations
//! (`forEachList`/`mapList`/`filterList`/`foldList`). A range is a stack
//! `{ i64, i64 }` (start, end); `map`/`filter` record a pending stage and pass
//! the range through; the consuming `forEach`/`fold` emits one counted loop
//! replaying those stages, so no intermediate collection is ever materialised.
//! Implements [BUILTIN-ITER-*].

use crate::builder::{Codegen, FnSig};
use crate::conv::{as_i64, box_to_i64};
use crate::error::{CodegenError, Result};
use crate::expr::{apply_lambda_values, call_with_values, gen_expr};
use crate::llty::{LType, Value};
use crate::loops::{close_list_loop, close_range_loop, open_list_loop, open_range_loop};
use osprey_ast::{Expr, NamedArgument, Parameter};

const RANGE_TY: &str = "{ i64, i64 }";
const RANGE_OWNER: &str = "Range";

/// A recorded stream-fusion stage.
#[derive(Clone)]
pub(crate) struct IterOp {
    pub map: bool, // true = map (transform), false = filter (predicate)
    pub cb: Callback,
}

/// An iterator callback in any of its three spellings.
#[derive(Clone)]
pub(crate) enum Callback {
    /// A top-level function/builtin name — a direct call.
    Named(String),
    /// An inline (or let-bound) lambda — beta-reduced per element, so its
    /// captures resolve in the enclosing scope.
    Lambda(Vec<Parameter>, Expr),
    /// A function-typed local (closure value) — called through its cell.
    Local(String, FnSig),
    /// A computed closure value (a call result like `makeAdder(1)` or a field
    /// access like `cfg.processor`) — already evaluated to a handle operand,
    /// called through its cell per element.
    Value(String, FnSig),
}

/// Resolve an iterator callback argument to a [`Callback`]. A materialized
/// closure value wins over the beta-reduction cache — the cell carries the
/// captures snapshotted at creation. A computed callback (call result or field
/// access) is evaluated once here to a closure handle.
fn callback_of(cg: &mut Codegen, e: &Expr) -> Result<Callback> {
    match e {
        Expr::Lambda {
            parameters, body, ..
        } => Ok(Callback::Lambda(parameters.clone(), (**body).clone())),
        Expr::Identifier(n) => match cg.fn_ptr_locals.get(n) {
            Some(sig) => Ok(Callback::Local(n.clone(), sig.clone())),
            None => match cg.lambdas.get(n) {
                Some((params, body)) => Ok(Callback::Lambda(params.clone(), body.clone())),
                None => Ok(Callback::Named(n.clone())),
            },
        },
        _ => {
            let sig = cg
                .callee_fn_type(e)
                .as_ref()
                .and_then(Codegen::fn_value_sig)
                .ok_or_else(|| {
                    CodegenError::unsupported("iterator callback must be a function name or lambda")
                })?;
            let handle = gen_expr(cg, e)?;
            Ok(Callback::Value(handle.operand, sig))
        }
    }
}

/// Apply a callback to already-evaluated argument values.
fn invoke(cg: &mut Codegen, cb: &Callback, args: Vec<Value>) -> Result<Value> {
    match cb {
        Callback::Named(name) => call_with_values(cg, name, args),
        Callback::Lambda(params, body) => apply_lambda_values(cg, params, body, args),
        Callback::Local(name, sig) => {
            let handle = cg.lookup(name).ok_or_else(|| CodegenError::unknown(name))?;
            let typed = crate::closure::coerce_typed_args(cg, sig, args)?;
            Ok(crate::closure::cell_call(cg, &handle.operand, sig, &typed))
        }
        Callback::Value(operand, sig) => {
            let typed = crate::closure::coerce_typed_args(cg, sig, args)?;
            Ok(crate::closure::cell_call(cg, operand, sig, &typed))
        }
    }
}

/// Dispatch an iterator builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Option<Value>> {
    let v = match name {
        "range" => range(cg, args)?,
        "map" => record(cg, args, true)?,
        "filter" => record(cg, args, false)?,
        "forEach" => for_each(cg, args)?,
        "fold" => fold(cg, args)?,
        "forEachList" => for_each_list(cg, args)?,
        "mapList" => list_builder(cg, args, false)?,
        "filterList" => list_builder(cg, args, true)?,
        "foldList" => fold_list(cg, args)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn nth(args: &[Expr], i: usize) -> Result<&Expr> {
    args.get(i)
        .ok_or_else(|| CodegenError::invalid("iterator builtin: missing argument"))
}

/// `range(start, end)` → a `{ start, end }` block (half-open, step 1).
fn range(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let s = gen_expr(cg, nth(args, 0)?)?;
    let s = as_i64(cg, s)?;
    let e = gen_expr(cg, nth(args, 1)?)?;
    let e = as_i64(cg, e)?;
    let obj = cg.malloc_struct(RANGE_TY, crate::meta::KIND_RAW);
    crate::aggregate::store_field(cg, RANGE_TY, &obj, 0, LType::I64, &s.operand);
    crate::aggregate::store_field(cg, RANGE_TY, &obj, 1, LType::I64, &e.operand);
    Ok(Value::handle(obj, RANGE_OWNER))
}

/// `map`/`filter`: record a pending stage and return the iterator unchanged.
fn record(cg: &mut Codegen, args: &[Expr], is_map: bool) -> Result<Value> {
    let iter = gen_expr(cg, nth(args, 0)?)?;
    let cb = callback_of(cg, nth(args, 1)?)?;
    cg.pending_iter_ops.push(IterOp { map: is_map, cb });
    Ok(iter)
}

/// Load a range block's `(start, end)` bounds.
fn bounds(cg: &mut Codegen, range: &Value) -> (String, String) {
    let s = crate::aggregate::load_field(cg, RANGE_TY, &range.operand, 0, LType::I64);
    let e = crate::aggregate::load_field(cg, RANGE_TY, &range.operand, 1, LType::I64);
    (s, e)
}

/// Replay the pending map/filter stages on element `v` in the current block,
/// branching to `skip` when a filter rejects it. Returns the transformed value.
fn replay(cg: &mut Codegen, v: Value, skip: &str) -> Result<Value> {
    let ops = std::mem::take(&mut cg.pending_iter_ops);
    let mut cur = v;
    for op in &ops {
        if op.map {
            cur = invoke(cg, &op.cb, vec![cur])?;
            cur = crate::result::unwrap(cg, cur);
        } else {
            let pred = invoke(cg, &op.cb, vec![cur.clone()])?;
            let pred = crate::result::unwrap(cg, pred);
            let pb = as_i64(cg, pred)?;
            let nz = cg.fresh_reg();
            cg.emit(format!("{nz} = icmp ne i64 {}, 0", pb.operand));
            let pass = cg.fresh_label();
            cg.emit(format!("br i1 {nz}, label %{pass}, label %{skip}"));
            cg.start_block(&pass);
        }
    }
    Ok(cur)
}

/// `forEach(iterator, fn)` — counted loop applying `fn` to each (fused) element.
fn for_each(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let range = gen_expr(cg, nth(args, 0)?)?;
    let consumer = callback_of(cg, nth(args, 1)?)?;
    let (start, end) = bounds(cg, &range);

    let lp = open_range_loop(cg, &start, &end);
    // Per-iteration ARC region: values the body owns drop before the
    // back-edge, so slots are reusable next iteration [GC-ARC-PERCEUS].
    crate::arc::push_frame(cg);
    let elem = replay(cg, Value::new(lp.i.clone(), LType::I64), &lp.incr)?;
    let _ = invoke(cg, &consumer, vec![elem])?;
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);
    Ok(range)
}

/// An `i64` accumulator slot seeded with a `fold` builtin's `initial` (the 2nd
/// argument), evaluated, unwrapped and boxed.
fn acc_init(cg: &mut Codegen, args: &[Expr]) -> Result<String> {
    let initial = gen_expr(cg, nth(args, 1)?)?;
    let initial = crate::result::unwrap(cg, initial);
    // A pointer accumulator escapes into the loop-carried slot: dup it so the
    // per-iteration region drop cannot free it [GC-ARC-PERCEUS].
    crate::arc::escape_retain(cg, &initial);
    let initial = box_to_i64(cg, initial);
    let acc = cg.fresh_reg();
    cg.emit(format!("{acc} = alloca i64"));
    cg.emit(format!("store i64 {}, i64* {acc}", initial.operand));
    Ok(acc)
}

/// One fold step: load the accumulator, apply `combine(acc, elem)`, box the
/// result back into the slot.
fn acc_step(cg: &mut Codegen, acc: &str, combine: &Callback, elem: Value) -> Result<()> {
    let a = cg.emit_reg(format!("load i64, i64* {acc}"));
    let new = invoke(cg, combine, vec![Value::new(a.clone(), LType::I64), elem])?;
    let new = crate::result::unwrap(cg, new);
    // Loop-carried slot rebind for a pointer accumulator: dup the incoming
    // value BEFORE dropping the outgoing one (a combine returning `acc`
    // unchanged must never free it). Keyed on the static type — an integer
    // accumulator's bits must never be released [GC-ARC-PERCEUS].
    if matches!(new.ty, LType::Str | LType::Ptr) {
        crate::arc::escape_retain(cg, &new);
        let old = cg.emit_reg(format!("inttoptr i64 {a} to i8*"));
        crate::arc::release_operand(cg, &old);
    }
    let new = box_to_i64(cg, new);
    cg.emit(format!("store i64 {}, i64* {acc}", new.operand));
    Ok(())
}

/// Read the final accumulator value as an `i64`.
fn acc_result(cg: &mut Codegen, acc: &str) -> Value {
    Value::new(cg.emit_reg(format!("load i64, i64* {acc}")), LType::I64)
}

/// `fold(iterator, initial, fn)` — counted loop accumulating `fn(acc, elem)`.
fn fold(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let range = gen_expr(cg, nth(args, 0)?)?;
    let acc = acc_init(cg, args)?;
    let combine = callback_of(cg, nth(args, 2)?)?;
    let (start, end) = bounds(cg, &range);

    let lp = open_range_loop(cg, &start, &end);
    crate::arc::push_frame(cg);
    let elem = replay(cg, Value::new(lp.i.clone(), LType::I64), &lp.incr)?;
    acc_step(cg, &acc, &combine, elem)?;
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);

    Ok(acc_result(cg, &acc))
}

/// The `i`-th positional argument as a list handle.
fn list_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let v = gen_expr(cg, nth(args, i)?)?;
    crate::cast::coerce_to(cg, v, LType::Ptr)
}

/// `forEachList(list, fn)` — call `fn` on each element in order.
fn for_each_list(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let l = list_arg(cg, args, 0)?;
    let consumer = callback_of(cg, nth(args, 1)?)?;
    let lp = open_list_loop(cg, &l.operand);
    crate::arc::push_frame(cg);
    let _ = invoke(cg, &consumer, vec![Value::new(lp.elem.clone(), LType::I64)])?;
    crate::arc::pop_frame(cg);
    close_list_loop(cg, &lp);
    Ok(l)
}

/// `mapList`/`filterList` — build a new list via the runtime list builder.
fn list_builder(cg: &mut Codegen, args: &[Expr], filter: bool) -> Result<Value> {
    let l = list_arg(cg, args, 0)?;
    let f = callback_of(cg, nth(args, 1)?)?;
    let bld = crate::collections::list_builder_new(cg);
    let lp = open_list_loop(cg, &l.operand);
    crate::arc::push_frame(cg);
    let elem = Value::new(lp.elem.clone(), LType::I64);
    if filter {
        let pred = invoke(cg, &f, vec![elem.clone()])?;
        let pred = crate::result::unwrap(cg, pred);
        let pb = as_i64(cg, pred)?;
        let nz = cg.fresh_reg();
        cg.emit(format!("{nz} = icmp ne i64 {}, 0", pb.operand));
        let push = cg.fresh_label();
        let skip = cg.fresh_label();
        cg.emit(format!("br i1 {nz}, label %{push}, label %{skip}"));
        cg.start_block(&push);
        crate::collections::list_builder_push(cg, &bld, &lp.elem);
        cg.emit(format!("br label %{skip}"));
        cg.start_block(&skip);
    } else {
        let mapped = invoke(cg, &f, vec![elem])?;
        let mapped = crate::result::unwrap(cg, mapped);
        // The built list stores the mapped element: dup it before the
        // per-iteration region drop [GC-ARC-PERCEUS].
        crate::arc::escape_retain(cg, &mapped);
        let boxed = box_to_i64(cg, mapped);
        crate::collections::list_builder_push(cg, &bld, &boxed.operand);
    }
    crate::arc::pop_frame(cg);
    close_list_loop(cg, &lp);
    Ok(crate::collections::list_builder_seal(cg, &bld))
}

/// `foldList(list, initial, fn)` — reduce a list with `fn(acc, elem)`.
fn fold_list(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let l = list_arg(cg, args, 0)?;
    let acc = acc_init(cg, args)?;
    let combine = callback_of(cg, nth(args, 2)?)?;
    let lp = open_list_loop(cg, &l.operand);
    crate::arc::push_frame(cg);
    acc_step(cg, &acc, &combine, Value::new(lp.elem.clone(), LType::I64))?;
    crate::arc::pop_frame(cg);
    close_list_loop(cg, &lp);
    Ok(acc_result(cg, &acc))
}
