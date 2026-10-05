//! Type checking: publish.
use super::{
    dedupe_sites, erased_type_params, resolve_op, resolve_positioned, type_name_to_type, Checker,
    HashMap, Program, Type,
};

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

    let functions = checker
        .fn_sigs
        .iter()
        .map(|(name, (params, ret))| {
            let rp = params.iter().map(|t| checker.ctx.apply(t)).collect();
            let rr = checker.ctx.apply(ret);
            (name.clone(), (rp, rr))
        })
        .collect();
    let ctors = publish_ctors(&checker);
    let unions = checker.union_variants.clone();
    let effects = publish_effects(&checker);
    let lambda_tys = checker.lambda_tys.clone();
    let let_tys = checker.let_tys.clone();
    let list_tys = checker.list_tys.clone();
    let lambdas = resolve_positioned(&mut checker.ctx, &lambda_tys);
    let lets = resolve_positioned(&mut checker.ctx, &let_tys);
    let lists = resolve_positioned(&mut checker.ctx, &list_tys);
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
        function_effects: checker.function_effects.clone(),
        ctors,
        unions,
        effects,
        lambdas,
        lets,
        lists,
        performs,
        handler_ops,
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
