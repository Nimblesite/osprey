//! Effect row gpu.
use super::{
    expression_name, requirement_name, Analyzer, CallableEnv, Expr, Position, Requirement,
    TypeError,
};

/// The argument slot holding the kernel callback of a GPU combinator, if
/// `name` is one. Implements [GPU-KERNEL-PURE]
/// (docs/specs/0034-GPUComputation.md).
pub(super) fn gpu_kernel_slot(name: &str) -> Option<usize> {
    match name {
        "gpuMap" | "gpuFilter" => Some(1),
        "gpuFold" | "gpuZipWith" | "gpuScan" => Some(2),
        _ => None,
    }
}

/// Reject a GPU combinator call whose kernel is not provably pure
/// [GPU-KERNEL-PURE]. A surrounding handler cannot lift the restriction: a
/// handler makes an effect dischargeable on the host, but a kernel body cannot
/// leave the device to reach one, so the requirement is purity, and any
/// kernel whose effects cannot be proven absent fails closed.
pub(super) fn validate_gpu_kernel(
    analyzer: &Analyzer<'_>,
    expression: &Expr,
    scope: &[String],
    env: &CallableEnv,
    errors: &mut Vec<TypeError>,
) {
    let Expr::Call {
        function,
        arguments,
        ..
    } = expression
    else {
        return;
    };
    let Some(slot) = expression_name(function)
        .filter(|name| !env.shadowed.contains(*name))
        .and_then(gpu_kernel_slot)
    else {
        return;
    };
    let Some(kernel) = arguments.get(slot) else {
        return;
    };
    if let Some(message) = gpu_kernel_verdict(analyzer, kernel, scope, env) {
        errors.push(TypeError::new(message).with_pos(kernel_position(kernel)));
    }
}

/// The rejection message for a GPU combinator kernel, or `None` when the kernel
/// is stage-legal.
///
/// Two rejections, and the difference is what the checker could see. When it
/// cannot resolve the callback to a definition it says so and fails closed
/// ([GPU-KERNEL-PURE]); when it CAN, and the row it read still needs a runtime
/// handler, it names those operations instead — a kernel is no longer required
/// to be pure, only to have nothing left to answer at the boundary.
/// Implements [STAGE-GPU-LEGAL], [STAGE-GPU-DIAG].
pub(super) fn gpu_kernel_verdict(
    analyzer: &Analyzer<'_>,
    kernel: &Expr,
    scope: &[String],
    env: &CallableEnv,
) -> Option<String> {
    let Some(callee) = analyzer.callable(kernel, scope, env) else {
        return Some(String::from(
            "cannot prove GPU kernel pure; pass a named function or an inline lambda",
        ));
    };
    let row = analyzer.invoke_with_values(callee, &[]);
    // Before discharge the kernel's row still names the static operations an
    // enclosing `handle static` answers; those are rewritten away, so only the
    // rest is dynamic. After discharge no static handler remains and an
    // unanswered static request is exactly a residual requirement.
    let dynamic: Vec<&Requirement> = row
        .required
        .iter()
        .filter(|requirement| {
            // Host kernels use the ordinary arithmetic policy. [ARITH-EFFECT-DISCHARGE]
            requirement.effect != osprey_ast::ARITH_EFFECT
                && !env
                    .static_answers
                    .contains(&(requirement.effect.clone(), requirement.operation.clone()))
        })
        .collect();
    if !dynamic.is_empty() || !row.runtime_builtins.is_empty() {
        let performed: Vec<String> = dynamic
            .into_iter()
            .map(requirement_name)
            .chain(row.runtime_builtins)
            .collect();
        return Some(format!(
            "kernel body is not stage-legal; it requires dynamic effects: {}",
            performed.join(", ")
        ));
    }
    if row.unresolved_dynamic_call || !row.parameter_uses.is_empty() {
        return Some(String::from(
            "cannot prove GPU kernel pure; pass a named function or an inline lambda",
        ));
    }
    None
}

/// A kernel expression's own source position, when it carries one.
pub(super) fn kernel_position(kernel: &Expr) -> Option<Position> {
    match kernel {
        Expr::Lambda { position, .. } => *position,
        _ => None,
    }
}
