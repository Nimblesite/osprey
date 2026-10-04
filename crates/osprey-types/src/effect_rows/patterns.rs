//! Effect row patterns.
use super::{
    project_element, project_field, project_success_value, CallableEnv, Index, Pattern, Value,
};

pub(super) fn bind_pattern(
    pattern: &Pattern,
    value: Option<&Value>,
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
            for field in fields {
                let _ = env.shadowed.insert(field.clone());
                let projected = value.and_then(|value| {
                    // Elvis binds the same success payload under a reserved
                    // name; losing it would discard the successful arm's
                    // callable effects at the fallback join. [PATTERN-RESULT-DEFAULT]
                    if name == "Success"
                        && (field == "value" || field == osprey_ast::RESULT_DEFAULT_PAYLOAD)
                    {
                        project_success_value(value.clone())
                    } else {
                        project_field(value.clone(), field)
                    }
                });
                if let Some(projected) = projected {
                    let _ = env.values.insert(field.clone(), projected);
                } else {
                    let _ = env.values.remove(field);
                }
            }
            for (position, sub_pattern) in sub_patterns.iter().enumerate() {
                let projected = value.and_then(|value| {
                    index
                        .constructors
                        .get(name)
                        .and_then(|fields| fields.get(position))
                        .and_then(|field| project_field(value.clone(), field))
                });
                bind_pattern(sub_pattern, projected.as_ref(), index, env);
            }
        }
        Pattern::Structural { fields, .. } => {
            for (field, binder) in fields {
                let _ = env.shadowed.insert(binder.clone());
                if let Some(projected) = value.and_then(|value| project_field(value.clone(), field))
                {
                    let _ = env.values.insert(binder.clone(), projected);
                } else {
                    let _ = env.values.remove(binder);
                }
            }
        }
        Pattern::List { elements, rest } => {
            let element = value.and_then(|value| project_element(value.clone()));
            for pattern in elements {
                bind_pattern(pattern, element.as_ref(), index, env);
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
