//! Scalar intrinsic kernels reuse ordinary builtin lowering. [GPU-KERNEL-EXTRACT]
use super::{Callback, Codegen, Expr, GpuKernelMode, LType, Parameter, Result};
use osprey_types::{names, Type};

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
    (arity(name).is_some() && !shadowed(cg, name)).then(|| Callback::Named(name.clone()))
}

fn shadowed(cg: &Codegen, name: &str) -> bool {
    cg.lookup(name).is_some()
        || cg.lambda_def(name).is_some()
        || cg.cell_slots.contains_key(name)
        || cg.fn_ptr_locals.contains_key(name)
        || cg.call_aliases.contains_key(name)
        || cg.module_globals.contains_key(name)
        || cg.fn_params.contains_key(name)
}

pub(super) fn admissible(name: &str, slots: &[LType]) -> bool {
    arity(name) == Some(slots.len())
        && slots
            .iter()
            .all(|slot| matches!(slot, LType::I64 | LType::Double | LType::I1))
}

/// Use the authoritative builtin scheme; host handles and Result slots decline.
fn arity(name: &str) -> Option<usize> {
    let Type::Fun { params, ret } = osprey_types::builtin_function_type(name)? else {
        return None;
    };
    params
        .iter()
        .chain(std::iter::once(ret.as_ref()))
        .all(scalar)
        .then_some(params.len())
}

fn scalar(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } => {
            args.is_empty() && matches!(name.as_str(), names::INT | names::FLOAT | names::BOOL)
        }
        _ => false,
    }
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
