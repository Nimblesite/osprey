//! Buffer construction and host transfer for the GPU surface
//! (docs/specs/runtime/0034-GPUComputation.md): `toGpu` in its three lowerings —
//! literal [GPU-BUFFER-LITERAL], iterator fusion [GPU-BUFFER-FUSE] and list
//! copy [GPU-BUFFER-FROM-LIST] — and `fromGpu` [GPU-BUFFER-TO-LIST] and `gpuGet`
//! [GPU-GET]. These are
//! the host↔device boundary; the combinators that run kernels live in the
//! parent module.

use super::{
    buffer_alloc, buffer_arg, buffer_get, buffer_len, buffer_set, elem_owner, scalar_word,
    tagged_elem,
};
use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::iter::nth;
use crate::llty::{LType, Value};
use crate::loops::{close_range_loop, open_range_loop};
use osprey_ast::Expr;

/// The element `LType` of the list `v` holds, from either list representation:
/// a flat literal's `[]double` or a runtime handle's `List#double`. The copy
/// into a buffer is element-blind without it, so `toGpu` of a `List<float>`
/// value produced a bare-tagged buffer whose reads mis-typed [GPU-BUFFER-ELEM].
fn list_elem(v: &Value) -> Option<LType> {
    crate::llty::elem_of_tag(v, "[]")
        .or_else(|| crate::collections::tagged_elem(v))
        .map(|tag| crate::llty::elem_of_spelling(&tag).0)
}

/// `toGpu([a, b, c])` — a buffer literal [GPU-BUFFER-LITERAL]. The elements are
/// stored straight into the dense buffer at their constant indices, so neither
/// the flat literal block nor an `OspreyList` is ever built: one allocation
/// instead of three, and no copy loop. The element tag comes from the first
/// element's lowered type, exactly as a flat list literal's would — or, for an
/// EMPTY literal, from the element type inference resolved at the literal's own
/// position, so `toGpu([])` in a float context is a float buffer instead of a
/// bare handle whose reads default to `int` [GPU-BUFFER-ELEM].
fn buffer_literal(
    cg: &mut Codegen,
    elements: &[Expr],
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    let pending = super::allocation::Pending::new(cg, &elements.len().to_string());
    let out = &pending.value;
    let mut elem = crate::listlit::inferred_elem(cg, position);
    // A literal element still spelled at its default type is fitted to the
    // published width before boxing ([TYPE-NUMERIC-WORD]).
    let sized = elem.as_deref().and_then(crate::llty::sized_of_spelling);
    for (i, e) in elements.iter().enumerate() {
        let v = gen_expr(cg, e)?;
        let v = crate::conv::fit_literal(cg, v, sized)?;
        elem = elem.or_else(|| Some(crate::llty::scalar_spelling(v.ty).to_string()));
        let word = scalar_word(cg, v, "a GPU buffer literal element")?;
        buffer_set(cg, out, &i.to_string(), &word.operand);
    }
    let elem = elem
        .as_deref()
        .map(|tag| crate::llty::elem_of_spelling(tag).0);
    pending.finish(cg, elem)
}

/// The compaction counter shared by `gpuFilter` and iterator fusion: a stack
/// slot holding how many elements have been kept so far. Both write into a
/// buffer sized at the *source* length, because the kept count is not known
/// until the loop has run.
pub(super) fn kept_counter(cg: &mut Codegen) -> String {
    let kept = cg.hoist_slot("i64");
    cg.emit(format!("store i64 0, i64* {kept}"));
    kept
}

/// Append `word` at the counter's position and advance it.
pub(super) fn push_kept(cg: &mut Codegen, out: &Value, kept: &str, word: &str) {
    let at = cg.emit_reg(format!("load i64, i64* {kept}"));
    buffer_set(cg, out, &at, word);
    let next = cg.emit_reg(format!("add i64 {at}, 1"));
    cg.emit(format!("store i64 {next}, i64* {kept}"));
}

/// Append `word` only when `verdict` (an `i1` operand) holds — one compaction
/// step of `gpuFilter` [GPU-FILTER].
pub(super) fn push_if_kept(cg: &mut Codegen, verdict: &str, out: &Value, kept: &str, word: &str) {
    let skip = cg.open_guard(verdict);
    push_kept(cg, out, kept, word);
    cg.emit(format!("br label %{skip}"));
    cg.start_block(&skip);
}

/// Publish the compacted prefix: the buffer's length becomes the kept count.
pub(super) fn take_kept(cg: &mut Codegen, out: &Value, kept: &str) {
    let total = cg.emit_reg(format!("load i64, i64* {kept}"));
    cg.call_void("osprey_gpu_take", "i8*, i64", &[&out.operand, &total]);
}

/// `toGpu(iterator)` — iterator/buffer fusion [GPU-BUFFER-FUSE]. `toGpu` is a
/// consuming stage of the iterator pipeline: the pending map/filter stages
/// replay inside one counted loop that writes each surviving element straight
/// into the dense buffer, so `range |> map |> toGpu` never materializes a
/// `List`. A filter stage makes the kept count dynamic, so this fills a
/// span-length buffer and publishes the exact prefix, as `gpuFilter` does.
fn fuse_iterator(cg: &mut Codegen, range: &Value) -> Result<Value> {
    let stages = crate::iter::stages_of(cg, range);
    let (start, end) = crate::iter::bounds(cg, range);
    let span = span_of(cg, &start, &end);
    let pending = super::allocation::Pending::new(cg, &span);
    let out = &pending.value;
    let kept = kept_counter(cg);
    let lp = open_range_loop(cg, &start, &end);
    crate::arc::push_frame(cg, crate::arc::FrameKind::Scope);
    let v = crate::iter::replay(cg, &stages, Value::new(lp.i.clone(), LType::I64), &lp.incr)?;
    let elem = v.ty;
    let word = scalar_word(cg, v, "a toGpu element")?;
    push_kept(cg, out, &kept, &word.operand);
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);
    take_kept(cg, out, &kept);
    pending.finish(cg, Some(elem))
}

/// The element count of `[start, end)`. An inverted range yields no elements,
/// so its buffer is empty, not a negative-length allocation [GPU-BUFFER-FUSE].
fn span_of(cg: &mut Codegen, start: &str, end: &str) -> String {
    let span = cg.emit_reg(format!("sub i64 {end}, {start}"));
    let empty = cg.emit_reg(format!("icmp slt i64 {span}, 0"));
    cg.emit_reg(format!("select i1 {empty}, i64 0, i64 {span}"))
}

/// `toGpu(list)` — dense copy of a scalar list [GPU-BUFFER-FROM-LIST]. A flat
/// list literal's element tag (`[]double`) becomes the buffer's element tag.
/// A literal argument takes [`buffer_literal`] and an iterator pipeline
/// [`fuse_iterator`], so neither builds a list on the way to the buffer.
pub(super) fn to_gpu(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let arg = nth(args, 0)?;
    if let Expr::List(elements, position) = arg {
        return buffer_literal(cg, elements, *position);
    }
    let source = gen_expr(cg, arg)?;
    if source.osp_ty.as_deref() == Some(crate::iter::RANGE_OWNER) {
        return fuse_iterator(cg, &source);
    }
    let elem = list_elem(&source);
    let l = crate::listlit::to_runtime_list(cg, source);
    let l = crate::cast::coerce_to(cg, l, LType::Ptr)?;
    let len = cg.call("i64", "osprey_list_length", "i8*", &[&l.operand]);
    let buf = buffer_alloc(cg, &len, elem_owner(elem));
    let lp = open_range_loop(cg, "0", &len);
    let elem = cg.call("i64", "osprey_list_get", "i8*, i64", &[&l.operand, &lp.i]);
    buffer_set(cg, &buf, &lp.i, &elem);
    close_range_loop(cg, &lp);
    Ok(buf)
}

/// `fromGpu(buffer)` — materialize a buffer as a list [GPU-BUFFER-TO-LIST].
/// Elements are scalars, so the builder's kind is unmanaged.
pub(super) fn from_gpu(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let buffer = buffer_arg(cg, args, 0)?;
    let (status, reason) = read_status(cg, &buffer, "0");
    let unreadable = cg.emit_reg(format!("icmp slt i32 {status}, 0"));
    let missing = cg.emit_reg(format!("icmp eq i8* {}, null", buffer.operand));
    let failed = cg.emit_reg(format!("or i1 {unreadable}, {missing}"));
    let count = buffer_len(cg, &buffer);
    let len = cg.emit_reg(format!("select i1 {failed}, i64 0, i64 {count}"));
    let values = materialize(cg, &buffer, &len)?;
    crate::result::make_result_if_err_because(
        cg,
        values,
        LType::Ptr,
        &failed,
        Some("fromGpu: buffer unavailable"),
        Some(&reason),
    )
}

/// The checked caller supplies zero iterations on Error; its inactive list
/// payload is never published as Success. Preserve the scalar element owner.
fn materialize(cg: &mut Codegen, b: &Value, len: &str) -> Result<Value> {
    let bld = crate::collections::list_builder_new_of(cg, "0");
    let lp = open_range_loop(cg, "0", len);
    let elem = buffer_get(cg, b, &lp.i);
    // [GPU-BUFFER-TO-LIST]: signed narrow words regain canonical sign extension.
    let value = super::elem_value(cg, elem, tagged_elem(b));
    let word = scalar_word(cg, value, "a fromGpu element")?;
    crate::collections::list_builder_push(cg, &bld, &word.operand, "0");
    close_range_loop(cg, &lp);
    let sealed = crate::collections::list_builder_seal(cg, &bld);
    Ok(sealed.with_owner(Some(crate::collections::list_owner(
        tagged_elem(b).map(crate::llty::scalar_spelling),
    ))))
}

/// [GPU-GET] [FAILURE-EXPLICIT]: a download failure is an Error, not a value.
pub(super) fn get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let buffer = buffer_arg(cg, args, 0)?;
    let index = gen_expr(cg, nth(args, 1)?)?;
    let index = crate::conv::as_i64(cg, index)?;
    let (status, reason) = read_status(cg, &buffer, &index.operand);
    let failed = cg.emit_reg(format!("icmp sle i32 {status}, 0"));
    let raw = guarded_get(cg, &buffer, &index.operand, &failed);
    let value = super::elem_value(cg, raw, tagged_elem(&buffer));
    let inner = value.ty;
    crate::result::make_result_if_err_because(
        cg,
        value,
        inner,
        &failed,
        Some("gpuGet: index out of bounds"),
        Some(&reason),
    )
}

fn read_status(cg: &mut Codegen, buffer: &Value, index: &str) -> (String, String) {
    crate::extern_call::clear_io_error(cg);
    let status = cg.call(
        "i32",
        "osprey_gpu_readable",
        "i8*, i64",
        &[&buffer.operand, index],
    );
    let reason = crate::extern_call::take_io_error(cg);
    (status, reason)
}

/// Do not evaluate the raw accessor in either error branch. The zero word
/// only fills the inactive payload of Error, never a published Success.
fn guarded_get(cg: &mut Codegen, buffer: &Value, index: &str, failed: &str) -> String {
    let entry = cg.cur_block().to_owned();
    let (read, join) = (cg.fresh_label(), cg.fresh_label());
    cg.emit(format!("br i1 {failed}, label %{join}, label %{read}"));
    cg.start_block(&read);
    let value = buffer_get(cg, buffer, index);
    let ready = cg.snapshot_to(&join);
    cg.start_block(&join);
    cg.emit_reg(format!("phi i64 [ 0, %{entry} ], [ {value}, %{ready} ]"))
}
