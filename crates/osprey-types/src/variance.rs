//! Declaration-site variance validation: a covariant (`out`) type parameter
//! may appear only in output positions, a contravariant (`in`) parameter only
//! in input positions. Function types flip polarity for their parameters;
//! nested constructors compose polarities through their declared variance.
//! Implements [TYPE-VARIANCE-POSITIONS].

use crate::convert::{parse_fn_sig, type_name_to_type};
use crate::ctx::InferCtx;
use crate::error::TypeError;
use crate::ty::Type;
use osprey_ast::{EffectOperation, TypeParam, TypeVariant, Variance};
use std::collections::HashMap;

/// The polarity of a position while walking a declared type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    Out,
    In,
    Both,
}

/// Compose the current polarity with a constructor argument's declared
/// variance: an invariant argument position demands both directions; a
/// contravariant argument flips the polarity.
fn compose(outer: Polarity, arg: Variance) -> Polarity {
    match (outer, arg) {
        (_, Variance::Invariant) | (Polarity::Both, _) => Polarity::Both,
        (p, Variance::Covariant) => p,
        (p, Variance::Contravariant) => flip(p),
    }
}

fn flip(p: Polarity) -> Polarity {
    match p {
        Polarity::Out => Polarity::In,
        Polarity::In => Polarity::Out,
        Polarity::Both => Polarity::Both,
    }
}

/// Whether a parameter declared with `variance` may legally sit at `polarity`.
fn legal(variance: Variance, polarity: Polarity) -> bool {
    match variance {
        Variance::Invariant => true,
        Variance::Covariant => polarity == Polarity::Out,
        Variance::Contravariant => polarity == Polarity::In,
    }
}

fn describe(polarity: Polarity) -> &'static str {
    match polarity {
        Polarity::Out => "output",
        Polarity::In => "input",
        Polarity::Both => "invariant",
    }
}

/// Validate every variance-annotated parameter's positions in a `type`
/// declaration's variant fields. Field types are output positions; function
/// types flip their parameter side.
pub(crate) fn validate_type_decl(
    ctx: &InferCtx,
    owner: &str,
    params: &[TypeParam],
    variants: &[TypeVariant],
) -> Vec<TypeError> {
    let Some(declared) = non_invariant_declared_map(params) else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    for variant in variants {
        for field in &variant.fields {
            let ty = type_name_to_type(&field.ty, &HashMap::new());
            walk(
                ctx,
                &ty,
                Polarity::Out,
                &declared,
                &mut errors,
                &format!("field `{}` of `{owner}`", field.name),
            );
        }
    }
    errors
}

/// Validate an `effect` declaration's operation signatures: operation
/// parameters are input positions, operation results output positions.
pub(crate) fn validate_effect_decl(
    ctx: &InferCtx,
    owner: &str,
    params: &[TypeParam],
    operations: &[EffectOperation],
) -> Vec<TypeError> {
    let Some(declared) = non_invariant_declared_map(params) else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    for op in operations {
        let (ps, ret) = parse_fn_sig(&op.ty, &HashMap::new());
        let label = format!("operation `{}` of `{owner}`", op.name);
        for p in &ps {
            walk(ctx, p, Polarity::In, &declared, &mut errors, &label);
        }
        walk(ctx, &ret, Polarity::Out, &declared, &mut errors, &label);
    }
    errors
}

/// Reject variance annotations on a function's type parameters — variance is
/// declaration-site on types and effects only. Implements
/// [TYPE-VARIANCE-DECL].
pub(crate) fn reject_fn_variance(name: &str, params: &[TypeParam]) -> Vec<TypeError> {
    params
        .iter()
        .filter(|tp| tp.variance != Variance::Invariant)
        .map(|tp| {
            TypeError::new(format!(
                "variance annotations are only valid on type and effect declarations; \
                 remove the marker from `{}` on function `{name}`",
                tp.name
            ))
        })
        .collect()
}

fn non_invariant_declared_map(params: &[TypeParam]) -> Option<HashMap<String, Variance>> {
    let declared: HashMap<_, _> = params
        .iter()
        .map(|p| (p.name.clone(), p.variance))
        .collect();
    declared
        .values()
        .any(|variance| *variance != Variance::Invariant)
        .then_some(declared)
}

/// Walk a declared type at the given polarity, reporting every
/// variance-annotated parameter that sits at an incompatible position.
fn walk(
    ctx: &InferCtx,
    ty: &Type,
    polarity: Polarity,
    declared: &HashMap<String, Variance>,
    errors: &mut Vec<TypeError>,
    label: &str,
) {
    match ty {
        Type::Con { name, args } => {
            if let Some(v) = declared.get(name) {
                if !legal(*v, polarity) {
                    let marker = if *v == Variance::Covariant {
                        "out"
                    } else {
                        "in"
                    };
                    errors.push(TypeError::new(format!(
                        "type parameter `{name}` is declared `{marker} {name}` but appears \
                         in {} position in {label}",
                        describe(polarity)
                    )));
                }
            }
            for (i, a) in args.iter().enumerate() {
                let av = ctx
                    .variance_of(name)
                    .and_then(|vs| vs.get(i))
                    .copied()
                    .unwrap_or(Variance::Invariant);
                walk(ctx, a, compose(polarity, av), declared, errors, label);
            }
        }
        Type::Fun { params, ret } => {
            for p in params {
                walk(ctx, p, flip(polarity), declared, errors, label);
            }
            walk(ctx, ret, polarity, declared, errors, label);
        }
        Type::Record { fields, .. } => {
            for f in fields.values() {
                walk(ctx, f, polarity, declared, errors, label);
            }
        }
        Type::Union { variants, .. } => {
            for v in variants {
                walk(ctx, v, polarity, declared, errors, label);
            }
        }
        Type::Var(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osprey_ast::TypeField;

    fn tp(name: &str, variance: Variance) -> TypeParam {
        TypeParam {
            name: name.into(),
            variance,
        }
    }

    fn variant(fields: &[(&str, &str)]) -> TypeVariant {
        TypeVariant {
            name: "V".into(),
            fields: fields
                .iter()
                .map(|(n, t)| TypeField {
                    name: (*n).to_string(),
                    ty: (*t).to_string(),
                    constraint: None,
                })
                .collect(),
        }
    }

    #[test]
    fn covariant_param_is_legal_in_field_and_return_positions() {
        let ctx = InferCtx::new();
        let params = [tp("T", Variance::Covariant)];
        let ok = validate_type_decl(
            &ctx,
            "Source",
            &params,
            &[variant(&[("produce", "T"), ("gen", "(int) -> T")])],
        );
        assert!(ok.is_empty(), "unexpected: {ok:?}");
    }

    #[test]
    fn covariant_param_in_fn_parameter_position_is_rejected() {
        let ctx = InferCtx::new();
        let params = [tp("T", Variance::Covariant)];
        let errs = validate_type_decl(&ctx, "Bad", &params, &[variant(&[("f", "(T) -> int")])]);
        assert!(errs.iter().any(|e| e.message.contains("input position")));
    }

    #[test]
    fn contravariant_param_positions_flip() {
        let ctx = InferCtx::new();
        let params = [tp("T", Variance::Contravariant)];
        // Legal: T as a function parameter (input).
        let ok = validate_type_decl(
            &ctx,
            "Sink",
            &params,
            &[variant(&[("accept", "(T) -> Unit")])],
        );
        assert!(ok.is_empty(), "unexpected: {ok:?}");
        // Illegal: T as a plain field (output).
        let errs = validate_type_decl(&ctx, "Bad", &params, &[variant(&[("held", "T")])]);
        assert!(errs.iter().any(|e| e.message.contains("output position")));
        // Double flip: T in the parameter of a parameter-function is output again.
        let errs = validate_type_decl(
            &ctx,
            "Bad2",
            &params,
            &[variant(&[("g", "((T) -> int) -> int")])],
        );
        assert!(errs.iter().any(|e| e.message.contains("output position")));
    }

    #[test]
    fn nested_constructor_variance_composes() {
        let mut ctx = InferCtx::new();
        ctx.set_variance("List", vec![Variance::Covariant]);
        let co = [tp("T", Variance::Covariant)];
        // T inside a covariant List keeps its polarity: legal.
        let ok = validate_type_decl(&ctx, "Ok", &co, &[variant(&[("items", "List<T>")])]);
        assert!(ok.is_empty(), "unexpected: {ok:?}");
        // An unregistered constructor's args are invariant positions: illegal
        // for an `out` param.
        let errs = validate_type_decl(&ctx, "Bad", &co, &[variant(&[("cell", "Cell<T>")])]);
        assert!(errs
            .iter()
            .any(|e| e.message.contains("invariant position")));
        // A contravariant constructor argument flips the polarity, so an `out T`
        // inside a `Sink` lands in input position after all.
        ctx.set_variance("Sink", vec![Variance::Contravariant]);
        let errs = validate_type_decl(&ctx, "Bad2", &co, &[variant(&[("s", "Sink<T>")])]);
        assert!(
            errs.iter().any(|e| e.message.contains("input position")),
            "{errs:?}"
        );
        // An invariant parameter carries no constraint, so every position suits
        // it — including the one that rejected `out T` above.
        let inv = [tp("T", Variance::Invariant)];
        let ok = validate_type_decl(&ctx, "Cell", &inv, &[variant(&[("s", "Sink<T>")])]);
        assert!(ok.is_empty(), "unexpected: {ok:?}");
    }

    #[test]
    fn effect_ops_check_params_in_and_result_out() {
        let ctx = InferCtx::new();
        let out_t = [tp("T", Variance::Covariant)];
        let op = |ty: &str| EffectOperation {
            name: "op".into(),
            ty: ty.into(),
            parameters: Vec::new(),
            return_type: String::new(),
        };
        // `out T` in result position: legal.
        assert!(validate_effect_decl(&ctx, "Ask", &out_t, &[op("fn() -> T")]).is_empty());
        // `out T` in an operation parameter: rejected.
        let errs = validate_effect_decl(&ctx, "Bad", &out_t, &[op("fn(T) -> Unit")]);
        assert!(errs.iter().any(|e| e.message.contains("input position")));
        // `in T` flips: parameter legal, result rejected.
        let in_t = [tp("T", Variance::Contravariant)];
        assert!(validate_effect_decl(&ctx, "Emit", &in_t, &[op("fn(T) -> Unit")]).is_empty());
        let errs = validate_effect_decl(&ctx, "Bad2", &in_t, &[op("fn() -> T")]);
        assert!(errs.iter().any(|e| e.message.contains("output position")));
    }

    #[test]
    fn fn_type_params_reject_variance_markers() {
        let errs = reject_fn_variance("map", &[tp("T", Variance::Covariant)]);
        assert!(errs
            .iter()
            .any(|e| e.message.contains("only valid on type")));
        assert!(reject_fn_variance("map", &[tp("T", Variance::Invariant)]).is_empty());
    }
}
