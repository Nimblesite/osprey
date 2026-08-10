//! Kernel extraction for the GPU combinators — implements [GPU-KERNEL-EXTRACT]
//! (docs/specs/0034-GPUComputation.md).
//!
//! A combinator's kernel is compiled ONCE, as a module-scope function with a
//! flat scalar signature, and the host loop calls that symbol per element. A
//! lambda kernel's captured free variables become LEADING parameters
//! (uniforms), so the emitted function carries no environment pointer and no
//! closure cell — the first-order shape a PTX/AIR/SPIR-V emitter consumes, so
//! [GPU-BACKEND-DEVICE] becomes a target driver rather than a rewrite.
//!
//! Extraction is a lowering choice, never a semantic one: the pre-extraction
//! inlined lowering is retained behind [`GPU_KERNELS_ENV`], and
//! `crates/run_test_corpus.sh` compiles `tests/core/gpu` both ways and requires
//! byte-identical output.

use crate::builder::{Codegen, FnSig, ParamSig};
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::iter::{callback_of, nth, Callback};
use crate::llty::{LType, Value};
use osprey_ast::{Expr, Parameter};
use std::collections::BTreeSet;

/// The environment switch selecting the GPU kernel lowering
/// [GPU-KERNEL-EXTRACT]. `extract` (the default) lifts each kernel to its own
/// function; `inline` keeps the pre-stage-3 host-loop lowering, which the
/// corpus harness uses as a differential oracle.
pub const GPU_KERNELS_ENV: &str = "OSPREY_GPU_KERNELS";

/// The value naming the extracted lowering.
const MODE_EXTRACT: &str = "extract";
/// The value naming the retained inlined lowering.
const MODE_INLINE: &str = "inline";

/// Symbol prefix of every lifted kernel. The suffix is a module-monotonic id
/// advanced only by extraction, so a kernel's name is a pure function of AST
/// walk order — identical for a Default/ML twin pair [FLAVOR-IR-EQUIV].
const KERNEL_PREFIX: &str = "__gpu_kernel_";

/// Which lowering the GPU combinators use for their kernels
/// [GPU-KERNEL-EXTRACT].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuKernelMode {
    /// Lift each admissible kernel to a module-scope function.
    #[default]
    Extract,
    /// Beta-reduce each lambda kernel into the host loop, as before stage 3.
    Inline,
}

/// The lowering `value` names; an unset (or empty) value keeps extraction, and
/// anything unrecognised is an error rather than a silent fallback.
pub(crate) fn mode_of(value: Option<&str>) -> Result<GpuKernelMode> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None | Some(MODE_EXTRACT) => Ok(GpuKernelMode::Extract),
        Some(MODE_INLINE) => Ok(GpuKernelMode::Inline),
        Some(other) => Err(CodegenError::invalid(format!(
            "{GPU_KERNELS_ENV}={other}: expected `{MODE_EXTRACT}` or `{MODE_INLINE}`"
        ))),
    }
}

/// [`mode_of`] read from the process environment.
pub(crate) fn mode_from_env() -> Result<GpuKernelMode> {
    match std::env::var(GPU_KERNELS_ENV) {
        Ok(value) => mode_of(Some(&value)),
        Err(std::env::VarError::NotPresent) => Ok(GpuKernelMode::Extract),
        Err(std::env::VarError::NotUnicode(_)) => Err(CodegenError::invalid(format!(
            "{GPU_KERNELS_ENV} is not valid UTF-8"
        ))),
    }
}

/// A kernel lifted to a module-scope function with the flat scalar ABI. The
/// `uniforms` are the loop-invariant free variables the host evaluated once
/// before the loop and re-passes at every element.
#[derive(Clone, Debug)]
pub(crate) struct Extracted {
    symbol: String,
    uniforms: Vec<Value>,
    /// The element/accumulator slots only; `uniforms` are prepended at the call.
    sig: FnSig,
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
            Callback::Named(_) | Callback::Extracted(_) => None,
        });
    sig.and_then(|(params, _, _, _)| params.get(slot).map(|param| param.ty))
}

/// The element `LType` a kernel's parameter at `slot` receives from `src`: the
/// buffer's own owner tag when it carries one, else the kernel signature's.
pub(crate) fn kernel_elem(
    cg: &Codegen,
    kernel_expr: &Expr,
    kernel: &Callback,
    src: &Value,
    slot: usize,
) -> Option<LType> {
    crate::gpu::tagged_elem(src).or_else(|| kernel_elem_ltype(cg, kernel_expr, kernel, slot))
}

/// The shared preamble of every combinator that runs a kernel over `src`: the
/// `arg_i`-th argument as a callback, plus the element `LType` recovered from
/// the buffer's owner tag or the kernel's parameter at `slot`.
pub(crate) fn kernel_of(
    cg: &mut Codegen,
    args: &[Expr],
    arg_i: usize,
    src: &Value,
    slot: usize,
) -> Result<(Callback, Option<LType>)> {
    let expr = nth(args, arg_i)?;
    let kernel = callback_of(cg, expr)?;
    let elem = kernel_elem(cg, expr, &kernel, src, slot);
    Ok((kernel, elem))
}

/// The kernel parameter `LType` for a buffer element whose type the owner tag
/// did not pin: the raw word, matching [`crate::gpu`]'s element fallback.
pub(crate) fn slot(elem: Option<LType>) -> LType {
    elem.unwrap_or(LType::I64)
}

/// Lift `cb` to a module-scope function when the mode and the kernel's shape
/// allow it, else hand it back unchanged for the inlined lowering. `slots` are
/// the combinator-derived `LType`s of the kernel's own parameters, in call
/// order (accumulator first for `gpuFold`/`gpuScan`).
///
/// A named kernel is left alone: it already has an emitted symbol with a
/// concrete signature and the host loop already calls it
/// ([`crate::expr::call_with_values`]), so re-lifting would emit a second copy
/// of a body that exists — and a BUILTIN name (`gpuMap(toFloat)`) has no symbol
/// at all, only a per-element value form. A closure cell (`Local`/`Value`) is
/// precisely the captured environment this ABI forbids, and its call already
/// goes through the cell rather than the loop.
pub(crate) fn extract(cg: &mut Codegen, cb: Callback, slots: &[LType]) -> Result<Callback> {
    if cg.gpu_kernels() == GpuKernelMode::Inline {
        return Ok(cb);
    }
    match cb {
        Callback::Named(_) | Callback::Local(..) | Callback::Value(..) | Callback::Extracted(_) => {
            Ok(cb)
        }
        Callback::Lambda(parameters, body, own) => lift(cg, parameters, body, own, slots),
    }
}

/// Lift a lambda kernel, or hand the callback back untouched when its shape
/// declines extraction. Declining is always safe: it is the pre-extraction
/// lowering, which produces the same values.
fn lift(
    cg: &mut Codegen,
    parameters: Vec<Parameter>,
    body: Expr,
    own: Option<FnSig>,
    slots: &[LType],
) -> Result<Callback> {
    let caps = crate::closure::capture_list(cg, &parameters, &body);
    if !admissible(cg, &parameters, &body, own.as_ref(), &caps, slots) {
        return Ok(Callback::Lambda(parameters, body, own));
    }
    let params = param_sigs(&parameters, own.as_ref(), slots);
    let kernel = Lifting {
        caps: &caps,
        parameters: &parameters,
        body: &body,
        own: own.as_ref(),
    };
    emit(cg, &kernel, params)
}

/// The pieces of one lambda kernel being lifted, travelling together so the
/// emitter's signature names a kernel rather than six loose fragments.
struct Lifting<'a> {
    caps: &'a [crate::closure::Capture],
    parameters: &'a [Parameter],
    body: &'a Expr,
    own: Option<&'a FnSig>,
}

/// Whether this lambda's shape fits the extracted ABI.
fn admissible(
    cg: &Codegen,
    parameters: &[Parameter],
    body: &Expr,
    own: Option<&FnSig>,
    caps: &[crate::closure::Capture],
    slots: &[LType],
) -> bool {
    parameters.len() == slots.len()
        && !host_bound(cg, &crate::closure::free_names(parameters, body))
        && caps.iter().all(|c| uniform_admissible(&c.val))
        && own.is_none_or(flat_slots)
}

/// Whether every declared parameter slot is a plain value. A `Result` slot
/// travels as an erased `i8*` and a Fiber slot carries an element shape; both
/// are host-side structure the flat device ABI has no representation for, so a
/// kernel declaring one keeps the inlined lowering.
fn flat_slots(sig: &FnSig) -> bool {
    sig.0
        .iter()
        .all(|p| p.result_inner.is_none() && p.fiber.is_none())
}

/// Free names the lifted function cannot see. [`Codegen::enter_nested_fn`]
/// clears the scope stack, the cell slots and the ARC ledger, but NOT
/// `lambdas` / `fn_ptr_locals` / `call_aliases`: a body reaching one of those
/// would beta-reduce or indirect-call against enclosing state the kernel no
/// longer has, and fall through to `call @f` on a symbol that is never defined
/// — a link failure with no source location. Extraction declines instead.
fn host_bound(cg: &Codegen, names: &BTreeSet<String>) -> bool {
    names.iter().any(|n| {
        cg.cell_slots.contains_key(n)
            || cg.lambdas.contains_key(n)
            || cg.fn_ptr_locals.contains_key(n)
            || cg.call_aliases.contains_key(n)
    })
}

/// Whether a capture can travel as an extracted-kernel parameter: a scalar
/// word, or a GPU buffer handle. A string, a record/union handle, a `Result`
/// block or a Fiber carries host-side structure the flat device ABI has no
/// slot for — the same restriction [GPU-BUFFER-ELEM] places on element data.
fn uniform_admissible(v: &Value) -> bool {
    v.result_inner.is_none()
        && v.fiber_elem.is_none()
        && match v.ty {
            LType::I64 | LType::Double | LType::I1 => true,
            LType::Ptr => v.osp_ty.as_deref().is_some_and(crate::gpu::is_buffer_owner),
            LType::Str | LType::I32 => false,
        }
}

/// The kernel's own parameter slots: its inferred signature where it has one,
/// else the combinator-derived element/accumulator types.
fn param_sigs(parameters: &[Parameter], own: Option<&FnSig>, slots: &[LType]) -> Vec<ParamSig> {
    (0..parameters.len())
        .map(|i| match own.and_then(|s| s.0.get(i)).copied() {
            Some(declared) => declared,
            None => ParamSig {
                ty: slots.get(i).copied().unwrap_or(LType::I64),
                result_inner: None,
                fiber: None,
            },
        })
        .collect()
}

/// Emit the lifted `define` and hand back the callback that calls it.
fn emit(cg: &mut Codegen, k: &Lifting<'_>, params: Vec<ParamSig>) -> Result<Callback> {
    let symbol = format!("{KERNEL_PREFIX}{}", cg.next_kernel_id());
    let uniforms: Vec<Value> = k.caps.iter().map(|c| c.val.clone()).collect();
    let saved = cg.enter_nested_fn();
    let plist = declare(cg, k, &params);
    let emitted = kernel_body(cg, k.body, k.own);
    let ret = emitted
        .as_ref()
        .map_or_else(|_| LType::I64.to_string(), Value::llvm_ty);
    // Restore the enclosing function BEFORE propagating, so a failed body never
    // leaves the host's emission state clobbered ([`crate::closure`] does the
    // same).
    cg.exit_nested_fn(saved, &ret, &symbol, &plist);
    let value = emitted?;
    let sig = (
        params,
        value.ty,
        value.result_inner,
        k.own.and_then(|s| s.3),
    );
    Ok(Callback::Extracted(Extracted {
        symbol,
        uniforms,
        sig,
    }))
}

/// The lifted function's parameter list — uniforms first, then the element
/// slots — with every name bound in the nested function's fresh scope.
fn declare(cg: &mut Codegen, k: &Lifting<'_>, params: &[ParamSig]) -> Vec<(LType, String)> {
    let mut plist = bind_uniforms(cg, k.caps);
    plist.extend(crate::closure::bind_params_from(
        cg,
        k.parameters,
        params,
        k.caps.len(),
    ));
    plist
}

/// Bind each uniform to its leading parameter register and collect the
/// `define`'s leading parameter list.
fn bind_uniforms(cg: &mut Codegen, caps: &[crate::closure::Capture]) -> Vec<(LType, String)> {
    let mut out = Vec::with_capacity(caps.len());
    for (i, c) in caps.iter().enumerate() {
        let reg = crate::llty::param_register(i);
        let sig = ParamSig {
            ty: c.val.ty,
            result_inner: None,
            fiber: None,
        };
        // `incoming_param` registers no ownership: a uniform buffer handle is
        // BORROWED for the call's duration, exactly as a top-level function's
        // parameters are [GC-ARC-PERCEUS].
        let value = crate::cast::incoming_param(cg, format!("%{reg}"), sig, c.val.osp_ty.clone());
        cg.bind(c.name.clone(), value);
        out.push((c.val.ty, reg));
    }
    out
}

/// Lower the kernel body and emit its `ret`. The body is always in VALUE
/// position — it is what the function returns — whatever the enclosing
/// statement was doing with the loop.
fn kernel_body(cg: &mut Codegen, body: &Expr, own: Option<&FnSig>) -> Result<Value> {
    let outer = std::mem::replace(&mut cg.value_discarded, false);
    let lowered = gen_expr(cg, body).and_then(|v| crate::expr::fit_lambda_return(cg, v, own));
    cg.value_discarded = outer;
    let value = lowered?;
    // Function epilogue: the return transfers +1, owned locals drop
    // [GC-ARC-PERCEUS]. A scalar return makes the retain a no-op.
    crate::arc::epilogue(cg, Some(&value));
    cg.emit(format!("ret {} {}", value.llvm_ty(), value.operand));
    Ok(value)
}

/// Call an extracted kernel: the uniforms first, then this element's slots.
pub(crate) fn extracted_call(
    cg: &mut Codegen,
    kernel: &Extracted,
    args: Vec<Value>,
) -> Result<Value> {
    let typed = crate::closure::coerce_typed_args(cg, &kernel.sig, args)?;
    let ret = crate::llty::ret_spelling(kernel.sig.1, kernel.sig.2);
    let operands = kernel
        .uniforms
        .iter()
        .map(Value::typed)
        .chain(typed)
        .collect::<Vec<_>>()
        .join(", ");
    let reg = cg.emit_reg(format!("call {ret} @{}({operands})", kernel.symbol));
    let value = crate::closure::returned(reg, &kernel.sig);
    // Callee epilogues transfer +1 on every return [GC-ARC-PERCEUS].
    crate::arc::own(cg, &value);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{mode_of, GpuKernelMode};

    /// The parsed mode, or the rendered error — assertable without requiring
    /// `PartialEq` on the codegen error type.
    fn parsed(value: Option<&str>) -> std::result::Result<GpuKernelMode, String> {
        mode_of(value).map_err(|e| e.to_string())
    }

    #[test]
    fn unset_and_named_modes_parse() {
        assert_eq!(parsed(None), Ok(GpuKernelMode::Extract));
        assert_eq!(parsed(Some("")), Ok(GpuKernelMode::Extract));
        assert_eq!(parsed(Some("extract")), Ok(GpuKernelMode::Extract));
        assert_eq!(parsed(Some(" inline ")), Ok(GpuKernelMode::Inline));
    }

    #[test]
    fn an_unrecognised_mode_is_an_error_not_a_fallback() {
        let message = parsed(Some("nope")).err().unwrap_or_default();
        assert!(
            message.contains("OSPREY_GPU_KERNELS=nope"),
            "unexpected message: {message}"
        );
    }
}
