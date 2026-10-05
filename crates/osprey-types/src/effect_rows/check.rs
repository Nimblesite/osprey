//! Effect row check.
use super::reports::CheckedEffects;
use super::{
    clear_verdicts, converge, entry_errors, file_scope_env, full_application, requirement_name,
    specialize_argument, validate_arithmetic_initializers, validate_handler_arms,
    validate_statement_handlers, Analyzer, BTreeSet, CallableEnv, Index, Instances, Program,
    Summary, TypeError,
};

/// Check inferred rows and entry discharge. Implements
/// [EFFECTS-STATIC-DISCHARGE].
#[expect(
    clippy::too_many_lines,
    reason = "the fixed point, contract validation, and entry proof form one ordered checker pass"
)]
pub(crate) fn check(program: &Program, instances: &Instances, exports: &[&str]) -> CheckedEffects {
    let mut index = Index::collect(program);
    for function in &mut index.functions {
        let entries = function.position.and_then(|position| {
            instances
                .declared_rows
                .get(&(position.line, position.column))
        });
        if let Some(entries) = entries {
            for (declared, arguments) in function.declared_effects.iter_mut().zip(entries) {
                if let Some(arguments) = arguments {
                    declared.arguments = Some(arguments.clone());
                }
            }
        }
    }
    let axis = crate::multiplicity::OperationAxis::collect(program);
    let mut errors = Vec::new();
    let exported: Vec<_> = exports
        .iter()
        .filter_map(|name| {
            if let Some(id) = index.functions.iter().position(|f| f.qualified == *name) {
                Some((id, *name))
            } else {
                errors.push(TypeError::new(format!(
                    "library export `{name}` is not a defined function"
                )));
                None
            }
        })
        .collect();
    for function in &index.functions {
        if function
            .effect_tail
            .as_ref()
            .is_some_and(|tail| !tail.chars().next().is_some_and(char::is_lowercase))
        {
            errors.push(
                TypeError::new(format!(
                    "function `{}` has an invalid effect row variable; use a lowercase name",
                    function.qualified
                ))
                .with_pos(function.position),
            );
        }
        let unknown: BTreeSet<_> = function
            .declared_effects
            .iter()
            .filter(|effect| !index.effects.contains_key(&effect.name))
            .map(|effect| effect.name.clone())
            .collect();
        if !unknown.is_empty() {
            errors.push(
                TypeError::new(format!(
                    "function `{}` declares unknown effects: {}",
                    function.qualified,
                    unknown.into_iter().collect::<Vec<_>>().join(", ")
                ))
                .with_pos(function.position),
            );
        }
    }
    // Without declarations or written row contracts, there is no algebraic
    // row to solve. A written `![]` still needs analysis: runtime builtins and
    // opaque callbacks cannot be certified pure by this fast path.
    if index.effects.is_empty()
        && index
            .functions
            .iter()
            .all(|function| !function.effect_row_present)
    {
        return CheckedEffects {
            errors,
            ..CheckedEffects::default()
        };
    }
    let mut rows = vec![Summary::default(); index.functions.len()];
    let mut returns = vec![None; index.functions.len()];

    // TWO sweeps, not one. `unresolved_dynamic_call` is a property of the
    // CONVERGED rows, but a returned closure carries its OWN summary inside
    // `returns[id]`, and that summary is read back on the next iteration. A
    // verdict raised on the first iteration — when a self-call still reads
    // `returns[id] == None`, resolves to no callable, and fails closed — is
    // therefore laundered through the stored closure and re-derives itself
    // forever, condemning every recursive curried function. Sweep once for
    // structure, erase those transient verdicts, then sweep again from
    // converged provenance. Nothing genuine is lost: every verdict is
    // recomputed from the body, from a stored `Callable::Unknown`, or from a
    // callee whose own verdict the second sweep raises first and propagates.
    converge(
        &index,
        instances,
        &program.statements,
        &mut rows,
        &mut returns,
    );
    clear_verdicts(&mut rows, &mut returns);
    converge(
        &index,
        instances,
        &program.statements,
        &mut rows,
        &mut returns,
    );

    let analyzer = Analyzer {
        index: &index,
        rows: &rows,
        returns: &returns,
        instances,
        file_scope: file_scope_env(&index, instances, &program.statements, &rows, &returns),
    };
    // An annotation is a row contract/instantiation hint, never a handler.
    // Inferred operations outside its named effects are therefore an error.
    for ((function, own), returned) in index.functions.iter().zip(&rows).zip(&returns) {
        if function.effect_row_present {
            let row = &full_application(function.body, own, returned.as_ref());
            let undeclared: BTreeSet<_> = row
                .required
                .iter()
                .filter(|requirement| {
                    !function.declared_effects.iter().any(|declared| {
                        declared.name == requirement.effect
                            && declared.arguments.as_ref().is_none_or(|arguments| {
                                let bindings = instances.binders.get(&function.name);
                                arguments
                                    .iter()
                                    .map(|arg| {
                                        bindings.map_or_else(
                                            || arg.clone(),
                                            |b| specialize_argument(arg, b),
                                        )
                                    })
                                    .eq(requirement.arguments.iter().cloned())
                            })
                    })
                })
                .map(requirement_name)
                .collect();
            if !undeclared.is_empty() {
                errors.push(
                    TypeError::new(format!(
                        "function `{}` performs effects outside its declared row: {}",
                        function.qualified,
                        undeclared.into_iter().collect::<Vec<_>>().join(", ")
                    ))
                    .with_pos(function.position),
                );
            }
            // Host operations are tracked separately from user-declared
            // effects. An explicitly empty row cannot turn `print` (or an
            // alias of it) into pure code merely because there was no
            // `perform` expression in the body.
            if function.declared_effects.is_empty() && !row.runtime_builtins.is_empty() {
                errors.push(
                    TypeError::new(format!(
                        "function `{}` uses runtime builtins outside its declared row: {}",
                        function.qualified,
                        row.runtime_builtins
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .with_pos(function.position),
                );
            }
            if (function.effect_tail.is_none() && !row.parameter_uses.is_empty())
                || row.unresolved_dynamic_call
            {
                errors.push(
                    TypeError::new(format!(
                        "function `{}` calls a function with unproven effects outside its declared row",
                        function.qualified
                    ))
                    .with_pos(function.position),
                );
            }
        }
        validate_handler_arms(
            &analyzer,
            &axis,
            function.body,
            &function.scope,
            &analyzer.scoped_env(&function.parameters),
            &mut errors,
        );
    }
    let mut top_level_env = CallableEnv::default();
    validate_statement_handlers(
        &analyzer,
        &axis,
        &program.statements,
        &[],
        &mut top_level_env,
        &mut errors,
    );

    validate_arithmetic_initializers(&analyzer, &program.statements, &mut errors);

    // The entry is `main` when the program declares one, otherwise the
    // top-level let/assignment/expression sequence. Either way the file-scope
    // initializers run before it and are part of it: a `perform` in a top-level
    // `let` needs a handler exactly as one written inside `main` does.
    let mut entry = {
        let mut env = CallableEnv::default();
        analyzer.statements(&program.statements, &[], &mut env)
    };
    if let Some(main) = index
        .functions
        .iter()
        .position(|function| function.scope.is_empty() && function.name == "main")
    {
        entry.union(rows.get(main).cloned().unwrap_or_default());
    }
    errors.extend(entry_errors(
        &entry,
        "program entry",
        !instances.non_callable_call_error,
    ));
    for (id, name) in exported {
        if let Some(row) = rows.get(id) {
            errors.extend(entry_errors(
                row,
                &format!("library export `{name}`"),
                !instances.non_callable_call_error,
            ));
        }
    }
    CheckedEffects {
        errors,
        functions: super::reports::collect(&index, instances, &rows, &returns),
    }
}
