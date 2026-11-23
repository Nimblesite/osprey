//! Effect row substitute.
use super::{
    map_parameter_use_values, map_projection_values, shift_value_levels, Analyzer, Callable,
    Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn substitute_summary_at(
        &self,
        mut summary: Summary,
        target_level: usize,
        arguments: &[Option<Value>],
    ) -> Summary {
        let uses = std::mem::take(&mut summary.parameter_uses);
        for mut use_ in uses {
            map_parameter_use_values(&mut use_, |value| {
                *value = self.substitute_value_at(std::mem::take(value), target_level, arguments);
            });
            if use_.level == target_level {
                let argument = arguments.get(use_.index).and_then(Option::as_ref);
                if argument.is_some_and(|value| self.projection_absent(value, &use_.projection)) {
                    continue;
                }
                // The callback is caller provenance, but the arguments this
                // use passes it already belong to the callee. Lift only the
                // callback into the callee's levels: shifting the invoked
                // summary instead re-attributes a returned closure's own
                // parameter to its maker's caller, so `one(f) = fn(w) => f(w)`
                // left `w` undischargeable at the entry. [EFFECTS-PROVENANCE]
                let callback = argument.and_then(|value| {
                    self.project_lifted_path(value, &use_.projection, target_level)?
                        .callable
                });
                if let Some(callback) = callback {
                    let invoked = self.invoke_with_arguments(callback, use_.arguments);
                    summary.union(self.validate_callback_scopes(invoked, &use_.scopes));
                } else if use_.index >= arguments.len() {
                    // No argument occupies this slot yet — the call is still
                    // under-applied, so the parameter stays symbolic for an
                    // enclosing scope to resolve. Keep its level: decrementing
                    // here is what re-attributes it to the wrong binder.
                    let _ = summary.parameter_uses.insert(use_);
                } else {
                    // An argument IS supplied but carries no callable
                    // provenance, so nothing proves what this slot will invoke.
                    // Re-attributing it to the caller's parameter at the same
                    // index — which `target_level.saturating_sub(1)` did, since
                    // a direct call has `target_level == 0` and 0 - 1 saturates
                    // back to 0 — hands the requirement to an unrelated binder,
                    // and any pure argument the caller passes in that slot then
                    // discharges an effect it never supplied. That is fail-OPEN
                    // and it breaks [EFFECTS-STATIC-DISCHARGE]: the program
                    // compiles and aborts at runtime with `unhandled effect`.
                    summary.unresolved_dynamic_call = true;
                }
            } else {
                if use_.level > target_level {
                    use_.level -= 1;
                }
                let _ = summary.parameter_uses.insert(use_);
            }
        }
        summary
    }

    pub(super) fn substitute_value_at(
        &self,
        mut value: Value,
        target_level: usize,
        arguments: &[Option<Value>],
    ) -> Value {
        // Substitute the callee's children before inserting caller provenance.
        // A replacement already belongs to the caller: walking it again can
        // replace its parameter with itself indefinitely for nested records,
        // and can also attribute a callback to the wrong lexical binder.
        for nested in value.container_children_mut() {
            *nested = self.substitute_value_at(std::mem::take(nested), target_level, arguments);
        }
        value.deferred = self.substitute_summary_at(value.deferred, target_level, arguments);
        if let Some(Callable::Parameter { projection, .. }) = &mut value.callable {
            map_projection_values(projection, |value| {
                *value = self.substitute_value_at(std::mem::take(value), target_level, arguments);
            });
        }
        if let Some(callable) = value.callable.take() {
            match callable {
                Callable::Alternatives(choices) => {
                    for choice in choices {
                        let branch = self.substitute_value_at(
                            Value::from_callable(choice),
                            target_level,
                            arguments,
                        );
                        if value.carries_provenance() {
                            value.union(branch);
                        } else {
                            value = branch;
                        }
                    }
                }
                Callable::Parameter {
                    level,
                    index,
                    projection,
                } if level == target_level => {
                    if arguments
                        .get(index)
                        .and_then(Option::as_ref)
                        .is_some_and(|argument| self.projection_absent(argument, &projection))
                    {
                        return value;
                    }
                    if let Some(mut replacement) = arguments
                        .get(index)
                        .and_then(Option::as_ref)
                        .and_then(|argument| self.project_borrowed_path(argument, &projection))
                    {
                        shift_value_levels(&mut replacement, target_level, 0);
                        value.union_replacement(replacement);
                    } else if index >= arguments.len() {
                        // Only an absent argument keeps the original binder.
                        // A supplied opaque value or failed projection must
                        // never be resolved through an unrelated caller slot.
                        value.callable = Some(Callable::Parameter {
                            level,
                            index,
                            projection,
                        });
                    } else {
                        value.callable = Some(Callable::Unknown);
                    }
                }
                Callable::Parameter {
                    mut level,
                    index,
                    projection,
                } => {
                    if level > target_level {
                        level -= 1;
                    }
                    value.callable = Some(Callable::Parameter {
                        level,
                        index,
                        projection,
                    });
                }
                Callable::Known(mut known) => {
                    known.summary =
                        self.substitute_summary_at(known.summary, target_level + 1, arguments);
                    if let Some(returned) = known.returned.as_deref_mut() {
                        *returned = self.substitute_value_at(
                            std::mem::take(returned),
                            target_level + 1,
                            arguments,
                        );
                    }
                    value.callable = Some(Callable::Known(known));
                }
                Callable::Unknown => value.callable = Some(Callable::Unknown),
            }
        }
        value
    }
}

#[test]
fn callback_substitution_only_copies_the_selected_field() {
    use super::{CallArguments, ParameterUse, Projection};
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let receiver = Value {
            fields: (0..128)
                .map(|i| (format!("field{i}"), Value::unknown_value()))
                .collect(),
            ..Value::default()
        };
        let summary = Summary {
            parameter_uses: [ParameterUse {
                level: 0,
                index: 0,
                projection: vec![Projection::Field("field73".into())],
                arguments: CallArguments::default(),
                scopes: Vec::new(),
            }]
            .into_iter()
            .collect(),
            ..Summary::default()
        };
        for level in [0, 1, 3] {
            let mut case = summary.clone();
            case.parameter_uses = std::mem::take(&mut case.parameter_uses)
                .into_iter()
                .map(|mut requirement| {
                    requirement.level = level;
                    requirement
                })
                .collect();
            let arguments = [Some(receiver.clone())];
            let (result, clones) = super::provenance::measured_clones(|| {
                analyzer.substitute_summary_at(case, level, &arguments)
            });
            assert!(result.unresolved_dynamic_call);
            assert!(
                clones <= 1,
                "copied {clones} values to select one callback at level {level}"
            );
        }
    });
}

#[test]
fn invoking_owned_callback_moves_its_deferred_arguments() {
    use super::{CallArguments, KnownCallable, ParameterUse};
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let deferred = ParameterUse {
            level: 1,
            index: 2,
            projection: Vec::new(),
            arguments: CallArguments {
                positional: vec![Some(Value::unknown_value())],
                named: Vec::new(),
            },
            scopes: Vec::new(),
        };
        let callback = KnownCallable {
            parameters: Vec::new(),
            returned: None,
            summary: Summary {
                parameter_uses: [deferred].into_iter().collect(),
                ..Summary::default()
            },
        };
        let (result, clones) = super::provenance::measured_clones(|| {
            analyzer.invoke_with_arguments(
                Callable::Known(Box::new(callback)),
                CallArguments::default(),
            )
        });
        let captured = result
            .parameter_uses
            .first()
            .expect("preserved outer callback");
        assert_eq!((captured.level, captured.index), (0, 2));
        assert_eq!(
            captured.arguments.positional,
            [Some(Value::unknown_value())]
        );
        assert_eq!(clones, 0);
    });
}
