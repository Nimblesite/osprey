//! Effect row projections.
use super::{
    ordered_values, project_callable, project_deep, project_element, project_fiber_value,
    project_success_value, Analyzer, BTreeMap, BTreeSet, CallArguments, Callable, ParameterUse,
    Projection, Requirement, Requirements, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn projection_absent(&self, value: &Value, path: &[Projection]) -> bool {
        match path.split_first() {
            Some((projection, remaining)) if !value.unknown_structure => {
                value.proves_projection_absent(projection)
                    || match borrowed_structural_child(value, projection) {
                        ProjectionChild::Borrowed(child) => {
                            self.projection_absent(child, remaining)
                        }
                        ProjectionChild::Missing => false,
                        ProjectionChild::Owned => self.owned_projection_absent(value.clone(), path),
                    }
            }
            _ => false,
        }
    }

    fn owned_projection_absent(&self, mut value: Value, path: &[Projection]) -> bool {
        for projection in path {
            if value.unknown_structure {
                return false;
            }
            if value.proves_projection_absent(projection) {
                return true;
            }
            let Some(next) = self.project_path(value, std::slice::from_ref(projection)) else {
                return false;
            };
            value = next;
        }
        false
    }

    pub(super) fn project_borrowed_path(
        &self,
        value: &Value,
        path: &[Projection],
    ) -> Option<Value> {
        let mut selected = value;
        let mut remaining = path;
        while let Some((projection, rest)) = remaining.split_first() {
            match borrowed_structural_child(selected, projection) {
                ProjectionChild::Borrowed(child) => selected = child,
                ProjectionChild::Missing => return None,
                ProjectionChild::Owned => {
                    return self.project_borrowed_fallback(selected, remaining);
                }
            }
            remaining = rest;
        }
        Some(selected.clone())
    }

    fn project_borrowed_fallback(&self, value: &Value, path: &[Projection]) -> Option<Value> {
        match path.split_first() {
            Some((Projection::Field(field), remaining)) => self
                .project_borrowed_field(value, field)
                .and_then(|projected| self.project_path(projected, remaining)),
            Some((Projection::Returned(_), _)) => value
                .callable
                .clone()
                .and_then(|callable| self.project_path(Value::from_callable(callable), path)),
            _ => self.project_path(value.clone(), path),
        }
    }

    pub(super) fn project_path(&self, mut value: Value, path: &[Projection]) -> Option<Value> {
        for projection in path {
            value = match projection {
                Projection::Field(field) => self.project_owned_field(value, field),
                Projection::Method(method) => self.project_method(value, *method.clone()),
                Projection::Returned(arguments) => {
                    self.project_call_return(value, *arguments.clone())
                }
                Projection::Handled(answers) => Some(Self::resolve_handled_value(value, answers)),
                Projection::Element => project_element(value),
                Projection::SuccessValue => project_success_value(value),
                Projection::FiberValue => project_fiber_value(value),
                Projection::Deep => Some(project_deep(&value)),
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
        Self::resolve_handled_alternatives(&mut value, answers);
        if answers.is_empty() {
            return value;
        }
        for nested in value.container_children_mut() {
            *nested = Self::resolve_handled_value(std::mem::take(nested), answers);
        }
        if let Some(Callable::Parameter { projection, .. }) = &mut value.callable {
            projection.push(Projection::Handled(answers.clone()));
        }
        Self::resolve_performed_answers(value, answers)
    }

    fn resolve_handled_alternatives(value: &mut Value, answers: &BTreeMap<Requirement, Value>) {
        if let Some(Callable::Alternatives(choices)) = &mut value.callable {
            super::provenance::map_alternatives(choices, |branch| {
                *branch = Self::resolve_handled_value(std::mem::take(branch), answers);
            });
        }
    }

    fn resolve_performed_answers(
        mut value: Value,
        answers: &BTreeMap<Requirement, Value>,
    ) -> Value {
        for requirement in std::mem::take(&mut value.performed) {
            match answers.get(&requirement) {
                Some(answer) => value.union_replacement(answer.clone()),
                None => {
                    let _ = value.performed.insert(requirement);
                }
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
        match value.callable.take() {
            Some(Callable::Alternatives(choices)) => {
                self.project_return_choices(choices, &arguments)
            }
            Some(Callable::Known(known)) => self.project_known_return(*known, &arguments),
            callable => Self::project_other_return(callable, arguments),
        }
    }

    fn project_other_return(callable: Option<Callable>, arguments: CallArguments) -> Option<Value> {
        let mut projected = None;
        project_callable(
            callable,
            Projection::Returned(Box::new(arguments)),
            &mut projected,
        );
        projected
    }

    fn project_return_choices(
        &self,
        choices: Vec<Callable>,
        arguments: &CallArguments,
    ) -> Option<Value> {
        let mut returned = None;
        for choice in choices {
            super::merge_optional_value(
                &mut returned,
                self.project_call_return(Value::from_callable(choice), arguments.clone())
                    .unwrap_or_else(Value::unknown_value),
            );
        }
        returned
    }

    fn project_known_return(
        &self,
        mut known: super::KnownCallable,
        arguments: &CallArguments,
    ) -> Option<Value> {
        let values = ordered_values(&known.parameters, &arguments.positional, &arguments.named);
        known
            .returned
            .take()
            .map(|returned| self.substitute_value_at(*returned, 0, &values))
            .or_else(|| Some(Value::unknown_value()))
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
            Callable::Alternatives(choices) => {
                let mut summary = Summary::default();
                for choice in choices {
                    summary.union(self.invoke_with_arguments(choice, arguments.clone()));
                }
                summary
            }
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
                    scopes: Vec::new(),
                }]
                .into_iter()
                .collect(),
                unresolved_dynamic_call: false,
            },
        }
    }
}

enum ProjectionChild<'a> {
    Borrowed(&'a Value),
    Missing,
    Owned,
}

impl<'a> From<Option<&'a Value>> for ProjectionChild<'a> {
    fn from(value: Option<&'a Value>) -> Self {
        match value {
            Some(value) => Self::Borrowed(value),
            None => Self::Missing,
        }
    }
}

fn borrowed_structural_child<'a>(value: &'a Value, projection: &Projection) -> ProjectionChild<'a> {
    if value.callable.is_some() || value.unknown_structure {
        return ProjectionChild::Owned;
    }
    match projection {
        Projection::Field(field) if value.field_owner.is_none() => value.fields.get(field).into(),
        Projection::Element => value.element.as_deref().into(),
        Projection::SuccessValue => borrowed_success_child(value),
        Projection::FiberValue => value.fiber_payload.as_deref().into(),
        _ => ProjectionChild::Owned,
    }
}

fn borrowed_success_child(value: &Value) -> ProjectionChild<'_> {
    match (value.result_payload.as_deref(), value.fields.get("value")) {
        (Some(_), Some(_)) => ProjectionChild::Owned,
        (payload, field) => payload.or(field).into(),
    }
}

#[cfg(test)]
#[test]
fn owned_return_choices_do_not_clone_their_payloads() {
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let value = Value::from_callable(Callable::Alternatives(
            (0..2).map(super::model_values::returning_unknown).collect(),
        ));
        let arguments = CallArguments {
            positional: vec![None],
            named: Vec::new(),
        };
        let (actual, clones) =
            super::provenance::measured_clones(|| analyzer.project_call_return(value, arguments));
        assert_eq!(actual, Some(Value::unknown_value()));
        assert_eq!(clones, 0, "owned choices clone");
    });
}

#[cfg(test)]
#[test]
fn borrowed_structural_projection_clones_only_the_selected_subtree() {
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let value = Value {
            fields: (0..128)
                .map(|i| (format!("sibling{i}"), Value::unknown_value()))
                .chain([(String::from("selected"), Value::unknown_value())])
                .collect(),
            ..Value::default()
        };
        let projection = Projection::Field("selected".to_owned());
        for value in [value.clone(), projection_metadata(value, 10)] {
            assert_projection_budget(&analyzer, &value, projection.clone(), 1);
        }
    });
}

#[cfg(test)]
fn projection_steps() -> Vec<Projection> {
    vec![
        Projection::Field("selected".to_owned()),
        Projection::Field("missing".to_owned()),
        Projection::Element,
        Projection::SuccessValue,
        Projection::FiberValue,
        Projection::Deep,
        Projection::Returned(Box::default()),
        Projection::Handled(BTreeMap::new()),
        Projection::Method(Box::new(super::MethodProjection {
            field: "selected".to_owned(),
            arguments: Vec::new(),
            named: Vec::new(),
            fallback: Value::unknown_value(),
        })),
    ]
}

#[cfg(test)]
fn projection_container(leaf: &Value) -> Value {
    Value {
        fields: [
            ("selected".to_owned(), leaf.clone()),
            ("value".to_owned(), leaf.clone()),
        ]
        .into_iter()
        .collect(),
        element: Some(Box::new(leaf.clone())),
        result_payload: Some(Box::new(leaf.clone())),
        fiber_payload: Some(Box::new(leaf.clone())),
        ..Value::default()
    }
}

#[cfg(test)]
fn projection_test_callable(mode: usize) -> Option<Callable> {
    match mode % 5 {
        0 => None,
        1 => Some(Callable::Unknown),
        2 => Value::parameter(1, 2).callable,
        3 => Some(super::model_values::returning_unknown(0)),
        _ => Some(Callable::Alternatives(vec![
            Callable::Unknown,
            super::model_values::returning_unknown(0),
        ])),
    }
}

#[cfg(test)]
fn projection_metadata(mut value: Value, mode: usize) -> Value {
    value.callable = projection_test_callable(mode);
    value.unknown_structure = mode & 1 != 0;
    value.empty_case = Some(super::EmptyCase::Error);
    value.field_owner = (mode & 2 != 0).then(|| super::field_rows::FieldInstance {
        name: "Control".to_owned(),
        arguments: Vec::new(),
    });
    value.field_names = Some(["selected".to_owned()].into_iter().collect());
    let _ = value
        .performed
        .insert(Requirement::new("Read", "get", Vec::new()));
    let _ = value.channel_sites.insert(17);
    value.deferred = projection_deferred();
    value
}

#[cfg(test)]
#[test]
fn borrowed_projection_matches_owned_projection_for_mixed_paths_and_metadata() {
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let paths = projection_test_paths();
        for path in paths {
            for mode in 0..20 {
                let leaf = projection_metadata(Value::default(), mode);
                let value =
                    projection_metadata(projection_container(&projection_container(&leaf)), mode);
                assert_eq!(
                    analyzer.project_borrowed_path(&value, &path),
                    analyzer.project_path(value.clone(), &path),
                    "{mode}: {path:?}"
                );
            }
        }
    });
}

#[cfg(test)]
fn projection_test_paths() -> Vec<Vec<Projection>> {
    let steps = projection_steps();
    std::iter::once(Vec::new())
        .chain(steps.iter().cloned().map(|step| vec![step]))
        .chain(steps.iter().flat_map(|first| {
            steps
                .iter()
                .map(move |second| vec![first.clone(), second.clone()])
        }))
        .collect()
}

#[cfg(test)]
#[test]
fn borrowed_payload_and_missing_projection_clone_budgets() {
    super::identity_tests::with_empty_analyzer(|analyzer| {
        let mut value = projection_container(&Value::unknown_value());
        let _ = value.fields.remove("value");
        for step in [
            Projection::Element,
            Projection::SuccessValue,
            Projection::FiberValue,
        ] {
            assert_projection_budget(&analyzer, &value, step, 1);
        }
        assert_missing_projection_budgets(&analyzer);
        value.result_payload = None;
        let _ = value
            .fields
            .insert("value".to_owned(), Value::unknown_value());
        assert_projection_budget(&analyzer, &value, Projection::SuccessValue, 1);
    });
}

#[cfg(test)]
fn assert_projection_budget(
    analyzer: &Analyzer<'_>,
    value: &Value,
    step: Projection,
    budget: usize,
) {
    let path = [step];
    let expected = analyzer.project_path(value.clone(), &path);
    let (actual, clones) =
        super::provenance::measured_clones(|| analyzer.project_borrowed_path(value, &path));
    assert_eq!(actual, expected);
    assert_eq!(clones, budget, "{path:?}");
}

#[cfg(test)]
fn assert_missing_projection_budgets(analyzer: &Analyzer<'_>) {
    for step in projection_steps()
        .into_iter()
        .filter(Projection::is_structural)
    {
        assert_projection_budget(analyzer, &Value::default(), step, 0);
    }
}

#[cfg(test)]
fn projection_deferred() -> Summary {
    Summary {
        parameter_uses: [projection_parameter_use()].into_iter().collect(),
        unresolved_dynamic_call: true,
        ..super::model_values::test_summary_metadata()
    }
}

#[cfg(test)]
fn projection_parameter_use() -> ParameterUse {
    ParameterUse {
        level: 1,
        index: 2,
        projection: vec![Projection::Field("selected".to_owned())],
        arguments: CallArguments::default(),
        scopes: Vec::new(),
    }
}
