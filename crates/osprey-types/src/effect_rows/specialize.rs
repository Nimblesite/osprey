//! Effect row specialize.
use super::{
    map_parameter_use_values, map_projection_values, Callable, HashMap, Projection, Requirements,
    Summary, Value,
};

/// Substitute complete type-name tokens, so `t3` never rewrites `t30`.
pub(super) fn specialize_argument(argument: &str, bindings: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut token = String::new();
    for character in argument.chars().chain(std::iter::once('\0')) {
        if character.is_alphanumeric() || character == '_' {
            token.push(character);
        } else {
            result.push_str(bindings.get(&token).unwrap_or(&token));
            token.clear();
            if character != '\0' {
                result.push(character);
            }
        }
    }
    result
}

pub(super) fn specialize_requirements(
    requirements: &mut Requirements,
    bindings: &HashMap<String, String>,
) {
    *requirements = std::mem::take(requirements)
        .into_iter()
        .map(|mut requirement| {
            requirement.arguments = requirement
                .arguments
                .iter()
                .map(|arg| specialize_argument(arg, bindings))
                .collect();
            requirement
        })
        .collect();
}

pub(super) fn specialize_summary(summary: &mut Summary, bindings: &HashMap<String, String>) {
    specialize_requirements(&mut summary.required, bindings);
    summary.parameter_uses = std::mem::take(&mut summary.parameter_uses)
        .into_iter()
        .map(|mut use_| {
            specialize_requirements(&mut use_.excluded, bindings);
            map_parameter_use_values(&mut use_, |value| {
                specialize_value(value, bindings);
            });
            use_
        })
        .collect();
}

pub(super) fn specialize_value(value: &mut Value, bindings: &HashMap<String, String>) {
    specialize_summary(&mut value.deferred, bindings);
    specialize_requirements(&mut value.performed, bindings);
    if let Some(Callable::Parameter { projection, .. }) = &mut value.callable {
        for part in projection.iter_mut() {
            if let Projection::Handled(answers) = part {
                *answers = std::mem::take(answers)
                    .into_iter()
                    .map(|(mut requirement, answer)| {
                        requirement.arguments = requirement
                            .arguments
                            .iter()
                            .map(|argument| specialize_argument(argument, bindings))
                            .collect();
                        (requirement, answer)
                    })
                    .collect();
            }
        }
        map_projection_values(projection, |value| specialize_value(value, bindings));
    }
    if let Some(Callable::Known(known)) = &mut value.callable {
        specialize_summary(&mut known.summary, bindings);
        if let Some(returned) = &mut known.returned {
            specialize_value(returned, bindings);
        }
    }
    for nested in value.fields.values_mut() {
        specialize_value(nested, bindings);
    }
    for nested in [
        value.element.as_mut(),
        value.result_payload.as_mut(),
        value.fiber_payload.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        specialize_value(nested, bindings);
    }
}
