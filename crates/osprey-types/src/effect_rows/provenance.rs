//! Effect row provenance.
use super::{
    BTreeSet, CallArguments, Callable, KnownCallable, ParameterUse, Projection, Requirements,
    Summary, Value, MAX_PROVENANCE_DEPTH, MAX_PROVENANCE_NODES,
};

pub(super) fn map_projection_values(
    projection: &mut [Projection],
    mut visit: impl FnMut(&mut Value),
) {
    for part in projection {
        let (positional, named) = match part {
            Projection::Method(method) => {
                visit(&mut method.fallback);
                (&mut method.arguments, &mut method.named)
            }
            Projection::Returned(arguments) => (&mut arguments.positional, &mut arguments.named),
            Projection::Handled(answers) => {
                for answer in answers.values_mut() {
                    visit(answer);
                }
                continue;
            }
            _ => continue,
        };
        for value in positional
            .iter_mut()
            .chain(named.iter_mut().map(|(_, value)| value))
            .flatten()
        {
            visit(value);
        }
    }
}

pub(super) fn map_parameter_use_values(use_: &mut ParameterUse, mut visit: impl FnMut(&mut Value)) {
    map_projection_values(&mut use_.projection, &mut visit);
    for value in use_
        .arguments
        .positional
        .iter_mut()
        .chain(use_.arguments.named.iter_mut().map(|(_, value)| value))
        .flatten()
    {
        visit(value);
    }
}

pub(super) fn ordered_values(
    parameters: &[String],
    positional: &[Option<Value>],
    named: &[(String, Option<Value>)],
) -> Vec<Option<Value>> {
    let mut values = positional.to_vec();
    values.resize(values.len().max(parameters.len()), None);
    for (name, value) in named {
        if let Some(index) = parameters.iter().position(|parameter| parameter == name) {
            if let Some(slot) = values.get_mut(index) {
                slot.clone_from(value);
            }
        }
    }
    values
}

pub(super) fn merge_value(slot: &mut Value, incoming: Value) {
    slot.union(incoming);
}

pub(super) fn merge_optional_value(slot: &mut Option<Value>, incoming: Value) {
    if let Some(value) = slot {
        value.union(incoming);
    } else {
        *slot = Some(incoming);
    }
}

pub(super) fn merge_boxed_value(slot: &mut Option<Box<Value>>, incoming: Value) {
    if let Some(value) = slot {
        value.union(incoming);
    } else {
        *slot = Some(Box::new(incoming));
    }
}

pub(super) fn widen_value(value: Value, depth: usize) -> Value {
    widen_value_budget(value, depth, MAX_PROVENANCE_NODES)
}

pub(super) fn widen_value_budget(mut value: Value, depth: usize, budget: usize) -> Value {
    let children = value.child_count();
    if depth >= MAX_PROVENANCE_DEPTH || budget <= children {
        return terminal_value(value);
    }
    let child_budget = (budget - 1) / children.max(1);
    value.deferred.widen_at_budget(depth, child_budget);
    value.callable = value
        .callable
        .take()
        .map(|callable| widen_callable(callable, depth, child_budget, &mut value.deferred));
    value.fields = value
        .fields
        .into_iter()
        .map(|(name, nested)| (name, widen_value_budget(nested, depth + 1, child_budget)))
        .collect();
    widen_payload(&mut value.element, depth, child_budget);
    widen_payload(&mut value.result_payload, depth, child_budget);
    widen_payload(&mut value.fiber_payload, depth, child_budget);
    value
}

pub(super) fn widen_payload(payload: &mut Option<Box<Value>>, depth: usize, budget: usize) {
    if let Some(nested) = payload.take() {
        *payload = Some(Box::new(widen_value_budget(*nested, depth + 1, budget)));
    }
}

pub(super) fn widen_callable(
    callable: Callable,
    depth: usize,
    budget: usize,
    deferred: &mut Summary,
) -> Callable {
    match callable {
        Callable::Known(mut known) => {
            known.summary.widen_at_budget(depth, budget);
            widen_payload(&mut known.returned, depth, budget);
            Callable::Known(known)
        }
        Callable::Parameter {
            level,
            index,
            mut projection,
        } if level < MAX_PROVENANCE_DEPTH && projection.len() < MAX_PROVENANCE_DEPTH => {
            map_projection_values(&mut projection, |value| {
                *value = widen_value_budget(value.clone(), depth + 1, budget);
            });
            Callable::Parameter {
                level,
                index,
                projection,
            }
        }
        Callable::Parameter { .. } | Callable::Unknown => {
            deferred.unresolved_dynamic_call = true;
            Callable::Unknown
        }
    }
}

pub(super) fn terminal_value(mut value: Value) -> Value {
    value.deferred.widen_at(MAX_PROVENANCE_DEPTH);
    value.fields = value
        .fields
        .into_keys()
        .map(|name| (name, Value::unknown_callable()))
        .collect();
    value.element = value.element.map(|_| Box::new(Value::unknown_callable()));
    value.result_payload = value
        .result_payload
        .map(|_| Box::new(Value::unknown_callable()));
    value.fiber_payload = value
        .fiber_payload
        .map(|_| Box::new(Value::unknown_callable()));
    value.callable = value
        .callable
        .map(|callable| terminal_callable(callable, &mut value.deferred));
    value
}

pub(super) fn terminal_callable(callable: Callable, deferred: &mut Summary) -> Callable {
    match callable {
        Callable::Known(mut known) => {
            known.summary.widen_at(MAX_PROVENANCE_DEPTH);
            known.returned = known.returned.map(|_| Box::new(Value::unknown_callable()));
            Callable::Known(known)
        }
        Callable::Parameter { ref projection, .. }
            if projection.iter().any(|part| {
                matches!(
                    part,
                    Projection::Method(_) | Projection::Returned(_) | Projection::Handled(_)
                )
            }) =>
        {
            deferred.unresolved_dynamic_call = true;
            Callable::Unknown
        }
        other => other,
    }
}

pub(super) fn shift_summary_from(summary: &mut Summary, amount: usize, minimum_level: usize) {
    summary.parameter_uses = summary
        .parameter_uses
        .iter()
        .cloned()
        .map(|mut use_| {
            if use_.level >= minimum_level {
                use_.level = use_.level.saturating_add(amount);
            }
            map_parameter_use_values(&mut use_, |value| {
                shift_value_levels(value, amount, minimum_level);
            });
            use_
        })
        .collect();
}

pub(super) fn shift_summary_levels(summary: &mut Summary, amount: usize) {
    shift_summary_from(summary, amount, 0);
}

pub(super) fn shift_value_levels(value: &mut Value, amount: usize, minimum_level: usize) {
    if let Some(callable) = &mut value.callable {
        match callable {
            Callable::Parameter {
                level, projection, ..
            } => {
                if *level >= minimum_level {
                    *level = level.saturating_add(amount);
                }
                map_projection_values(projection, |value| {
                    shift_value_levels(value, amount, minimum_level);
                });
            }
            Callable::Known(known) => {
                shift_summary_from(&mut known.summary, amount, minimum_level + 1);
                if let Some(returned) = &mut known.returned {
                    shift_value_levels(returned, amount, minimum_level + 1);
                }
            }
            Callable::Unknown => {}
        }
    }
    for nested in value.container_children_mut() {
        shift_value_levels(nested, amount, minimum_level);
    }
    shift_summary_from(&mut value.deferred, amount, minimum_level);
}

pub(super) fn callable_summary(callable: &Callable) -> Summary {
    match callable {
        Callable::Known(known) => known.summary.clone(),
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
                level: *level,
                index: *index,
                projection: projection.clone(),
                arguments: CallArguments::default(),
                scopes: Vec::new(),
            }]
            .into_iter()
            .collect(),
            unresolved_dynamic_call: false,
        },
    }
}

pub(super) fn merge_callable(slot: &mut Option<Callable>, incoming: Callable) {
    let Some(existing) = slot.take() else {
        *slot = Some(incoming);
        return;
    };
    if existing == incoming {
        *slot = Some(existing);
        return;
    }
    if let Some(merged) = merged_projection(&existing, &incoming) {
        *slot = Some(merged);
        return;
    }
    *slot = Some(synthetic_join(&existing, &incoming));
}

/// The synthetic callable of a join neither side of which can summarise the
/// other: it carries both sides' summaries and returned values, so invoking
/// it still accounts for every alternative. Implements [EFFECTS-PROVENANCE].
fn synthetic_join(existing: &Callable, incoming: &Callable) -> Callable {
    let mut summary = callable_summary(existing);
    summary.union(callable_summary(incoming));
    let (parameters, returned) = joined_shapes(existing, incoming);
    Callable::Known(Box::new(KnownCallable {
        parameters,
        summary,
        returned: returned.map(Box::new),
    }))
}

/// The shared parameters and merged returned values of the joined callables.
fn joined_shapes(existing: &Callable, incoming: &Callable) -> (Vec<String>, Option<Value>) {
    let mut parameters = Vec::new();
    let mut returned = None;
    for callable in [existing, incoming] {
        if let Callable::Known(known) = callable {
            if parameters.is_empty() || parameters == known.parameters {
                parameters.clone_from(&known.parameters);
            }
            if let Some(value) = known.returned.as_deref().cloned() {
                merge_optional_value(&mut returned, value);
            }
        }
    }
    (parameters, returned)
}

/// Both sides project the same parameter of the same scope: alternatives over
/// one aggregate, not independent callables.
fn same_parameter<'a>(
    existing: &'a Callable,
    incoming: &'a Callable,
) -> Option<(usize, usize, &'a [Projection], &'a [Projection])> {
    match (existing, incoming) {
        (
            Callable::Parameter {
                level,
                index,
                projection,
            },
            Callable::Parameter {
                level: other_level,
                index: other_index,
                projection: other,
            },
        ) if level == other_level && index == other_index => {
            Some((*level, *index, projection, other))
        }
        _ => None,
    }
}

/// Two projections of the SAME parameter are control-flow alternatives over
/// one aggregate (a union's arms, a recursion's levels). Summarise them as
/// their common prefix plus one `Deep` step: every callable either alternative
/// could reach is stored below that prefix, so the merge stays sound and
/// finite where tracking both paths would grow one level per fixed-point
/// iteration. Implements [EFFECTS-PROVENANCE].
fn merged_projection(existing: &Callable, incoming: &Callable) -> Option<Callable> {
    let (level, index, left, right) = same_parameter(existing, incoming)?;
    let prefix = common_prefix(left, right);
    (tails_deep_summarizable(left, prefix) && tails_deep_summarizable(right, prefix)).then(|| {
        Callable::Parameter {
            level,
            index,
            projection: common_deep_summary(left, prefix),
        }
    })
}

/// A tail a `Deep` step can summarise: structural steps traverse stored data,
/// and `Deep` itself is already the summary. A call-shaped step (`Method`,
/// `Returned`, `Handled`) produces a callable no storage path reaches, so
/// summarising across one would drop its provenance.
fn tails_deep_summarizable(path: &[Projection], prefix: usize) -> bool {
    path.iter()
        .skip(prefix)
        .all(|part| part.is_structural() || matches!(part, Projection::Deep))
}

/// The length of the common prefix of two paths.
fn common_prefix(left: &[Projection], right: &[Projection]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

/// The common structural prefix of two paths followed by one `Deep` step: the
/// finite summary of every callable stored below it. Implements
/// [EFFECTS-PROVENANCE].
fn common_deep_summary(left: &[Projection], prefix: usize) -> Vec<Projection> {
    left.iter()
        .take(prefix)
        .cloned()
        .chain([Projection::Deep])
        .collect()
}
