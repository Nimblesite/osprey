//! Type checking: publish.
use super::{
    dedupe_sites, erased_type_params, open_lambda_vars, resolve_aliases, resolve_op,
    resolve_positioned, type_name_to_type, Checker, HashMap, HashSet, Program, Type,
};

pub(super) fn resolved_signatures(
    checker: &mut Checker,
    resolve: fn(&mut crate::ctx::InferCtx, &Type) -> Type,
) -> HashMap<String, (Vec<Type>, Type)> {
    checker
        .fn_sigs
        .iter()
        .map(|(name, (params, ret))| {
            let params = params
                .iter()
                .map(|ty| resolve(&mut checker.ctx, ty))
                .collect();
            (name.clone(), (params, resolve(&mut checker.ctx, ret)))
        })
        .collect()
}

/// Runtime site tables keep field layouts after the effect proof has retained
/// and checked the complete nominal instance, including phantom parameters.
pub(super) fn runtime_site_type(ctx: &crate::ctx::InferCtx, ty: &Type) -> Type {
    match ty {
        Type::Con { name, args } if !args.is_empty() => runtime_record(ctx, name, args),
        Type::Fun { params, ret } => Type::fun(
            params.iter().map(|ty| runtime_site_type(ctx, ty)).collect(),
            runtime_site_type(ctx, ret),
        ),
        _ => ty.clone(),
    }
}

fn runtime_record(ctx: &crate::ctx::InferCtx, name: &str, args: &[Type]) -> Type {
    ctx.record_fields(name, args).map_or_else(
        || {
            Type::con(
                name.to_owned(),
                args.iter().map(|ty| runtime_site_type(ctx, ty)).collect(),
            )
        },
        |fields| Type::Record {
            name: name.to_owned(),
            fields,
        },
    )
}

/// The erased view of every effect: each declared type parameter resolves to a
/// type variable, which the backend lowers to its uniform boxed representation
/// — one operation ABI per program regardless of how many instantiations
/// exist. Implements [EFFECTS-GENERIC-RUNTIME].
pub(super) fn publish_effects(
    checker: &Checker,
) -> HashMap<String, HashMap<String, crate::info::OpType>> {
    checker
        .effects
        .iter()
        .map(|(name, info)| {
            let pmap = erased_type_params(&info.type_params);
            (name.clone(), Checker::instantiate_ops(info, &pmap))
        })
        .collect()
}
/// Capability sites for the backend: position → the capability's effect.
/// Implements [EFFECTS-INSTANCE].
fn published_capabilities(checker: &Checker) -> HashMap<(u32, u32), String> {
    checker
        .capability_sites
        .iter()
        .map(|(position, effect, _)| ((position.line, position.column), effect.clone()))
        .collect()
}

pub(super) fn publish_program(
    program: &Program,
    mut checker: Checker,
    applications: bool,
) -> crate::info::ProgramTypes {
    use crate::info::ProgramTypes;
    let call_bindings = if applications {
        crate::applications::collect(program, &checker.instantiations, &mut checker.ctx)
    } else {
        HashMap::new()
    };

    let functions = resolved_signatures(&mut checker, crate::ctx::InferCtx::apply);
    let ctors = publish_ctors(&checker);
    let unions = checker.union_variants.clone();
    let effects = publish_effects(&checker);
    let lambda_tys = checker.lambda_tys.clone();
    let let_tys = checker.let_tys.clone();
    let list_tys = checker.list_tys.clone();
    let mut lambdas = resolve_positioned(&mut checker.ctx, &lambda_tys);
    let mut lets = resolve_positioned(&mut checker.ctx, &let_tys);
    let mut lists = resolve_positioned(&mut checker.ctx, &list_tys);
    let handler_sites: HashSet<_> = checker
        .handler_tys
        .iter()
        .map(|(position, _, _)| (position.line, position.column))
        .collect();
    let lambda_candidates = open_lambda_vars(&lambdas, &handler_sites, &mut checker.ctx);
    resolve_aliases(
        &lambda_candidates,
        &mut [&mut lambdas, &mut lets, &mut lists],
        &checker.instantiations,
        &mut checker.ctx,
    );
    for table in [&mut lambdas, &mut lets, &mut lists] {
        for ty in table.values_mut() {
            *ty = runtime_site_type(&checker.ctx, ty);
        }
    }
    let perform_tys = checker.perform_tys.clone();
    let performs = dedupe_sites(perform_tys.iter().map(|(pos, op, args)| {
        let site = crate::info::PerformSite {
            op: resolve_op(&mut checker.ctx, op),
            effect_args: args.iter().map(|t| checker.ctx.apply(t)).collect(),
        };
        ((pos.line, pos.column), site)
    }));
    let handler_tys = checker.handler_tys.clone();
    let handler_ops = dedupe_sites(handler_tys.iter().map(|(pos, args, ops)| {
        let site = crate::info::HandlerSite {
            effect_args: args.iter().map(|t| checker.ctx.apply(t)).collect(),
            ops: ops
                .iter()
                .map(|(n, op)| (n.clone(), resolve_op(&mut checker.ctx, op)))
                .collect(),
        };
        ((pos.line, pos.column), site)
    }));
    ProgramTypes {
        obligations: publish_obligations(&mut checker),
        methods: crate::methods::collect(program, &checker.methods),
        functions,
        signal_queries: checker.signal_queries.clone(),
        function_effects: checker.function_effects.clone(),
        ctors,
        unions,
        effects,
        lambdas,
        lets,
        lists,
        performs,
        handler_ops,
        capabilities: published_capabilities(&checker),
        call_bindings,
        applications: checker
            .application_tys
            .iter()
            .map(|(position, bindings)| {
                (
                    (position.line, position.column),
                    bindings
                        .iter()
                        .map(|(var, ty)| (*var, checker.ctx.apply(ty)))
                        .collect(),
                )
            })
            .collect(),
        declared_params: checker
            .fn_typarams
            .iter()
            .map(|(name, bindings)| {
                (
                    name.clone(),
                    bindings
                        .iter()
                        .map(|(name, ty)| (name.clone(), checker.ctx.apply(ty)))
                        .collect(),
                )
            })
            .collect(),
    }
}
/// Publish constructor fields using the common erased binder numbering.
pub(super) fn publish_obligations(checker: &mut Checker) -> HashMap<String, Vec<(String, Type)>> {
    checker
        .scheme_obligations
        .iter()
        .map(|(owner, obligations)| {
            let resolved = obligations
                .iter()
                .map(|(name, ty)| (name.clone(), checker.ctx.apply(ty)))
                .collect();
            (owner.clone(), resolved)
        })
        .collect()
}
pub(super) fn publish_ctors(checker: &Checker) -> HashMap<String, crate::info::CtorLayout> {
    use crate::info::CtorLayout;
    checker
        .ctors
        .iter()
        .map(|(name, info)| {
            // A declared type parameter resolves to a type variable — the
            // backend lowers a variable to its uniform boxed representation,
            // which is exactly the generic-payload rule.
            let pmap = erased_type_params(&info.type_params);
            let fields = info
                .fields
                .iter()
                .map(|(f, written)| (f.clone(), type_name_to_type(written, &pmap)))
                .collect();
            (
                name.clone(),
                CtorLayout {
                    owner: info.owner.clone(),
                    owner_is_record: info.owner_is_record,
                    type_params: info.type_params.clone(),
                    fields,
                },
            )
        })
        .collect()
}
