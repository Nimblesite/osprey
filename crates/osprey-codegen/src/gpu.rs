//! GPU computation builtins — the host execution backend [GPU-BACKEND-HOST]
//! (docs/specs/runtime/0034-GPUComputation.md). `toGpu`/`fromGpu` copy between
//! persistent lists and dense scalar buffers; the combinators (`gpuMap`,
//! `gpuFold`, `gpuZipWith`, `gpuScan`, `gpuFilter`, `gpuIota`, `gpuGet`) run
//! kernels as native counted loops over those buffers. The dense unboxed
//! layout is the staging layout a device transfer will use; kernels reaching
//! codegen are already proven pure by the effect checker [GPU-KERNEL-PURE].

mod buffer;
mod io;

use crate::builder::Codegen;
use crate::conv::box_to_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::gpu_device::{declined, launch_fold, launch_map};
use crate::gpu_kernel::{extract, kernel_elem, kernel_of, slot};
use crate::iter::{acc_init, acc_result, acc_step, invoke, nth, Callback};
use crate::llty::{LType, Value};
use crate::loops::{close_range_loop, open_range_loop};
use buffer::{kept_counter, push_if_kept, take_kept};
use osprey_ast::{Expr, NamedArgument};

/// The owner tag GPU buffer handles travel under when their element type is
/// not statically known.
const GPU_OWNER: &str = "GpuBuffer";

/// Whether `ty` is a buffer word — `int`, `float` or `bool`. This is the
/// device sublanguage's element restriction [GPU-BUFFER-ELEM], spelled ONCE:
/// element data, kernel results, reduction accumulators and extracted-kernel
/// uniforms ([`crate::gpu_kernel`]) all consult this predicate.
pub(crate) fn is_scalar(ty: LType) -> bool {
    matches!(ty, LType::I64 | LType::Double | LType::I1)
}

/// Whether `v` is a bare scalar word — [`is_scalar`] with no `Result` wrapper,
/// which would travel as an erased pointer the flat ABI has no slot for.
fn is_scalar_value(v: &Value) -> bool {
    v.result_inner.is_none() && is_scalar(v.ty)
}

/// Owner-tag prefix marking a buffer whose element `LType` is known; the suffix
/// is the element's LLVM spelling (`Gpu#double`), mirroring the flat
/// list-literal convention (`[]double`) so the tag survives lets, pipes,
/// parameters and returns.
pub(crate) const GPU_TAG: &str = "Gpu#";

/// The owner tag for a buffer whose element has the given tag spelling, or the
/// bare owner when the element is unknown — the one place `Gpu#` is joined to
/// a spelling, so a static type, a lowered `LType` and an inferred literal
/// element all tag their handles identically.
fn owner_of(elem: Option<&str>) -> String {
    crate::llty::elem_tagged_owner(GPU_TAG, GPU_OWNER, elem)
}

/// The owner tag for a `GpuBuffer<elem>` static type: element-typed when the
/// element is a concrete scalar, the bare owner otherwise.
pub(crate) fn buffer_owner(
    prog: &osprey_types::ProgramTypes,
    elem: Option<&osprey_types::Type>,
) -> String {
    owner_of(crate::types::elem_tag(prog, elem).as_deref())
}

/// The element `LType` recorded on a buffer value's owner tag, if any. A buffer
/// element is always a scalar, so the descriptor's owner half is never set.
pub(crate) fn tagged_elem(v: &Value) -> Option<LType> {
    crate::llty::elem_of_tag(v, GPU_TAG).map(|tag| crate::llty::elem_of_spelling(&tag).0)
}

/// Whether an owner tag names a GPU buffer handle — element-typed (`Gpu#double`)
/// or bare — the only pointer admissible in the extracted kernel ABI
/// [GPU-KERNEL-EXTRACT].
pub(crate) fn is_buffer_owner(owner: &str) -> bool {
    owner == GPU_OWNER || owner.starts_with(GPU_TAG)
}

/// The owner tag for a buffer holding `elem` words.
fn elem_owner(elem: Option<LType>) -> String {
    owner_of(elem.and_then(crate::llty::scalar_spelling))
}

/// Dispatch a GPU builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Option<Value>> {
    if name == "gpuDevice" {
        let name = cg.call("i8*", "osprey_gpu_device_name", "", &[]);
        return Ok(Some(Value::new(name, LType::Str)));
    }
    operation(name).map(|lower| lower(cg, args)).transpose()
}

/// Resolve one lowering function; argument evaluation stays in that function.
type Lowering = fn(&mut Codegen, &[Expr]) -> Result<Value>;

fn operation(name: &str) -> Option<Lowering> {
    Some(match name {
        "toGpu" => buffer::to_gpu,
        "fromGpu" => buffer::from_gpu,
        "gpuLength" => gpu_length,
        "gpuMap" => gpu_map,
        "gpuFold" => gpu_fold,
        "gpuZipWith" => gpu_zip_with,
        "gpuIota" => gpu_iota,
        "gpuGet" => gpu_get,
        "gpuScan" => gpu_scan,
        "gpuFilter" => gpu_filter,
        "gpuMatMul" => io::mat_mul,
        "gpuLoad" => io::load,
        "gpuLoadTensor" => io::load_tensor,
        _ => return None,
    })
}

/// The `i`-th positional argument as an evaluated buffer handle.
fn buffer_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let v = gen_expr(cg, nth(args, i)?)?;
    crate::cast::coerce_to(cg, v, LType::Ptr)
}

/// A freshly allocated buffer of `len` elements, owned by the current region
/// and tagged with its element's owner tag.
fn buffer_alloc(cg: &mut Codegen, len: &str, owner: String) -> Value {
    let raw = cg.call("i8*", "osprey_gpu_alloc", "i64", &[len]);
    let v = Value::handle(raw, owner);
    crate::arc::own(cg, &v);
    v
}

fn buffer_len(cg: &mut Codegen, buf: &Value) -> String {
    cg.call("i64", "osprey_gpu_len", "i8*", &[&buf.operand])
}

fn buffer_get(cg: &mut Codegen, buf: &Value, index: &str) -> String {
    cg.call("i64", "osprey_gpu_get", "i8*, i64", &[&buf.operand, index])
}

fn buffer_set(cg: &mut Codegen, buf: &Value, index: &str, word: &str) {
    cg.call_void(
        "osprey_gpu_set",
        "i8*, i64, i64",
        &[&buf.operand, index, word],
    );
}

/// A buffer word recovered at the kernel's element type: a `float` element's
/// raw bits become a `double` operand (never an integer conversion), a `bool`
/// becomes `i1`, and an unknown element passes through as the raw word.
fn elem_value(cg: &mut Codegen, raw: String, elem: Option<LType>) -> Value {
    crate::conv::from_word(cg, raw, elem.and_then(crate::llty::scalar_spelling))
}

/// The scalar-element backstop [GPU-BUFFER-ELEM]: a buffer word is an `int`,
/// `float`, or `bool`, boxed to its raw `i64` word. An unhandled `Result` gets
/// its own diagnostic, because the fix (handle it inside the kernel) differs
/// from a non-scalar's.
fn scalar_word(cg: &mut Codegen, v: Value, what: &str) -> Result<Value> {
    if v.result_inner.is_some() {
        return Err(CodegenError::unsupported(format!(
            "{what} cannot be an unhandled Result; handle it inside the kernel"
        )));
    }
    if !is_scalar(v.ty) {
        return Err(CodegenError::unsupported(format!(
            "{what} must be a scalar (int, float, or bool)"
        )));
    }
    Ok(box_to_i64(cg, v))
}

/// `gpuLength(buffer)` — the element count [GPU-BUFFER-LENGTH].
fn gpu_length(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let b = buffer_arg(cg, args, 0)?;
    let len = buffer_len(cg, &b);
    Ok(Value::new(len, LType::I64))
}

/// The shared chassis of `gpuMap`/`gpuFold`: one framed counted loop over
/// `src`, handing `body` each element recovered at `elem`'s type plus the
/// loop index.
fn kernel_loop(
    cg: &mut Codegen,
    src: &Value,
    len: &str,
    elem: Option<LType>,
    mut body: impl FnMut(&mut Codegen, Value, &str) -> Result<()>,
) -> Result<()> {
    let lp = open_range_loop(cg, "0", len);
    crate::arc::push_frame(cg, crate::arc::FrameKind::Scope);
    let raw = buffer_get(cg, src, &lp.i);
    let arg = elem_value(cg, raw, elem);
    body(cg, arg, &lp.i)?;
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);
    Ok(())
}

/// Branch to the host loop only when the runtime did not run the kernel on
/// the device ([GPU-BACKEND-DEVICE]); the loop rejoins at the returned label.
fn host_fallback(cg: &mut Codegen, done: &str) -> String {
    let (host, join) = (cg.fresh_label(), cg.fresh_label());
    let ran = cg.emit_reg(format!("icmp ne i32 {done}, 0"));
    cg.emit(format!("br i1 {ran}, label %{join}, label %{host}"));
    cg.start_block(&host);
    join
}

fn rejoin(cg: &mut Codegen, join: &str) {
    cg.emit(format!("br label %{join}"));
    cg.start_block(join);
}

/// The shared chassis of `gpuMap`/`gpuZipWith`: `launch` asks the device to
/// fill the fresh `len`-element buffer, and otherwise `apply` runs at every
/// index of `src` and each scalar result is stored at that index. The
/// kernel's result `LType` is codegen-time knowledge, so the handle is
/// retagged with it and downstream combinators recover the element without a
/// kernel signature.
fn map_into(
    cg: &mut Codegen,
    src: &Value,
    len: &str,
    elem: Option<LType>,
    what: &str,
    launch: impl FnOnce(&mut Codegen, &Value) -> String,
    mut apply: impl FnMut(&mut Codegen, Value, &str) -> Result<Value>,
) -> Result<Value> {
    let out = buffer_alloc(cg, len, GPU_OWNER.to_string());
    let done = launch(cg, &out);
    let join = host_fallback(cg, &done);
    let mut mapped_ty = LType::I64;
    kernel_loop(cg, src, len, elem, |cg, arg, i| {
        let mapped = apply(cg, arg, i)?;
        mapped_ty = mapped.ty;
        let stored = scalar_word(cg, mapped, what)?;
        buffer_set(cg, &out, i, &stored.operand);
        Ok(())
    })?;
    rejoin(cg, &join);
    Ok(out.with_owner(Some(elem_owner(Some(mapped_ty)))))
}

/// `(buffer, kernel)` — the preamble `gpuMap` and `gpuFilter` share: the source
/// handle, its element type, and the kernel extracted over that one element
/// slot. The kernel is lifted before the launch site is built, so its `define`
/// precedes the loop that calls it [GPU-KERNEL-EXTRACT].
fn unary_kernel(cg: &mut Codegen, args: &[Expr]) -> Result<(Value, Option<LType>, Callback)> {
    let src = buffer_arg(cg, args, 0)?;
    let (kernel, elem) = kernel_of(cg, args, 1, &src, 0)?;
    let kernel = extract(cg, kernel, &[slot(elem)])?;
    Ok((src, elem, kernel))
}

/// `gpuMap(buffer, kernel)` — apply a pure kernel to every element [GPU-MAP].
fn gpu_map(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let (src, elem, kernel) = unary_kernel(cg, args)?;
    let len = buffer_len(cg, &src);
    map_into(
        cg,
        &src,
        &len,
        elem,
        "a gpuMap kernel result",
        |cg, out| launch_map(cg, "gpuMap", &kernel, &[slot(elem)], &[&src], out, &len),
        |cg, arg, _| invoke(cg, &kernel, vec![arg]),
    )
}

/// `gpuZipWith(a, b, kernel)` — elementwise binary combination [GPU-ZIPWITH].
fn gpu_zip_with(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let left = buffer_arg(cg, args, 0)?;
    let right = buffer_arg(cg, args, 1)?;
    let (kernel, left_elem) = kernel_of(cg, args, 2, &left, 0)?;
    let right_elem = kernel_elem(cg, nth(args, 2)?, &kernel, &right, 1);
    let kernel = extract(cg, kernel, &[slot(left_elem), slot(right_elem)])?;
    let len = shorter_len(cg, &left, &right);
    let slots = [slot(left_elem), slot(right_elem)];
    map_into(
        cg,
        &left,
        &len,
        left_elem,
        "a gpuZipWith kernel result",
        |cg, out| {
            launch_map(
                cg,
                "gpuZipWith",
                &kernel,
                &slots,
                &[&left, &right],
                out,
                &len,
            )
        },
        |cg, a, i| {
            let raw = buffer_get(cg, &right, i);
            let b = elem_value(cg, raw, right_elem);
            invoke(cg, &kernel, vec![a, b])
        },
    )
}

/// The shorter of two buffers' lengths, so a ragged pair truncates rather than
/// reading past the end [GPU-ZIPWITH].
fn shorter_len(cg: &mut Codegen, left: &Value, right: &Value) -> String {
    let left_len = buffer_len(cg, left);
    let right_len = buffer_len(cg, right);
    let shorter = cg.emit_reg(format!("icmp slt i64 {left_len}, {right_len}"));
    cg.emit_reg(format!(
        "select i1 {shorter}, i64 {left_len}, i64 {right_len}"
    ))
}

/// The pieces every reduction shares [GPU-FOLD] [GPU-SCAN]: the source buffer
/// and its trip count, the scalar accumulator slot with the type template that
/// recovers it, the extracted combine kernel, and the element type.
struct Reduction {
    src: Value,
    len: String,
    acc: String,
    tmpl: Value,
    combine: Callback,
    elem: Option<LType>,
}

/// `(buffer, initial, combine)` — the preamble `gpuFold` and `gpuScan` share.
/// The accumulator is restricted to a scalar word so every accepted reduction
/// stays offloadable when device reductions land, and the combine kernel is
/// extracted with the accumulator slot first [GPU-KERNEL-EXTRACT].
fn reduction(cg: &mut Codegen, args: &[Expr], what: &str) -> Result<Reduction> {
    let src = buffer_arg(cg, args, 0)?;
    let (acc, tmpl) = acc_init(cg, args)?;
    if !is_scalar_value(&tmpl) {
        return Err(CodegenError::unsupported(format!(
            "a {what} accumulator must be a scalar (int, float, or bool)"
        )));
    }
    let (combine, elem) = kernel_of(cg, args, 2, &src, 1)?;
    let combine = extract(cg, combine, &[tmpl.ty, slot(elem)])?;
    let len = buffer_len(cg, &src);
    Ok(Reduction {
        src,
        len,
        acc,
        tmpl,
        combine,
        elem,
    })
}

/// One reduction step: fold the current element into the accumulator slot.
fn step(cg: &mut Codegen, r: &Reduction, arg: Value) -> Result<()> {
    acc_step(cg, &r.acc, &r.tmpl, &r.combine, arg)
}

/// `gpuFold(buffer, initial, combine)` — reduce to one scalar [GPU-FOLD].
fn gpu_fold(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let r = reduction(cg, args, "gpuFold")?;
    let done = launch_fold(
        cg,
        &r.combine,
        &[r.tmpl.ty, slot(r.elem)],
        &r.src,
        &r.acc,
        &r.len,
    );
    let join = host_fallback(cg, &done);
    kernel_loop(cg, &r.src, &r.len, r.elem, |cg, arg, _| step(cg, &r, arg))?;
    rejoin(cg, &join);
    Ok(acc_result(cg, &r.acc, &r.tmpl))
}

/// `gpuIota(n)` — the index buffer `[0, n)` [GPU-IOTA]. Gather, stencil, and
/// matrix addressing all start here, since a kernel sees values, not indices.
fn gpu_iota(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let n = gen_expr(cg, nth(args, 0)?)?;
    let n = crate::conv::as_i64(cg, n)?;
    let out = buffer_alloc(cg, &n.operand, elem_owner(Some(LType::I64)));
    // The trip count is the ALLOCATED length, never the requested `n`: the
    // runtime clamps negative/oversized requests and a failed data allocation
    // to the empty buffer, and a loop bounded by `n` would spin up to 2^63
    // dropped stores against that empty buffer [GPU-IOTA].
    let len = buffer_len(cg, &out);
    let lp = open_range_loop(cg, "0", &len);
    buffer_set(cg, &out, &lp.i, &lp.i.clone());
    close_range_loop(cg, &lp);
    Ok(out)
}

/// `gpuGet(buffer, index)` — bounds-checked gather [GPU-GET], returning
/// `Result<T, Error>` at the buffer's element type so a float buffer yields a
/// float rather than its raw bits.
fn gpu_get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let buf = buffer_arg(cg, args, 0)?;
    let index = gen_expr(cg, nth(args, 1)?)?;
    let index = crate::conv::as_i64(cg, index)?;
    let inb = cg.call(
        "i32",
        "osprey_gpu_in_bounds",
        "i8*, i64",
        &[&buf.operand, &index.operand],
    );
    let raw = buffer_get(cg, &buf, &index.operand);
    let value = elem_value(cg, raw, tagged_elem(&buf));
    crate::result::make_result_unless_in_bounds(cg, &inb, value, "gpuGet: index out of bounds")
}

/// `gpuScan(buffer, initial, combine)` — inclusive prefix scan [GPU-SCAN]:
/// element `i` of the result is `combine` folded over the source through `i`.
/// The running accumulator is the same scalar slot `gpuFold` uses, so a device
/// backend can swap in a work-efficient parallel scan without changing meaning.
fn gpu_scan(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let _ = declined(cg, "gpuScan has no device form yet");
    let r = reduction(cg, args, "gpuScan")?;
    let out = buffer_alloc(cg, &r.len, elem_owner(Some(r.tmpl.ty)));
    kernel_loop(cg, &r.src, &r.len, r.elem, |cg, arg, i| {
        step(cg, &r, arg)?;
        // The slot already holds the boxed running value — exactly a buffer
        // word — so publishing it costs one load, not a re-box.
        let running = cg.emit_reg(format!("load i64, i64* {}", r.acc));
        buffer_set(cg, &out, i, &running);
        Ok(())
    })?;
    Ok(out)
}

/// `gpuFilter(buffer, predicate)` — stream compaction [GPU-FILTER]. The output
/// length is unknown until the predicate has run, so this fills a
/// source-length scratch buffer and publishes the exact prefix length.
fn gpu_filter(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let _ = declined(cg, "gpuFilter has no device form yet");
    let (src, elem, predicate) = unary_kernel(cg, args)?;
    let len = buffer_len(cg, &src);
    let out = buffer_alloc(cg, &len, elem_owner(elem));
    let kept = kept_counter(cg);
    kernel_loop(cg, &src, &len, elem, |cg, arg, _i| {
        // The loop already loaded this element: box the typed value back to its
        // storage word rather than reading the buffer a second time per element.
        let raw = box_to_i64(cg, arg.clone()).operand;
        let verdict = invoke(cg, &predicate, vec![arg])?;
        let verdict = crate::cast::coerce_to(cg, verdict, LType::I1)?;
        push_if_kept(cg, &verdict.operand, &out, &kept, &raw);
        Ok(())
    })?;
    take_kept(cg, &out, &kept);
    Ok(out)
}
