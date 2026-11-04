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
            for field in fields {
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
                let field = index
                    .constructors
                    .get(name)
                    .and_then(|fields| fields.get(position));
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
            for (field, binder) in fields {
                let _ = env.shadowed.insert(binder.clone());
                let projected = value.and_then(|value| project_field(value.clone(), field));
                let projected = match owner {
                    FieldOwner::Named(name) => index.owned_field_value(name, field, projected),
                    FieldOwner::Anonymous | FieldOwner::Unknown => projected,
                };
                if let Some(projected) = projected {
                    let _ = env.values.insert(binder.clone(), projected);
                } else {
                    let _ = env.values.remove(binder);
                }
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
        Pattern::Wildcard | Pattern::Literal(_) => {}
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
        arguments: matched
            .map(|owner| owner.arguments.clone())
            .unwrap_or_default(),
    }
}
