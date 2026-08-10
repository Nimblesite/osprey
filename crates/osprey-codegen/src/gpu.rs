//! GPU computation builtins — the host execution backend [GPU-BACKEND-HOST]
//! (docs/specs/0034-GPUComputation.md). `toGpu`/`fromGpu` copy between
//! persistent lists and dense scalar buffers, `gpuMap`/`gpuFold` run kernels
//! as fused native loops over those buffers. The dense unboxed layout is the
//! staging layout a device transfer will use; kernels reaching codegen are
//! already proven pure by the effect checker [GPU-KERNEL-PURE].

use crate::builder::{Codegen, FnSig};
use crate::conv::box_to_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::iter::{acc_init, acc_result, acc_step, callback_of, invoke, nth, Callback};
use crate::llty::{LType, Value};
use crate::loops::{close_range_loop, open_range_loop};
use osprey_ast::{Expr, NamedArgument};

/// The owner tag GPU buffer handles travel under when their element type is
/// not statically known.
const GPU_OWNER: &str = "GpuBuffer";

/// The name `gpuDevice()` reports for the host execution backend
/// [GPU-DEVICE] [GPU-BACKEND-HOST]. Device backends will report their own
/// names (`cuda:0`, `metal:0`) when they land.
const HOST_DEVICE: &str = "host";

/// Owner-tag prefix marking a buffer whose element `LType` is known; the suffix
/// is the element's LLVM spelling (`Gpu#double`), mirroring the flat
/// list-literal convention (`[]double`) so the tag survives lets, pipes,
/// parameters and returns.
pub(crate) const GPU_TAG: &str = "Gpu#";

/// The owner tag for a `GpuBuffer<elem>` static type: element-typed when the
/// element is a concrete scalar, the bare owner otherwise.
pub(crate) fn buffer_owner(elem: Option<&osprey_types::Type>) -> String {
    elem_owner(
        elem.filter(|ty| !osprey_types::has_type_var(ty))
            .map(crate::types::ltype_of),
    )
}

/// The scalar element `LType` spelled after `prefix` in a value's owner tag —
/// `Gpu#double` on buffers, `[]double` on flat list literals.
fn elem_of_tag(v: &Value, prefix: &str) -> Option<LType> {
    match v.osp_ty.as_deref()?.strip_prefix(prefix)? {
        "double" => Some(LType::Double),
        "i1" => Some(LType::I1),
        "i64" => Some(LType::I64),
        _ => None,
    }
}

/// The element `LType` recorded on a buffer value's owner tag, if any.
fn tagged_elem(v: &Value) -> Option<LType> {
    elem_of_tag(v, GPU_TAG)
}

/// The element `LType` tagged on a flat list-literal value (`[]double`), if any.
fn list_elem(v: &Value) -> Option<LType> {
    elem_of_tag(v, "[]")
}

/// The owner tag for a buffer holding `elem` words.
fn elem_owner(elem: Option<LType>) -> String {
    match elem {
        Some(lt) => format!("{GPU_TAG}{}", lt.as_str()),
        None => GPU_OWNER.to_string(),
    }
}

/// Dispatch a GPU builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Option<Value>> {
    let v = match name {
        "toGpu" => to_gpu(cg, args)?,
        "fromGpu" => from_gpu(cg, args)?,
        "gpuLength" => gpu_length(cg, args)?,
        "gpuMap" => gpu_map(cg, args)?,
        "gpuFold" => gpu_fold(cg, args)?,
        "gpuZipWith" => gpu_zip_with(cg, args)?,
        "gpuIota" => gpu_iota(cg, args)?,
        "gpuGet" => gpu_get(cg, args)?,
        "gpuScan" => gpu_scan(cg, args)?,
        "gpuFilter" => gpu_filter(cg, args)?,
        "gpuDevice" => cg.string_constant(HOST_DEVICE),
        _ => return Ok(None),
    };
    Ok(Some(v))
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

/// The `LType` of the kernel's element parameter at `slot`, when a concrete
/// signature is known — from the kernel expression's inferred function type
/// (named functions, fields, call chains), falling back to the callback's own
/// lowered signature (inline lambdas, closure locals). A generic kernel with
/// no concrete signature receives the raw buffer word, matching the eager
/// list combinators' behavior.
fn kernel_elem_ltype(
    cg: &Codegen,
    kernel_expr: &Expr,
    kernel: &Callback,
    slot: usize,
) -> Option<LType> {
    let sig: Option<FnSig> = cg
        .callee_fn_type(kernel_expr)
        .as_ref()
        .and_then(Codegen::fn_value_sig)
        .or_else(|| match kernel {
            Callback::Lambda(_, _, sig) => sig.clone(),
            Callback::Local(_, sig) | Callback::Value(_, sig) => Some(sig.clone()),
            Callback::Named(_) => None,
        });
    sig.and_then(|(params, _, _, _)| params.get(slot).map(|param| param.ty))
}

/// A buffer word recovered at the kernel's element type: a `float` element's
/// raw bits become a `double` operand (never an integer conversion), a `bool`
/// becomes `i1`, and an unknown element passes through as the raw word.
fn elem_value(cg: &mut Codegen, raw: String, elem: Option<LType>) -> Value {
    match elem {
        Some(lt) if lt != LType::I64 => crate::effects::unbox_coro_value(cg, &raw, lt, None),
        _ => Value::new(raw, LType::I64),
    }
}

/// The scalar-element backstop [GPU-BUFFER-ELEM]: a buffer word is an `int`,
/// `float`, or `bool`, boxed to its raw `i64` word.
fn scalar_word(cg: &mut Codegen, v: Value, what: &str) -> Result<Value> {
    if v.result_inner.is_some() {
        return Err(CodegenError::unsupported(format!(
            "{what} cannot be an unhandled Result; handle it inside the kernel"
        )));
    }
    if !matches!(v.ty, LType::I64 | LType::Double | LType::I1) {
        return Err(CodegenError::unsupported(format!(
            "{what} must be a scalar (int, float, or bool)"
        )));
    }
    Ok(box_to_i64(cg, v))
}

/// `toGpu([a, b, c])` — a buffer literal [GPU-BUFFER-LITERAL]. The elements are
/// stored straight into the dense buffer at their constant indices, so neither
/// the flat literal block nor an `OspreyList` is ever built: one allocation
/// instead of three, and no copy loop. The element tag comes from the first
/// element's lowered type, exactly as a flat list literal's would.
fn buffer_literal(cg: &mut Codegen, elements: &[Expr]) -> Result<Value> {
    let out = buffer_alloc(cg, &elements.len().to_string(), GPU_OWNER.to_string());
    let mut elem = None;
    for (i, e) in elements.iter().enumerate() {
        let v = gen_expr(cg, e)?;
        elem = elem.or(Some(v.ty));
        let word = scalar_word(cg, v, "a GPU buffer literal element")?;
        buffer_set(cg, &out, &i.to_string(), &word.operand);
    }
    Ok(out.with_owner(Some(elem_owner(elem))))
}

/// The compaction counter shared by `gpuFilter` and iterator fusion: a stack
/// slot holding how many elements have been kept so far. Both write into a
/// buffer sized at the *source* length, because the kept count is not known
/// until the loop has run.
fn kept_counter(cg: &mut Codegen) -> String {
    let kept = cg.fresh_reg();
    cg.emit(format!("{kept} = alloca i64"));
    cg.emit(format!("store i64 0, i64* {kept}"));
    kept
}

/// Append `word` at the counter's position and advance it.
fn push_kept(cg: &mut Codegen, out: &Value, kept: &str, word: &str) {
    let at = cg.emit_reg(format!("load i64, i64* {kept}"));
    buffer_set(cg, out, &at, word);
    let next = cg.emit_reg(format!("add i64 {at}, 1"));
    cg.emit(format!("store i64 {next}, i64* {kept}"));
}

/// Publish the compacted prefix: the buffer's length becomes the kept count.
fn take_kept(cg: &mut Codegen, out: &Value, kept: &str) {
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
    let (start, end) = crate::iter::bounds(cg, range);
    let span = cg.emit_reg(format!("sub i64 {end}, {start}"));
    // An inverted range yields no elements, so its buffer is empty, not a
    // negative-length allocation.
    let empty = cg.emit_reg(format!("icmp slt i64 {span}, 0"));
    let span = cg.emit_reg(format!("select i1 {empty}, i64 0, i64 {span}"));
    let out = buffer_alloc(cg, &span, GPU_OWNER.to_string());
    let kept = kept_counter(cg);
    let lp = open_range_loop(cg, &start, &end);
    crate::arc::push_frame(cg);
    let v = crate::iter::replay(cg, Value::new(lp.i.clone(), LType::I64), &lp.incr)?;
    let elem = v.ty;
    let word = scalar_word(cg, v, "a toGpu element")?;
    push_kept(cg, &out, &kept, &word.operand);
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);
    take_kept(cg, &out, &kept);
    Ok(out.with_owner(Some(elem_owner(Some(elem)))))
}

/// `toGpu(list)` — dense copy of a scalar list [GPU-BUFFER-FROM-LIST]. A flat
/// list literal's element tag (`[]double`) becomes the buffer's element tag.
/// A literal argument takes [`buffer_literal`] and an iterator pipeline
/// [`fuse_iterator`], so neither builds a list on the way to the buffer.
fn to_gpu(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let arg = nth(args, 0)?;
    if let Expr::List(elements) = arg {
        return buffer_literal(cg, elements);
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
fn from_gpu(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let b = buffer_arg(cg, args, 0)?;
    let bld = crate::collections::list_builder_new_of(cg, "0");
    let len = buffer_len(cg, &b);
    let lp = open_range_loop(cg, "0", &len);
    let elem = buffer_get(cg, &b, &lp.i);
    crate::collections::list_builder_push(cg, &bld, &elem, "0");
    close_range_loop(cg, &lp);
    Ok(crate::collections::list_builder_seal(cg, &bld))
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
    crate::arc::push_frame(cg);
    let raw = buffer_get(cg, src, &lp.i);
    let arg = elem_value(cg, raw, elem);
    body(cg, arg, &lp.i)?;
    crate::arc::pop_frame(cg);
    close_range_loop(cg, &lp);
    Ok(())
}

/// The shared preamble of every combinator that runs a kernel over `src`: the
/// `arg_i`-th argument as a callback, plus the element `LType` recovered from
/// the buffer's owner tag or the kernel's parameter at `slot`.
fn kernel_of(
    cg: &mut Codegen,
    args: &[Expr],
    arg_i: usize,
    src: &Value,
    slot: usize,
) -> Result<(Callback, Option<LType>)> {
    let expr = nth(args, arg_i)?;
    let kernel = callback_of(cg, expr)?;
    let elem = tagged_elem(src).or_else(|| kernel_elem_ltype(cg, expr, &kernel, slot));
    Ok((kernel, elem))
}

/// The fold/scan accumulator slot, restricted to scalar words so every
/// accepted reduction stays offloadable [GPU-FOLD] [GPU-SCAN].
fn scalar_acc_init(cg: &mut Codegen, args: &[Expr], what: &str) -> Result<(String, Value)> {
    let (acc, tmpl) = acc_init(cg, args)?;
    if tmpl.result_inner.is_some() || !matches!(tmpl.ty, LType::I64 | LType::Double | LType::I1) {
        return Err(CodegenError::unsupported(format!(
            "a {what} accumulator must be a scalar (int, float, or bool)"
        )));
    }
    Ok((acc, tmpl))
}

/// `gpuMap(buffer, kernel)` — apply a pure kernel to every element [GPU-MAP].
fn gpu_map(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let src = buffer_arg(cg, args, 0)?;
    let (kernel, elem) = kernel_of(cg, args, 1, &src, 0)?;
    let len = buffer_len(cg, &src);
    let out = buffer_alloc(cg, &len, GPU_OWNER.to_string());
    let mut mapped_ty = LType::I64;
    kernel_loop(cg, &src, &len, elem, |cg, arg, i| {
        let mapped = invoke(cg, &kernel, vec![arg])?;
        mapped_ty = mapped.ty;
        let stored = scalar_word(cg, mapped, "a gpuMap kernel result")?;
        buffer_set(cg, &out, i, &stored.operand);
        Ok(())
    })?;
    // The kernel's result LType is codegen-time knowledge: retag the handle so
    // downstream combinators recover the element without a kernel signature.
    Ok(out.with_owner(Some(elem_owner(Some(mapped_ty)))))
}

/// `gpuFold(buffer, initial, combine)` — reduce to one scalar [GPU-FOLD]. The
/// scalar-accumulator restriction keeps every accepted program offloadable
/// when device reductions land.
fn gpu_fold(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let src = buffer_arg(cg, args, 0)?;
    let (acc, tmpl) = scalar_acc_init(cg, args, "gpuFold")?;
    let (combine, elem) = kernel_of(cg, args, 2, &src, 1)?;
    let len = buffer_len(cg, &src);
    kernel_loop(cg, &src, &len, elem, |cg, arg, _| {
        acc_step(cg, &acc, &tmpl, &combine, arg)
    })?;
    Ok(acc_result(cg, &acc, &tmpl))
}

/// `gpuZipWith(a, b, kernel)` — elementwise binary combination [GPU-ZIPWITH].
/// The result length is the shorter operand's, so a ragged pair truncates
/// rather than reading past the end.
fn gpu_zip_with(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let left = buffer_arg(cg, args, 0)?;
    let right = buffer_arg(cg, args, 1)?;
    let (kernel, left_elem) = kernel_of(cg, args, 2, &left, 0)?;
    let kernel_expr = nth(args, 2)?;
    let right_elem = tagged_elem(&right).or_else(|| kernel_elem_ltype(cg, kernel_expr, &kernel, 1));
    let left_len = buffer_len(cg, &left);
    let right_len = buffer_len(cg, &right);
    let shorter = cg.emit_reg(format!("icmp slt i64 {left_len}, {right_len}"));
    let len = cg.emit_reg(format!(
        "select i1 {shorter}, i64 {left_len}, i64 {right_len}"
    ));
    let out = buffer_alloc(cg, &len, GPU_OWNER.to_string());
    let mut zipped_ty = LType::I64;
    kernel_loop(cg, &left, &len, left_elem, |cg, a, i| {
        let raw = buffer_get(cg, &right, i);
        let b = elem_value(cg, raw, right_elem);
        let zipped = invoke(cg, &kernel, vec![a, b])?;
        zipped_ty = zipped.ty;
        let stored = scalar_word(cg, zipped, "a gpuZipWith kernel result")?;
        buffer_set(cg, &out, i, &stored.operand);
        Ok(())
    })?;
    Ok(out.with_owner(Some(elem_owner(Some(zipped_ty)))))
}

/// `gpuIota(n)` — the index buffer `[0, n)` [GPU-IOTA]. Gather, stencil, and
/// matrix addressing all start here, since a kernel sees values, not indices.
fn gpu_iota(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let n = gen_expr(cg, nth(args, 0)?)?;
    let n = crate::conv::as_i64(cg, n)?;
    let out = buffer_alloc(cg, &n.operand, elem_owner(Some(LType::I64)));
    let lp = open_range_loop(cg, "0", &n.operand);
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
    let elem = tagged_elem(&buf);
    let value = elem_value(cg, raw, elem);
    let inner = value.ty;
    let is_err = cg.emit_reg(format!("icmp eq i32 {inb}, 0"));
    crate::result::make_result_if_err(
        cg,
        value,
        inner,
        &is_err,
        Some("gpuGet: index out of bounds"),
    )
}

/// `gpuScan(buffer, initial, combine)` — inclusive prefix scan [GPU-SCAN]:
/// element `i` of the result is `combine` folded over the source through `i`.
/// The running accumulator is the same scalar slot `gpuFold` uses, so a device
/// backend can swap in a work-efficient parallel scan without changing meaning.
fn gpu_scan(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let src = buffer_arg(cg, args, 0)?;
    let (acc, tmpl) = scalar_acc_init(cg, args, "gpuScan")?;
    let (combine, elem) = kernel_of(cg, args, 2, &src, 1)?;
    let len = buffer_len(cg, &src);
    let out = buffer_alloc(cg, &len, elem_owner(Some(tmpl.ty)));
    kernel_loop(cg, &src, &len, elem, |cg, arg, i| {
        acc_step(cg, &acc, &tmpl, &combine, arg)?;
        // The slot already holds the boxed running value — exactly a buffer
        // word — so publishing it costs one load, not a re-box.
        let running = cg.emit_reg(format!("load i64, i64* {acc}"));
        buffer_set(cg, &out, i, &running);
        Ok(())
    })?;
    Ok(out)
}

/// `gpuFilter(buffer, predicate)` — stream compaction [GPU-FILTER]. The output
/// length is unknown until the predicate has run, so this fills a
/// source-length scratch buffer and publishes the exact prefix length.
fn gpu_filter(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let src = buffer_arg(cg, args, 0)?;
    let (predicate, elem) = kernel_of(cg, args, 1, &src, 0)?;
    let len = buffer_len(cg, &src);
    let out = buffer_alloc(cg, &len, elem_owner(elem));
    let kept = kept_counter(cg);
    kernel_loop(cg, &src, &len, elem, |cg, arg, i| {
        let raw = buffer_get(cg, &src, i);
        let verdict = invoke(cg, &predicate, vec![arg])?;
        let verdict = crate::cast::coerce_to(cg, verdict, LType::I1)?;
        let keep = cg.fresh_label();
        let skip = cg.fresh_label();
        cg.emit(format!(
            "br i1 {}, label %{keep}, label %{skip}",
            verdict.operand
        ));
        cg.start_block(&keep);
        push_kept(cg, &out, &kept, &raw);
        cg.emit(format!("br label %{skip}"));
        cg.start_block(&skip);
        Ok(())
    })?;
    take_kept(cg, &out, &kept);
    Ok(out)
}
