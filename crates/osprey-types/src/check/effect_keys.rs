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
                    effect_type_key(&checker.ctx, &ty)
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
/// A non-generic named record keeps its useful source identity. A generic
/// record uses its instantiated fields so `Box<int>` and `Box<string>` cannot
/// collapse into the same effect requirement ([EFFECTS-GENERIC-INSTANTIATION]).
pub(super) fn effect_type_key(ctx: &InferCtx, ty: &Type) -> String {
    match ty {
        Type::Record { name, .. } if !name.is_empty() && ctx.record_fields(name, &[]).is_some() => {
            name.clone()
        }
        Type::Record { fields, .. } => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, field)| format!("{name}: {}", effect_type_key(ctx, field)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Con { name, args } if !args.is_empty() => {
            format!("{name}<{}>", effect_keys(ctx, args).join(", "))
        }
        Type::Fun { params, ret } => format!(
            "({}) -> {}",
            effect_keys(ctx, params).join(", "),
            effect_type_key(ctx, ret)
        ),
        _ => ty.to_string(),
    }
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
