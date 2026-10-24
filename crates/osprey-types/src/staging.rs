//! Validate staged data contracts before any operation or unused arm disappears.

#[cfg(test)]
mod factory_tests;
mod forwarders;
mod signals;
use signals::materialize_signal_queries;
pub(crate) use signals::parameter_names as signal_parameter_names;

use crate::TypeError;
use osprey_ast::{mutate, walk_program, AstVisitor, Expr, Program, Stmt};

/// Type-check the original staged program, then discharge its static handlers.
/// Call after module assembly so imported declarations and bindings are known.
/// The residual program still requires the ordinary type/effect check.
/// Implements [STAGE-LOWER-ORDER-PHASE].
///
/// # Errors
/// Returns source type errors or violations of the staging rules.
pub fn lower_static_checked(program: &Program) -> Result<Program, Vec<TypeError>> {
    // The backend lowers what this returns, so the prelude a program uses is
    // attached here, once, ahead of every later phase ([BUILTIN-JSON-TYPED]).
    let attached = crate::prelude::with_prelude(program);
    let program = attached.as_ref();
    // Declaration and syntax rules first: they name the exact defect, where
    // the residual row after inference would only report an unhandled
    // operation ([STAGE-GPU-DIAG], [EFFECTS-GENERIC-DECL]).
    let structural = osprey_ast::stage::validate(program);
    if !structural.is_empty() {
        return Err(stage_errors(structural));
    }
    if !has_staging(program) {
        return osprey_ast::stage::lower(program).map_err(stage_errors);
    }
    let checked = crate::check::infer_checked(program)?;
    // A query reached through a helper reads its callback's signals only in a
    // copy made for that callback ([STAGE-SIGNALS-EXACT]). A copy the checker
    // cannot type keeps the original, conservative program.
    let forwarded = forwarders::specialized(program).and_then(|copy| {
        crate::check::infer_checked(&copy)
            .ok()
            .map(|types| (copy, types))
    });
    let (program, source_types) = match &forwarded {
        Some((copy, types)) => (copy, types),
        None => (program, &checked),
    };
    // Static lowering compares source effect spellings. An inferred
    // `perform Echo.echo(42)` is nevertheless the same operation as the
    // enclosing `handle static Echo<int>`. Use the already checked site
    // instantiations for this backend copy before matching regions; otherwise
    // the rewrite silently leaves a request that the checker proved handled.
    let mut resolved = materialize_signal_queries(program, source_types);
    for statement in &mut resolved.statements {
        mutate::statement_children_mut(statement, &mut |expression| {
            resolve_effect_mentions(expression, source_types);
        });
    }
    osprey_ast::stage::lower(&resolved).map_err(stage_errors)
}

/// Resolved, validated signal may-read sets before static discharge.
/// Implements [STAGE-SIGNALS-EXACT]; callers must assemble modules first.
///
/// # Errors
/// Invalid source never produces a complete-looking dependency report.
pub fn signal_dependencies(
    program: &Program,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, Vec<TypeError>> {
    let _ = lower_static_checked(program)?;
    let types = crate::check::infer_checked(program)?;
    let mut static_effects = StaticEffects::default();
    walk_program(program, &mut static_effects);
    Ok(types
        .function_effects
        .into_iter()
        .map(|(name, requirements)| {
            (
                osprey_ast::symbol::demangle_message(&name).into_owned(),
                static_effects.dependencies(&requirements),
            )
        })
        .collect())
}

#[derive(Default)]
struct StaticEffects(std::collections::BTreeSet<String>);

impl AstVisitor for StaticEffects {
    fn statement(&mut self, statement: &Stmt) {
        if let Stmt::Effect { name, stage, .. } = statement {
            if stage.is_compile_time() {
                let _ = self.0.insert(name.clone());
            }
        }
    }
}

impl StaticEffects {
    fn dependencies(&self, requirements: &crate::EffectRequirements) -> Vec<String> {
        let mut operations: Vec<_> = requirements
            .operations
            .iter()
            .filter(|operation| {
                let effect = operation
                    .rsplit_once('.')
                    .map_or(operation.as_str(), |(effect, _)| effect);
                self.0.contains(osprey_ast::effect_name::base(effect))
            })
            .map(|operation| osprey_ast::symbol::demangle_message(operation).into_owned())
            .collect();
        if requirements.unresolved_callbacks || requirements.unresolved_instances {
            operations.push("<unknown>".to_owned());
        }
        operations.sort();
        operations.dedup();
        operations
    }
}

fn resolve_effect_mentions(expression: &mut Expr, types: &crate::ProgramTypes) {
    match expression {
        Expr::Perform {
            effect,
            position: Some(position),
            ..
        } => {
            if let Some(site) = types.performs.get(&(position.line, position.column)) {
                if let Some(arguments) = concrete_arguments(&site.effect_args, types) {
                    *effect = instantiated(osprey_ast::effect_name::base(effect), &arguments);
                }
            }
        }
        Expr::Handler {
            effect,
            position: Some(position),
            ..
        } => {
            if let Some(site) = types.handler_ops.get(&(position.line, position.column)) {
                if let Some(arguments) = concrete_arguments(&site.effect_args, types) {
                    *effect = instantiated(osprey_ast::effect_name::base(effect), &arguments);
                }
            }
        }
        _ => {}
    }
    mutate::children_mut(expression, &mut |child| {
        resolve_effect_mentions(child, types);
    });
}

fn concrete_arguments(
    arguments: &[crate::Type],
    types: &crate::ProgramTypes,
) -> Option<Vec<String>> {
    arguments
        .iter()
        .map(|argument| source_type(argument, types))
        .collect()
}

fn source_type(ty: &crate::Type, types: &crate::ProgramTypes) -> Option<String> {
    match ty {
        crate::Type::Con { name, args } => {
            let arguments: Option<Vec<_>> =
                args.iter().map(|arg| source_type(arg, types)).collect();
            Some(instantiated(name, &arguments?))
        }
        crate::Type::Fun { params, ret } => {
            let parameters: Option<Vec<_>> =
                params.iter().map(|arg| source_type(arg, types)).collect();
            Some(format!(
                "fn({}) -> {}",
                parameters?.join(", "),
                source_type(ret, types)?
            ))
        }
        crate::Type::Record { name, .. }
            if !name.is_empty()
                && types.ctors.get(name).is_some_and(|layout| {
                    layout.owner_is_record && layout.type_params.is_empty()
                }) =>
        {
            Some(name.clone())
        }
        crate::Type::Record { fields, .. } => {
            let fields: Option<Vec<_>> = fields
                .iter()
                .map(|(name, field)| Some(format!("{name}: {}", source_type(field, types)?)))
                .collect();
            Some(format!("{{ {} }}", fields?.join(", ")))
        }
        crate::Type::Var(_) | crate::Type::Union { .. } => None,
    }
}

fn instantiated(effect: &str, arguments: &[String]) -> String {
    if arguments.is_empty() {
        effect.to_owned()
    } else {
        format!("{effect}<{}>", arguments.join(", "))
    }
}

fn stage_errors(errors: Vec<osprey_ast::stage::StageError>) -> Vec<TypeError> {
    errors
        .into_iter()
        .map(|error| TypeError::new(error.message).with_pos(error.position))
        .collect()
}

pub(crate) fn has_staging(program: &Program) -> bool {
    struct Staging(bool);
    impl AstVisitor for Staging {
        fn statement(&mut self, statement: &Stmt) {
            if let Stmt::Effect { stage, .. } = statement {
                self.0 |= stage.is_compile_time();
            }
        }
        fn expression(&mut self, expression: &Expr) {
            self.0 |= matches!(expression, Expr::Call { function, .. } if matches!(function.as_ref(), Expr::Identifier(name) if signals::is_intrinsic(name)));
            if let Expr::Handler { stage, .. } = expression {
                self.0 |= stage.is_compile_time();
            }
        }
    }
    let mut staging = Staging(false);
    walk_program(program, &mut staging);
    staging.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    const GPU_ARMS_WITH_RESULT_FALLBACKS: &str = "static effect Tensor<B> {\n\
        get: fn(B, int) -> float\n\
        matMul: fn(B, B, int, int, int) -> B\n\
    }\n\
    fn onGpu(work) = {\n\
        handle static Tensor<GpuBuffer<float>> {\n\
            get b i => gpuGet(b, i) ?: 0.0\n\
            matMul a b rows inner cols => gpuMatMul(a, b, rows, inner, cols) ?: toGpu([])\n\
        }\n\
        work()\n\
    }\n\
    fn main() = {\n\
        let ones = toGpu([1.0, 1.0, 1.0, 1.0])\n\
        onGpu(fn() => perform Tensor.get(perform Tensor.matMul(ones, ones, 2, 2, 2), 0))\n\
    }\n";

    /// [PATTERN-RESULT-DEFAULT] types a `Success` payload by its field label,
    /// so the lowering's hygiene must leave `value` and the `?:` binder
    /// unrenamed: renamed, the fallback `toGpu([])` kept an open element type
    /// and the backend read the float buffer back as `int`.
    #[test]
    fn a_static_arm_keeps_its_result_field_labels_so_the_residual_types_its_fallback() {
        let parsed = osprey_syntax::parse_program_with_flavor(
            GPU_ARMS_WITH_RESULT_FALLBACKS,
            osprey_syntax::Flavor::Default,
        );
        let lowered = lower_static_checked(&parsed.program);
        assert!(lowered.is_ok(), "lowering failed: {lowered:?}");
        let Ok(lowered) = lowered else { return };
        let types = crate::infer_program(&lowered);
        let elements: Vec<_> = types
            .lists
            .iter()
            .map(|((line, column), _)| {
                types.list_elem_type(
                    None,
                    Some(osprey_ast::Position {
                        line: *line,
                        column: *column,
                    }),
                )
            })
            .collect();
        assert_eq!(elements.len(), 2, "both list literals survive lowering");
        assert!(
            elements
                .iter()
                .all(|element| *element == Some(&crate::Type::float())),
            "every list literal is a float list after lowering: {elements:?}"
        );
    }

    const GENERIC_MODEL_OVER_A_TENSOR_EFFECT: &str = "static effect Tensor<B> {\n\
        build: fn(int, fn(int) -> float) -> B\n\
        zipWith: fn(B, B, fn(float, float) -> float) -> B\n\
    }\n\
    type Acts<B> = { f: B }\n\
    fn onGpu(work) = {\n\
        handle static Tensor<GpuBuffer<float>> {\n\
            build n f => gpuIota(n) |> gpuMap(f)\n\
            zipWith a b f => gpuZipWith(a, b, f)\n\
        }\n\
        work()\n\
    }\n\
    fn blockLike(dy, acts) = perform Tensor.zipWith(dy, acts.f, fn(g, x) => g)\n\
    fn main() = onGpu(fn() => {\n\
        let ones = perform Tensor.build(4, fn(i) => 1.0)\n\
        blockLike(ones, Acts { f: ones })\n\
    })\n";

    /// A static operation's declared callback type is part of the contract the
    /// model was checked against. The residual must keep it: a kernel written
    /// against `fn(float, float) -> float` inside a generic function is
    /// otherwise re-inferred polymorphic, and the backend lowers its element
    /// slots as raw words — reading every float as an int.
    #[test]
    fn a_discharged_arm_keeps_the_operation_s_declared_callback_types() {
        let parsed = osprey_syntax::parse_program_with_flavor(
            GENERIC_MODEL_OVER_A_TENSOR_EFFECT,
            osprey_syntax::Flavor::Default,
        );
        let lowered = lower_static_checked(&parsed.program);
        assert!(lowered.is_ok(), "lowering failed: {lowered:?}");
        let Ok(lowered) = lowered else { return };
        let types = crate::infer_program(&lowered);
        let lambdas: Vec<String> = types.lambdas.values().map(ToString::to_string).collect();
        assert!(
            lambdas.len() >= 3,
            "every lambda survives lowering: {lambdas:?}"
        );
        assert!(
            !types.lambdas.values().any(crate::has_type_var),
            "every residual lambda is concrete: {lambdas:?}"
        );
    }

    #[test]
    fn nested_record_effect_identity_round_trips_through_source_spelling() {
        let fields = BTreeMap::from([
            (
                "compute".to_owned(),
                crate::Type::fun(
                    vec![crate::Type::int()],
                    crate::Type::list(crate::Type::int()),
                ),
            ),
            ("value".to_owned(), crate::Type::int()),
        ]);
        let argument = crate::Type::Record {
            name: "Payload".to_owned(),
            fields: fields.clone(),
        };
        let spelling = source_type(&argument, &crate::ProgramTypes::default());
        assert_eq!(
            spelling.as_deref(),
            Some("{ compute: fn(int) -> List<int>, value: int }")
        );
        let restored = crate::convert::type_name_to_type(
            spelling.as_deref().unwrap_or_default(),
            &HashMap::new(),
        );
        assert_eq!(
            restored,
            crate::Type::Record {
                name: String::new(),
                fields,
            }
        );
    }
}
