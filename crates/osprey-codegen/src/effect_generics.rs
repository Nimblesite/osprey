//! Generic-effect support: a generic effect's operations keep ONE erased ABI
//! program-wide (every type-parameter slot travels as a boxed `i64`), so the
//! C runtime's name-keyed handler stack needs no changes. Perform sites box
//! erased arguments and unbox erased results against the operation signature
//! inference resolved for that site; handler arms do the inverse at entry and
//! return. Implements [EFFECTS-GENERIC-RUNTIME].

use crate::builder::Codegen;
use crate::conv::box_to_i64;
use crate::effects::unbox_coro_value;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use crate::types::{ltype_of, owner_name, result_inner};
use osprey_ast::Position;
use osprey_types::{HandlerSite, PerformSite, Type};

/// The instantiation inference resolved for the `perform` at `position`.
pub(crate) fn site_perform_op(cg: &Codegen, position: Option<Position>) -> Option<PerformSite> {
    let p = position?;
    cg.prog.performs.get(&(p.line, p.column)).cloned()
}

/// The instantiation inference resolved for the `handle` at `position`.
pub(crate) fn site_handler_ops(cg: &Codegen, position: Option<Position>) -> Option<HandlerSite> {
    let p = position?;
    cg.prog.handler_ops.get(&(p.line, p.column)).cloned()
}

/// The runtime handler-stack key for an effect instantiation: a generic
/// effect's RESOLVED type arguments are mangled into the name
/// (`Stash$int`), so a handler only satisfies performs of the SAME
/// instantiation — a mismatch fails loudly as an unhandled effect instead of
/// type-confusing values. Monomorphic effects and unresolved instantiations
/// keep the bare name (byte-identical to pre-generics programs). The C
/// runtime treats keys as opaque strings, so it needs no changes. Implements
/// [EFFECTS-GENERIC-RUNTIME].
pub(crate) fn runtime_effect_key(effect: &str, args: &[Type]) -> String {
    if args.is_empty() || args.iter().any(osprey_types::has_type_var) {
        return effect.to_string();
    }
    let rendered: Vec<String> = args.iter().map(ToString::to_string).collect();
    format!("{effect}${}", rendered.join("$"))
}

/// Adapt a value to an erased operation slot's resolved semantic type without
/// boxing it yet. Keeping this separate lets escaping handler returns retain the
/// adapted value itself, including a newly-created `Success` block.
pub(crate) fn adapt_erased(
    cg: &mut Codegen,
    value: Value,
    resolved: Option<&Type>,
) -> Result<Value> {
    let expected_inner = resolved.and_then(result_inner);
    match (value.result_inner, expected_inner) {
        (Some(_), None) if resolved.is_some() => Err(CodegenError::invalid(
            "cannot pass an unhandled Result through a plain effect slot",
        )),
        (Some(_), Some(inner)) => crate::result::repack_to_inner(cg, value, inner),
        (None, Some(inner)) => crate::result::make_ok(cg, value, inner),
        _ => Ok(value),
    }
}

/// Box a value crossing an erased operation slot without losing a `Result`.
/// Floats bitcast (never `fptosi`) and pointers/strings use `ptrtoint`.
pub(crate) fn box_erased(cg: &mut Codegen, value: Value, resolved: Option<&Type>) -> Result<Value> {
    let value = adapt_erased(cg, value, resolved)?;
    Ok(box_raw_value(cg, value))
}

/// Box a codegen value exactly as represented, including the complete Result
/// block when present.
pub(crate) fn box_raw_value(cg: &mut Codegen, value: Value) -> Value {
    box_to_i64(cg, value)
}

/// Unbox an erased operation slot's `i64` to the type inference resolved for
/// this site, re-tagging nominal aggregates with their owner so field access
/// and pattern matching recover their layout.
pub(crate) fn unbox_erased(cg: &mut Codegen, raw: &str, resolved: &Type) -> Value {
    let target = ltype_of(resolved);
    let value = unbox_coro_value(cg, raw, target, result_inner(resolved));
    if value.osp_ty.is_none() && target == LType::Ptr {
        return value.with_owner(owner_name(&cg.prog, resolved));
    }
    value
}
