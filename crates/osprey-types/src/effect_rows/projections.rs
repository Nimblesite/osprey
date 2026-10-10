//! Effect row projections.
use super::{
    ordered_values, project_callable, project_element, project_fiber_value, project_field,
    project_success_value, Analyzer, BTreeMap, BTreeSet, CallArguments, Callable, ParameterUse,
    Projection, Requirement, Requirements, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn project_path(&self, mut value: Value, path: &[Projection]) -> Option<Value> {
        for projection in path {
            value = match projection {
                Projection::Field(field) => project_field(value, field),
                Projection::Method(method) => self.project_method(value, *method.clone()),
                Projection::Returned(arguments) => {
                    self.project_call_return(value, *arguments.clone())
                }
                Projection::Handled(answers) => Some(Self::resolve_handled_value(value, answers)),
                Projection::Element => project_element(value),
                Projection::SuccessValue => project_success_value(value),
                Projection::FiberValue => project_fiber_value(value),
            }?;
        }
        Some(value)
    }

    /// A handler runs the supplied computation before returning. Its dynamic
    /// operation answers therefore determine the value produced by a perform
    /// *inside that computation*, even when the callback was constructed in a
    /// different lexical scope. Preserve the same transformation symbolically
    /// on a parameter result until the actual callback value is substituted.
    /// Do not descend into a returned callable: its latent body may run after
    /// this handler has left scope.
    pub(super) fn resolve_handled_value(
        mut value: Value,
        answers: &BTreeMap<Requirement, Value>,
    ) -> Value {
        if answers.is_empty() {
            return value;
        }
        for nested in value.fields.values_mut() {
            *nested = Self::resolve_handled_value(nested.clone(), answers);
        }
        for slot in [
            &mut value.element,
            &mut value.result_payload,
            &mut value.fiber_payload,
        ] {
            if let Some(nested) = slot.take() {
                *slot = Some(Box::new(Self::resolve_handled_value(*nested, answers)));
            }
        }
        if let Some(Callable::Parameter { projection, .. }) = &mut value.callable {
            projection.push(Projection::Handled(answers.clone()));
        }
        let requested = std::mem::take(&mut value.performed);
        for requirement in requested {
            if let Some(answer) = answers.get(&requirement) {
                value.union(answer.clone());
            } else {
                let _ = value.performed.insert(requirement);
            }
        }
        value
    }

    pub(super) fn project_returned(&self, value: Value) -> Option<Value> {
        self.project_call_return(value, CallArguments::default())
    }

    pub(super) fn project_call_return(
        &self,
        mut value: Value,
        arguments: CallArguments,
    ) -> Option<Value> {
        if let Some(Callable::Known(known)) = &mut value.callable {
            let values = ordered_values(&known.parameters, &arguments.positional, &arguments.named);
            return known
                .returned
                .take()
                .map(|returned| self.substitute_value_at(*returned, 0, &values))
                .or_else(|| Some(Value::default()));
        }
        let mut projected = None;
        project_callable(
            value.callable,
            Projection::Returned(Box::new(arguments)),
            &mut projected,
        );
        projected
    }

    pub(super) fn invoke_with_values(
        &self,
        callee: Callable,
        arguments: &[Option<Value>],
    ) -> Summary {
        self.invoke_with_arguments(
            callee,
            CallArguments {
                positional: arguments.to_vec(),
                named: Vec::new(),
            },
        )
    }

    pub(super) fn invoke_with_arguments(
        &self,
        callee: Callable,
        arguments: CallArguments,
    ) -> Summary {
        match callee {
            Callable::Known(known) => {
                let values =
                    ordered_values(&known.parameters, &arguments.positional, &arguments.named);
                self.substitute_summary_at(known.summary.clone(), 0, &values)
            }
            Callable::Unknown => Summary {
                unresolved_dynamic_call: true,
                ..Summary::default()
            },
            Callable::Parameter {
                level,
                index,
                projection,
            } => Summary {
                required: Requirements::new(),
                runtime_builtins: BTreeSet::new(),
                parameter_uses: [ParameterUse {
                    level,
                    index,
                    projection,
                    arguments,
                    excluded: Requirements::new(),
                }]
                .into_iter()
                .collect(),
                unresolved_dynamic_call: false,
            },
        }
    }
}
