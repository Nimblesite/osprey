//! Buffer construction and host transfer for the GPU surface
//! (docs/specs/runtime/0034-GPUComputation.md): `toGpu` in its three lowerings —
//! literal [GPU-BUFFER-LITERAL], iterator fusion [GPU-BUFFER-FUSE] and list
//! copy [GPU-BUFFER-FROM-LIST] — and `fromGpu` [GPU-BUFFER-TO-LIST]. These are
//! the host↔device boundary; the combinators that run kernels live in the
//! parent module.

use super::{
    buffer_alloc, buffer_arg, buffer_get, buffer_len, buffer_set, elem_owner, owner_of,
    scalar_word, tagged_elem, GPU_OWNER,
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
    let out = buffer_alloc(cg, &elements.len().to_string(), GPU_OWNER.to_string());
    let mut elem = crate::listlit::inferred_elem(cg, position);
    for (i, e) in elements.iter().enumerate() {
        let v = gen_expr(cg, e)?;
        elem = elem.or_else(|| crate::llty::scalar_spelling(v.ty).map(str::to_string));
        let word = scalar_word(cg, v, "a GPU buffer literal element")?;
        buffer_set(cg, &out, &i.to_string(), &word.operand);
    }
    Ok(out.with_owner(Some(owner_of(elem.as_deref()))))
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
    let out = buffer_alloc(cg, &span, GPU_OWNER.to_string());
    let kept = kept_counter(cg);
    let lp = open_range_loop(cg, &start, &end);
    crate::arc::push_frame(cg, crate::arc::FrameKind::Scope);
    let v = crate::iter::replay(cg, &stages, Value::new(lp.i.clone(), LType::I64), &lp.incr)?;
    let elem = v.ty;
    let word = scalar_word(cg, v, "a toGpu element")?;
    push_kept(cg, &out, &kept, &word.operand);
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);
    take_kept(cg, &out, &kept);
    Ok(out.with_owner(Some(elem_owner(Some(elem)))))
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
    let b = buffer_arg(cg, args, 0)?;
    let bld = crate::collections::list_builder_new_of(cg, "0");
    let len = buffer_len(cg, &b);
    let lp = open_range_loop(cg, "0", &len);
    let elem = buffer_get(cg, &b, &lp.i);
    crate::collections::list_builder_push(cg, &bld, &elem, "0");
    close_range_loop(cg, &lp);
    // The buffer's element type crosses to the list's owner tag: a float buffer
    // materializes as a `List<float>` that reads back as floats, not as the
    // IEEE-754 words the uniform element ABI stores [GPU-BUFFER-TO-LIST].
    let sealed = crate::collections::list_builder_seal(cg, &bld);
    Ok(sealed.with_owner(Some(crate::collections::list_owner(
        tagged_elem(&b).and_then(crate::llty::scalar_spelling),
    ))))
}
