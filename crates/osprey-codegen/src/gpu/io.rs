//! The host-backend lowerings that make a trained model runnable: `gpuMatMul`
//! [GPU-MATMUL], `gpuLoad` [GPU-BUFFER-LOAD] and `gpuLoadTensor`
//! [GPU-BUFFER-TENSOR] (docs/specs/runtime/0034-GPUComputation.md). Each is one call
//! into `compiler/runtime/gpu_host.c` that returns a float buffer or NULL,
//! wrapped as `Result<GpuBuffer<float>, Error>` whose `Error` carries the
//! reason the runtime recorded — a missing file, an unknown tensor, a shape the
//! operands do not have — through the same channel `readFile` uses
//! [BUILTIN-FILE-ERRMSG].

use super::{buffer_arg, elem_owner};
use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::iter::nth;
use crate::llty::{LType, Value};
use osprey_ast::Expr;

/// The `i`-th positional argument coerced to `ty` (`Str` for a path or name,
/// `I64` for a shape or offset).
fn arg_as(cg: &mut Codegen, args: &[Expr], i: usize, ty: LType) -> Result<String> {
    let v = gen_expr(cg, nth(args, i)?)?;
    Ok(crate::cast::coerce_to(cg, v, ty)?.operand)
}

/// Call a runtime producer of a float buffer and wrap its nullable handle,
/// tagged `Gpu#double`.
fn fallible_buffer(
    cg: &mut Codegen,
    callee: &str,
    params: &str,
    operands: &[&str],
    fallback: &str,
) -> Result<Value> {
    let owner = elem_owner(Some(LType::F64));
    crate::extern_call::fallible_handle(cg, callee, params, operands, &owner, Some(fallback))
}

/// `gpuMatMul(left, right, rows, inner, cols)` [GPU-MATMUL].
pub(super) fn mat_mul(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let left = buffer_arg(cg, args, 0)?.operand;
    let right = buffer_arg(cg, args, 1)?.operand;
    let rows = arg_as(cg, args, 2, LType::I64)?;
    let inner = arg_as(cg, args, 3, LType::I64)?;
    let cols = arg_as(cg, args, 4, LType::I64)?;
    fallible_buffer(
        cg,
        "osprey_gpu_matmul",
        "i8*, i8*, i64, i64, i64",
        &[&left, &right, &rows, &inner, &cols],
        "gpuMatMul: operand lengths disagree with the stated shape",
    )
}

/// `gpuLoad(path, offset, count)` [GPU-BUFFER-LOAD].
pub(super) fn load(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let path = arg_as(cg, args, 0, LType::Str)?;
    let offset = arg_as(cg, args, 1, LType::I64)?;
    let count = arg_as(cg, args, 2, LType::I64)?;
    fallible_buffer(
        cg,
        "osprey_gpu_load_f32",
        "i8*, i64, i64",
        &[&path, &offset, &count],
        "gpuLoad: could not read the values",
    )
}

/// `gpuLoadTensor(path, name)` [GPU-BUFFER-TENSOR].
pub(super) fn load_tensor(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let path = arg_as(cg, args, 0, LType::Str)?;
    let name = arg_as(cg, args, 1, LType::Str)?;
    fallible_buffer(
        cg,
        "osprey_gpu_load_tensor",
        "i8*, i8*",
        &[&path, &name],
        "gpuLoadTensor: could not read the tensor",
    )
}
