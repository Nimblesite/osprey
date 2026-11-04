//! The host-backend lowerings that make a trained model runnable: `gpuMatMul`
//! [GPU-MATMUL], `gpuLoad` [GPU-BUFFER-LOAD] and `gpuLoadTensor`
//! [GPU-BUFFER-TENSOR] (docs/specs/runtime/0034-GPUComputation.md). Each is one
//! call into `compiler/runtime/gpu_host.c` that returns a buffer or NULL,
//! wrapped as `Result<GpuBuffer<T>, Error>` whose `Error` carries the reason
//! the runtime recorded — a missing file, an unknown tensor, a dtype the
//! program did not ask for, a shape the operands do not have — through the
//! same channel `readFile` uses [BUILTIN-FILE-ERRMSG].
//!
//! The element type: `gpuLoad` is `GpuBuffer<float32>` byte for byte
//! [GPU-BUFFER-LOAD]; `gpuMatMul` takes its operands' element (`float` or
//! `float32`) from the left operand's tag; `gpuLoadTensor` asks for the type
//! the call's own inferred result names, published by the checker at the call
//! site [GPU-BUFFER-TENSOR] — never the file's dtype.

use super::{buffer_arg, elem_owner};
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::iter::nth;
use crate::llty::{LType, Value};
use osprey_ast::Expr;
use osprey_types::numeric::Numeric;

/// The `i`-th positional argument coerced to `ty` (`Str` for a path or name,
/// `I64` for a shape or offset).
fn arg_as(cg: &mut Codegen, args: &[Expr], i: usize, ty: LType) -> Result<String> {
    let v = gen_expr(cg, nth(args, i)?)?;
    Ok(crate::cast::coerce_to(cg, v, ty)?.operand)
}

/// Call a runtime producer of a buffer of `elem` elements and wrap its
/// nullable handle, tagged with that element's owner.
fn fallible_buffer(
    cg: &mut Codegen,
    callee: &str,
    params: &str,
    operands: &[&str],
    elem: LType,
    fallback: &str,
) -> Result<Value> {
    let owner = elem_owner(Some(elem));
    crate::extern_call::fallible_handle(cg, callee, params, operands, &owner, Some(fallback))
}

/// `gpuMatMul(left, right, rows, inner, cols)` [GPU-MATMUL]: one float type
/// for all three buffers, taken from the operands' tags.
pub(super) fn mat_mul(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let left = buffer_arg(cg, args, 0)?;
    let right = buffer_arg(cg, args, 1)?;
    let elem = matrix_elem(&left, &right)?;
    let rows = arg_as(cg, args, 2, LType::I64)?;
    let inner = arg_as(cg, args, 3, LType::I64)?;
    let cols = arg_as(cg, args, 4, LType::I64)?;
    let width = elem.bytes().to_string();
    fallible_buffer(
        cg,
        "osprey_gpu_matmul",
        "i8*, i8*, i64, i64, i64, i64",
        &[&left.operand, &right.operand, &rows, &inner, &cols, &width],
        LType::Num(elem),
        "gpuMatMul: operand lengths disagree with the stated shape",
    )
}

/// The float element a buffer holds, when it is `float` or `float32` — the two
/// types `gpuMatMul` is open over [GPU-MATMUL]. Inference has already refused
/// every other operand; this fails closed on a tag codegen cannot read rather
/// than mis-sizing the result buffer.
fn float_elem(buf: &Value) -> Result<Numeric> {
    super::tagged_elem(buf)
        .and_then(|ty| match ty {
            LType::Num(n) => Some(n),
            _ => None,
        })
        .filter(|n| n.is_float() && n.bytes() >= 4)
        .ok_or_else(|| CodegenError::unsupported("a gpuMatMul operand must hold float or float32"))
}

fn matrix_elem(left: &Value, right: &Value) -> Result<Numeric> {
    let elem = float_elem(left)?;
    if float_elem(right)? != elem {
        return Err(CodegenError::unsupported(
            "gpuMatMul operands must have the same float element type",
        ));
    }
    Ok(elem)
}

/// `gpuLoad(path, offset, count)` [GPU-BUFFER-LOAD]: little-endian binary32
/// words byte for byte, no widening, as `GpuBuffer<float32>`.
pub(super) fn load(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let path = arg_as(cg, args, 0, LType::Str)?;
    let offset = arg_as(cg, args, 1, LType::I64)?;
    let count = arg_as(cg, args, 2, LType::I64)?;
    fallible_buffer(
        cg,
        "osprey_gpu_load_f32",
        "i8*, i64, i64",
        &[&path, &offset, &count],
        LType::Num(Numeric::FLOAT32),
        "gpuLoad: could not read the values",
    )
}

/// `gpuLoadTensor(path, name)` [GPU-BUFFER-TENSOR]: the element type the
/// program asked for, from the checker's record for this call site — F64 into
/// `float`, F32 and BF16 into `float32`, F16 into `float16`, anything else an
/// `Error` the runtime words.
pub(super) fn load_tensor(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let path = arg_as(cg, args, 0, LType::Str)?;
    let name = arg_as(cg, args, 1, LType::Str)?;
    let asked = tensor_elem(cg)?;
    let bytes = i64::from(asked.bytes()).to_string();
    fallible_buffer(
        cg,
        "osprey_gpu_load_tensor",
        "i8*, i8*, i64",
        &[&path, &name, &bytes],
        LType::Num(asked),
        "gpuLoadTensor: could not read the tensor",
    )
}

/// `gpuMapFile(path, offset, count)` [LLM-WEIGHTS-MAPPED]: a read-only mapped
/// region at the numeric type this call's result pins.
pub(super) fn map_file(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let path = arg_as(cg, args, 0, LType::Str)?;
    let offset = arg_as(cg, args, 1, LType::I64)?;
    let count = arg_as(cg, args, 2, LType::I64)?;
    let asked = result_elem(cg).ok_or_else(|| {
        CodegenError::unsupported("gpuMapFile needs a concrete numeric result type")
    })?;
    let bytes = i64::from(asked.bytes()).to_string();
    fallible_buffer(
        cg,
        "osprey_gpu_map_file",
        "i8*, i64, i64, i64",
        &[&path, &offset, &count, &bytes],
        LType::Num(asked),
        "gpuMapFile: could not map the region",
    )
}

/// The numeric element of this call's `Result<GpuBuffer<T>, Error>`.
fn result_elem(cg: &Codegen) -> Option<Numeric> {
    cg.expression_type
        .as_ref()
        .and_then(|ty| type_argument(ty, osprey_types::names::RESULT))
        .and_then(|ty| type_argument(ty, osprey_types::names::GPU_BUFFER))
        .and_then(Numeric::of_type)
}

/// [GPU-BUFFER-TENSOR]: use the existing unique inferred application, never
/// an enclosing expression's position or the file's dtype.
fn tensor_elem(cg: &Codegen) -> Result<Numeric> {
    result_elem(cg)
        .filter(|n| n.is_float() && n.bytes() >= 2)
        .ok_or_else(|| {
            CodegenError::unsupported(
                "gpuLoadTensor needs a concrete float, float32 or float16 result type",
            )
        })
}

fn type_argument<'a>(ty: &'a osprey_types::Type, expected: &str) -> Option<&'a osprey_types::Type> {
    match ty {
        osprey_types::Type::Con { name, args } if name == expected => args.first(),
        _ => None,
    }
}
