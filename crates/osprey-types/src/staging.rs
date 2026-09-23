//! Validate staged data contracts before any operation or unused arm disappears.

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
    let source_types = crate::check::infer_checked(program)?;
    // Static lowering compares source effect spellings. An inferred
    // `perform Echo.echo(42)` is nevertheless the same operation as the
    // enclosing `handle static Echo<int>`. Use the already checked site
    // instantiations for this backend copy before matching regions; otherwise
    // the rewrite silently leaves a request that the checker proved handled.
    let mut resolved = program.clone();
    for statement in &mut resolved.statements {
        mutate::statement_children_mut(statement, &mut |expression| {
            resolve_effect_mentions(expression, &source_types);
        });
    }
    osprey_ast::stage::lower(&resolved).map_err(stage_errors)
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
