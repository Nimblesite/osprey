//! Type checking: effect keys.
use super::{Checker, HashMap, InferCtx, Type};

pub(super) fn display_effect_arguments(
    checker: &Checker,
    arguments: &HashMap<Vec<String>, Vec<Type>>,
) -> HashMap<Vec<String>, Vec<String>> {
    arguments
        .iter()
        .map(|(key, types)| {
            let display = types
                .iter()
                .map(|ty| {
                    let ty = crate::ty::map_type_vars(ty, &mut |_| Type::prim(crate::ty::HOLE));
                    let display = if let Type::Con { name, .. } = &ty {
                        if checker.ctx.record_preserves_parameters(name) {
                            super::publish::runtime_site_type(&checker.ctx, &ty)
                        } else {
                            ty
                        }
                    } else {
                        ty
                    };
                    effect_type_key(&checker.ctx, &display)
                })
                .collect();
            (key.clone(), display)
        })
        .collect()
}
/// Each generic function's type parameters, keyed to their resolved effect keys.
pub(super) fn resolved_binders(checker: &mut Checker) -> HashMap<String, HashMap<String, String>> {
    checker
        .fn_typarams
        .iter()
        .map(|(name, binders)| {
            (
                name.clone(),
                binders
                    .iter()
                    .map(|(name, ty)| {
                        let resolved = checker.ctx.apply(ty);
                        (name.clone(), effect_type_key(&checker.ctx, &resolved))
                    })
                    .collect(),
            )
        })
        .collect()
}
pub(super) fn resolved_effect_keys(checker: &mut Checker, arguments: &[Type]) -> Vec<String> {
    let resolved: Vec<_> = arguments
        .iter()
        .map(|argument| checker.ctx.apply(argument))
        .collect();
    effect_keys(&checker.ctx, &resolved)
}
pub(super) fn effect_keys(ctx: &InferCtx, arguments: &[Type]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| effect_type_key(ctx, argument))
        .collect()
}
/// Named generic applications keep all arguments. Existing structural record
/// views use their fields; nullary records keep their declaration's name.
/// Implements [EFFECTS-GENERIC-INSTANTIATION].
pub(super) fn effect_type_key(ctx: &InferCtx, ty: &Type) -> String {
    crate::ty::effect_instance_key(ty, &|name| ctx.record_fields(name, &[]).is_some())
}

pub(super) fn resolved_declared_rows(
    checker: &mut Checker,
) -> HashMap<(u32, u32), Vec<Option<Vec<String>>>> {
    checker
        .declared_effect_rows
        .iter()
        .map(|(position, entries)| {
            (
                *position,
                entries
                    .iter()
                    .map(|entry| {
                        entry.as_ref().map(|arguments| {
                            arguments
                                .iter()
                                .map(|argument| {
                                    let resolved = checker.ctx.apply(argument);
                                    effect_type_key(&checker.ctx, &resolved)
                                })
                                .collect()
                        })
                    })
                    .collect(),
            )
        })
        .collect()
}
