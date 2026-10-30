//! Effect row provenance.
use super::{
    Callable, KnownCallable, ParameterUse, Projection, Summary, Value, MAX_PROVENANCE_DEPTH,
    MAX_PROVENANCE_NODES,
};

#[cfg(test)]
#[path = "sorted_join_tests.rs"]
mod sorted_join_tests;
#[cfg(test)]
pub(super) use sorted_join_tests::{measured_clones, CloneProbe};

pub(super) fn map_alternatives(choices: &mut Vec<Callable>, mut visit: impl FnMut(&mut Value)) {
    for choice in choices.iter_mut() {
        let mut branch = Value::from_callable(std::mem::replace(choice, Callable::Unknown));
        visit(&mut branch);
        *choice = branch.callable.unwrap_or(Callable::Unknown);
    }
    choices.sort();
    choices.dedup();
}

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

/// How much of the provenance depth one call-shaped nesting spends: a value
/// stored inside the arguments of a `Returned`/`Method` step is the result of
/// a symbolic call fed into another, and a recursion through a parameter's
/// function field produces one more such layer per fixed-point sweep. Four
/// layers reach the depth limit, so the sweep count stays bounded.
const CALL_NESTING_STEP: usize = MAX_PROVENANCE_DEPTH / 4;

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
        Callable::Alternatives(choices) => {
            let child_budget = budget / choices.len().max(1);
            bounded_alternatives(
                choices
                    .into_iter()
                    .map(|choice| widen_callable(choice, depth + 1, child_budget, deferred))
                    .collect(),
            )
        }
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
                *value =
                    widen_value_budget(std::mem::take(value), depth + CALL_NESTING_STEP, budget);
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
        Callable::Alternatives(choices) => bounded_alternatives(
            choices
                .into_iter()
                .map(|choice| terminal_callable(choice, deferred))
                .collect(),
        ),
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
            Callable::Alternatives(choices) => {
                map_alternatives(choices, |branch| {
                    shift_value_levels(branch, amount, minimum_level);
                });
            }
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

pub(super) fn merge_callable(slot: &mut Option<Callable>, incoming: Callable) {
    let Some(existing) = slot.take() else {
        *slot = Some(incoming);
        return;
    };
    if existing == incoming {
        *slot = Some(existing);
        return;
    }
    // A common-prefix Deep loses concrete fields and includes unselected
    // siblings. Keep each exact projection until the existing bounds apply.
    *slot = Some(joined_alternatives(existing, incoming));
}

fn joined_alternatives(existing: Callable, incoming: Callable) -> Callable {
    let choices = match (existing, incoming) {
        (Callable::Alternatives(left), Callable::Alternatives(right)) => {
            merge_sorted_choices(left, right)
        }
        (Callable::Alternatives(choices), choice) | (choice, Callable::Alternatives(choices)) => {
            insert_sorted_choice(choices, choice)
        }
        (left, right) => return bounded_alternatives(vec![left, right]),
    };
    bound_canonical_choices(choices)
}

// Producers and in-place transforms canonicalize Alternatives. Preserve that
// order without re-sorting every growing prefix; overflow still runs per join.
fn insert_sorted_choice(mut choices: Vec<Callable>, incoming: Callable) -> Vec<Callable> {
    if let Err(position) = choices.binary_search_by(|choice| compare_choices(choice, &incoming)) {
        choices.insert(position, incoming);
    }
    choices
}

fn merge_sorted_choices(left: Vec<Callable>, right: Vec<Callable>) -> Vec<Callable> {
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    std::iter::from_fn(|| match (left.peek(), right.peek()) {
        (Some(a), Some(b)) => match compare_choices(a, b) {
            std::cmp::Ordering::Less => left.next(),
            std::cmp::Ordering::Equal => {
                let _ = right.next();
                left.next()
            }
            std::cmp::Ordering::Greater => right.next(),
        },
        (Some(_), None) => left.next(),
        (None, _) => right.next(),
    })
    .collect()
}

fn bounded_alternatives(mut choices: Vec<Callable>) -> Callable {
    choices.sort_by(compare_choices);
    choices.dedup();
    bound_canonical_choices(choices)
}

fn bound_canonical_choices(choices: Vec<Callable>) -> Callable {
    if choices.len() <= MAX_PROVENANCE_NODES {
        return Callable::Alternatives(choices);
    }
    let mut known = Summary::default();
    for choice in choices {
        collect_known_effects(&choice, &mut known);
    }
    Callable::Alternatives(vec![known_effects_callable(known), Callable::Unknown])
}

fn known_effects_callable(summary: Summary) -> Callable {
    Callable::Known(Box::new(KnownCallable {
        parameters: Vec::new(),
        summary,
        returned: Some(Box::new(Value::unknown_value())),
    }))
}

fn collect_known_effects(callable: &Callable, summary: &mut Summary) {
    match callable {
        Callable::Known(known) => {
            summary
                .required
                .extend(known.summary.required.iter().cloned());
            summary
                .runtime_builtins
                .extend(known.summary.runtime_builtins.iter().cloned());
        }
        Callable::Alternatives(choices) => {
            for choice in choices {
                collect_known_effects(choice, summary);
            }
        }
        Callable::Parameter { .. } | Callable::Unknown => {}
    }
}

fn compare_choices(left: &Callable, right: &Callable) -> std::cmp::Ordering {
    #[cfg(test)]
    sorted_join_tests::record_comparison();
    left.cmp(right)
}
