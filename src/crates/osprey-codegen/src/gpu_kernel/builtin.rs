//! Scalar intrinsic kernels reuse ordinary builtin lowering. [GPU-KERNEL-EXTRACT]
use super::{Callback, Codegen, Expr, GpuKernelMode, LType, Parameter, Result};
use osprey_types::Type;

/// Preserve an applied builtin's name instead of allocating a closure cell.
pub(super) fn callback(cg: &Codegen, expression: &Expr) -> Option<Callback> {
    if cg.gpu_kernels() == GpuKernelMode::Inline {
        return None;
    }
    let name = match expression {
        Expr::Identifier(name) => name,
        Expr::TypeApply { function, .. } => return callback(cg, function),
        _ => return None,
    };
    (arity(name).is_some() && !crate::builtin_values::shadowed(cg, name))
        .then(|| Callback::Named(name.clone()))
}

/// A builtin lifts only when its arity matches and every combinator slot is a
/// scalar word [GPU-BUFFER-ELEM].
pub(super) fn admissible(name: &str, slots: &[LType]) -> bool {
    arity(name) == Some(slots.len()) && slots.iter().copied().all(crate::gpu::is_scalar)
}

/// Use the authoritative builtin scheme; host handles and Result slots decline.
fn arity(name: &str) -> Option<usize> {
    let Type::Fun { params, ret } = osprey_types::builtin_function_type(name)? else {
        return None;
    };
    params
        .iter()
        .chain(std::iter::once(ret.as_ref()))
        .all(osprey_types::is_gpu_scalar)
        .then_some(params.len())
}

/// Synthesize only the call surface; ordinary lowering owns intrinsic semantics.
pub(super) fn lift(cg: &mut Codegen, name: String, slots: &[LType]) -> Result<Callback> {
    let parameters: Vec<_> = slots
        .iter()
        .enumerate()
        .map(|(index, _)| Parameter {
            name: format!("$kernel_arg{index}"),
            ty: None,
            inline_constraint: false,
        })
        .collect();
    let body = Expr::Call {
        function: Box::new(Expr::Identifier(name)),
        arguments: parameters
            .iter()
            .map(|p| Expr::Identifier(p.name.clone()))
            .collect(),
        named_arguments: Vec::new(),
    };
    super::lift(cg, (parameters, body, None), None, slots)
}
