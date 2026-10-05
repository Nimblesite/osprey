//! Pipeline stages: what `map` and `filter` record and what a consumer replays.
//! Implements [BUILTIN-ITER-MAP], [BUILTIN-ITER-FILTER] and [BUILTIN-ITER-FUSION].
//!
//! A stage exists only at compile time. It belongs to the iterator it was
//! applied to, so each `map`/`filter` answers a fresh name for the same
//! iterator and a consumer replays exactly the stages recorded under the name
//! it is given. One function-wide list let a consumer run the stages of every
//! pipeline still pending: a second pipeline stole the first one's `map`.

use super::{callback_of, invoke, nth, Callback, IterOp};
use crate::builder::Codegen;
use crate::conv::as_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::Expr;

/// `map`/`filter`: extend the source iterator's pipeline by one stage. The
/// result is a fresh name for the same iterator, so two pipelines built on one
/// source stay distinct and each consumer replays exactly its own stages.
pub(super) fn record(cg: &mut Codegen, args: &[Expr], is_map: bool) -> Result<Value> {
    let source = gen_expr(cg, nth(args, 0)?)?;
    let stage = nth(args, 1)?;
    let cb = match (stage, callback_of(cg, stage)?) {
        // The stage may run in a later statement: it owns what it closes over.
        (Expr::Lambda { .. }, Callback::Lambda(parameters, body, sig, position)) => {
            let free = crate::closure::free_names(&parameters, &body);
            Callback::Closed(
                crate::closure::capture(cg, free),
                parameters,
                body,
                sig,
                position,
            )
        }
        (_, cb) => cb,
    };
    let mut stages = stages_of(cg, &source);
    stages.push(IterOp { map: is_map, cb });
    let ty = source.llvm_ty();
    let operand = cg.emit_reg(format!("bitcast {ty} {} to {ty}", source.operand));
    let _ = cg.iter_stages.insert(operand.clone(), stages);
    Ok(Value { operand, ..source })
}

/// The stages recorded on `iterator`, in source order.
pub(crate) fn stages_of(cg: &Codegen, iterator: &Value) -> Vec<IterOp> {
    cg.iter_stages
        .get(&iterator.operand)
        .cloned()
        .unwrap_or_default()
}

/// Refuse to let a pipeline leave the function that recorded its stages. They
/// exist only at compile time, so past this point the iterator would run
/// without them: a `map` or `filter` silently dropped [BUILTIN-ITER-FUSION].
pub(crate) fn reject_escape(cg: &Codegen, value: &Value) -> Result<()> {
    if cg.iter_stages.contains_key(&value.operand) {
        return Err(CodegenError::unsupported(
            "a `map`/`filter` pipeline leaving the function that built it (consume it with `forEach` or `fold` first)",
        ));
    }
    Ok(())
}

/// The aliases recorded stages read. A stage may run any number of statements
/// later, so they stay alive for as long as their scope [GC-ARC-PERCEUS].
pub(crate) fn stage_aliases(cg: &Codegen) -> Vec<String> {
    cg.iter_stages
        .values()
        .flatten()
        .flat_map(|op| match &op.cb {
            Callback::Closed(env, ..) => crate::closure::aliases(env),
            _ => Vec::new(),
        })
        .collect()
}

/// Emit `cb(elem)` as a truth test and branch on the result. Returns the
/// `(taken, rejected)` labels; neither block is started, so the caller decides
/// what each edge does. Every filtering combinator tests its predicate here.
pub(super) fn branch_on_predicate(
    cg: &mut Codegen,
    cb: &Callback,
    elem: Value,
) -> Result<(String, String)> {
    let pred = invoke(cg, cb, vec![elem])?;
    let pred = crate::cast::coerce_to(cg, pred, LType::I1)?;
    let pb = as_i64(cg, pred)?;
    let nz = cg.emit_reg(format!("icmp ne i64 {}, 0", pb.operand));
    let taken = cg.fresh_label();
    let rejected = cg.fresh_label();
    cg.emit(format!("br i1 {nz}, label %{taken}, label %{rejected}"));
    Ok((taken, rejected))
}

/// Replay an iterator's map/filter `stages` on element `v` in the current
/// block, branching to `skip` when a filter rejects it. Returns the
/// transformed value.
pub(crate) fn replay(cg: &mut Codegen, stages: &[IterOp], v: Value, skip: &str) -> Result<Value> {
    let mut cur = v;
    for op in stages {
        if op.map {
            cur = invoke(cg, &op.cb, vec![cur])?;
        } else {
            let (pass, reject) = branch_on_predicate(cg, &op.cb, cur.clone())?;
            // The reject edge jumps past the loop body's region close, so it
            // drops the region itself — otherwise every value the preceding
            // map stages owned this iteration leaks [GC-ARC-PERCEUS].
            cg.start_block(&reject);
            crate::arc::drop_frame_inline(cg);
            cg.emit(format!("br label %{skip}"));
            cg.start_block(&pass);
        }
    }
    Ok(cur)
}
