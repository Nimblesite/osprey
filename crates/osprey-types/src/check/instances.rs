//! Type checking: instances.
use super::{
    collect_site_candidates, dedupe_sites, display_effect_arguments, effect_keys, effect_type_key,
    push_site_candidate, resolved_binders, resolved_declared_rows, resolved_effect_keys,
    type_is_resolved, Checker, HashMap, HashSet, Type, VarId,
};

/// Compose call substitutions through aliases before specializing effects.
pub(super) fn resolved_instantiations(
    checker: &mut Checker,
) -> HashMap<usize, HashMap<VarId, Type>> {
    let substitutions: HashMap<_, HashMap<_, _>> = checker
        .instantiations
        .iter()
        .map(|(site, bindings)| {
            (
                *site,
                bindings
                    .iter()
                    .map(|(var, ty)| (*var, checker.ctx.apply(ty)))
                    .collect(),
            )
        })
        .collect();
    substitutions
        .iter()
        .map(|(site, bindings)| {
            (
                *site,
                crate::applications::expanded(bindings, &substitutions),
            )
        })
        .collect()
}
/// Preserve open as well as closed argument types for handler refinement.
pub(super) fn resolved_effect_arguments(checker: &mut Checker) -> Vec<Vec<Type>> {
    checker
        .perform_tys
        .clone()
        .into_iter()
        .map(|(_, _, args)| {
            args.iter()
                .map(|arg| checker.ctx.apply(arg))
                .collect::<Vec<_>>()
        })
        .collect()
}
/// Publish the current inference solution for the closed-program effect proof.
pub(super) fn effect_instances(checker: &mut Checker) -> crate::effect_rows::Instances {
    let substitutions = resolved_instantiations(checker);
    let resolved_arguments = resolved_effect_arguments(checker);
    let argument_types: HashMap<_, _> = resolved_arguments
        .iter()
        .cloned()
        .chain(substitutions.values().flat_map(|bindings| {
            resolved_arguments.iter().map(move |args| {
                args.iter()
                    .map(|arg| crate::env::subst_vars(arg, bindings))
                    .collect()
            })
        }))
        .map(|args| (resolved_effect_keys(checker, &args), args))
        .collect();
    let perform_tys = checker.perform_tys.clone();
    let perform_actual_tys = checker.perform_actual_tys.clone();
    let handler_tys = checker.handler_tys.clone();
    let scoped_performs =
        collect_site_candidates(perform_tys.into_iter().map(|(position, _, arguments)| {
            (
                (position.line, position.column),
                resolved_effect_keys(checker, &arguments),
            )
        }));
    let mut actual_performs = HashMap::new();
    let mut unresolved_actual_sites = HashSet::new();
    for (position, arguments) in perform_actual_tys {
        let key = (position.line, position.column);
        let arguments: Vec<_> = arguments
            .iter()
            .map(|argument| checker.ctx.apply(argument))
            .collect();
        if arguments.iter().all(type_is_resolved) {
            push_site_candidate(
                &mut actual_performs,
                key,
                effect_keys(&checker.ctx, &arguments),
            );
        } else {
            let _ = unresolved_actual_sites.insert(key);
        }
    }
    let mut performs = scoped_performs;
    for (key, candidates) in actual_performs {
        if unresolved_actual_sites.contains(&key) {
            for candidate in candidates {
                push_site_candidate(&mut performs, key, candidate);
            }
        } else {
            let _ = performs.insert(key, candidates);
        }
    }
    let declared_rows = resolved_declared_rows(checker);
    crate::effect_rows::Instances {
        expression_types: resolved_expression_types(checker).into(),
        methods: checker.methods.clone(),
        non_callable_call_error: checker
            .errors
            .iter()
            .any(|error| error.message.starts_with("cannot call non-function")),
        performs,
        handlers: dedupe_sites(handler_tys.into_iter().map(|(position, arguments, _)| {
            (
                (position.line, position.column),
                resolved_effect_keys(checker, &arguments),
            )
        })),
        declared_rows,
        display_arguments: display_effect_arguments(checker, &argument_types),
        argument_types,
        instantiations: substitutions
            .into_iter()
            .map(|(site, bindings)| {
                (
                    site,
                    bindings
                        .into_iter()
                        .map(|(var, ty)| {
                            (
                                Type::Var(var).to_string(),
                                effect_type_key(&checker.ctx, &ty),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
        binders: resolved_binders(checker),
        ..Default::default()
    }
}
/// Every inferred expression type under the final substitution, so the effect
/// proof can tell an `int` operator site from a `float` one.
/// Implements [ARITH-EFFECT-DISCHARGE].
pub(super) fn resolved_expression_types(checker: &mut Checker) -> HashMap<usize, Type> {
    checker
        .expression_types
        .iter()
        .map(|(site, ty)| (*site, checker.ctx.apply(ty)))
        .collect()
}
