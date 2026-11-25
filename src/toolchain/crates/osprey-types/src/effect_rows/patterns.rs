//! Effect row patterns.
use super::field_rows::{FieldInstance, FieldOwner};
use super::{
    project_element, project_field, project_success_value, CallableEnv, Index, Pattern, Value,
};

pub(super) fn bind_pattern(
    pattern: &Pattern,
    value: Option<&Value>,
    owner: &FieldOwner,
    index: &Index<'_>,
    env: &mut CallableEnv,
) {
    match pattern {
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => {
            let _ = env.shadowed.insert(name.clone());
            if let Some(value) = value {
                let _ = env.values.insert(name.clone(), value.clone());
            } else {
                let _ = env.values.remove(name);
            }
        }
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } => {
            let constructor = pattern_owner(name, value, owner);
            for field in fields.iter().filter(|_| sub_patterns.is_empty()) {
                let _ = env.shadowed.insert(field.clone());
                let projected = value.and_then(|value| {
                    // `Success` carries its payload in the result slot under
                    // whatever binder the flavor chooses (`value`, `?:`'s
                    // reserved name, or a positional ML binder). Projecting by
                    // that binder's spelling would discard the successful
                    // arm's callable effects at the fallback join.
                    // [PATTERN-RESULT-DEFAULT]
                    if name == "Success" {
                        project_success_value(value.clone())
                    } else {
                        project_field(value.clone(), field)
                    }
                });
                let projected = index.owned_field_value(&constructor, field, projected);
                if let Some(projected) = projected {
                    let _ = env.values.insert(field.clone(), projected);
                } else {
                    let _ = env.values.remove(field);
                }
            }
            for (position, sub_pattern) in sub_patterns.iter().enumerate() {
                let field = if fields.is_empty() {
                    index
                        .constructors
                        .get(name)
                        .and_then(|fields| fields.get(position))
                } else {
                    fields.get(position)
                };
                let projected = value.and_then(|value| {
                    // A positional ML binder is the same payload: `Success`
                    // is a builtin, so the constructor table has no field
                    // name for it. [PATTERN-RESULT-DEFAULT]
                    if name == "Success" {
                        project_success_value(value.clone())
                    } else {
                        field.and_then(|field| project_field(value.clone(), field))
                    }
                });
                let projected = match field {
                    Some(field) if name != "Success" => {
                        index.owned_field_value(&constructor, field, projected)
                    }
                    _ => projected,
                };
                bind_pattern(
                    sub_pattern,
                    projected.as_ref(),
                    &FieldOwner::Unknown,
                    index,
                    env,
                );
            }
        }
        Pattern::Structural { fields, .. } => {
            for (field, slot) in fields {
                let projected = value.and_then(|value| project_field(value.clone(), field));
                let projected = match owner {
                    FieldOwner::Named(name) => index.owned_field_value(name, field, projected),
                    FieldOwner::Anonymous | FieldOwner::Unknown => projected,
                };
                bind_pattern(slot, projected.as_ref(), &FieldOwner::Unknown, index, env);
            }
        }
        Pattern::List { elements, rest } => {
            let element = value.and_then(|value| project_element(value.clone()));
            for pattern in elements {
                bind_pattern(pattern, element.as_ref(), &FieldOwner::Unknown, index, env);
            }
            if let Some(rest) = rest {
                let _ = env.shadowed.insert(rest.clone());
                if let Some(value) = value {
                    let _ = env.values.insert(rest.clone(), value.clone());
                }
            }
        }
        Pattern::Or(alternatives) => bind_alternatives(alternatives, value, owner, index, env),
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

/// A binder of an or-pattern may come from any alternative, so it carries
/// the union of what each alternative gives it: a callable taken from either
/// side keeps both sides' effects ([PATTERN-OR-BINDINGS]).
fn bind_alternatives(
    alternatives: &[Pattern],
    value: Option<&Value>,
    owner: &FieldOwner,
    index: &Index<'_>,
    env: &mut CallableEnv,
) {
    let names = alternatives
        .first()
        .map_or_default(crate::pattern::pattern_binder_names);
    let mut joined: Vec<(String, Option<Value>)> =
        names.into_iter().map(|name| (name, None)).collect();
    for alternative in alternatives {
        bind_pattern(alternative, value, owner, index, env);
        for (name, slot) in &mut joined {
            if let Some(bound) = env.values.get(name.as_str()) {
                super::provenance::merge_optional_value(slot, bound.clone());
            }
        }
    }
    for (name, slot) in joined {
        if let Some(bound) = slot {
            let _ = env.values.insert(name, bound);
        } else {
            let _ = env.values.remove(&name);
        }
    }
}

fn pattern_owner(name: &str, value: Option<&Value>, owner: &FieldOwner) -> FieldInstance {
    let matched = value
        .and_then(|value| value.field_owner.as_ref())
        .or(match owner {
            FieldOwner::Named(owner) => Some(owner),
            FieldOwner::Anonymous | FieldOwner::Unknown => None,
        });
    FieldInstance {
        name: name.to_owned(),
        arguments: matched.map_or_default(|owner| owner.arguments.clone()),
    }
}
