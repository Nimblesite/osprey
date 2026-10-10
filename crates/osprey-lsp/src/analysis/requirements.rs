//! One presentation of the checker's operation proof. [LSP-EFFECT-REQUIREMENTS]
use super::json_str;
use osprey_types::EffectRequirements;

pub(crate) fn description(requirements: Option<&EffectRequirements>) -> Option<String> {
    let requirements = requirements?;
    let parts: Vec<_> = [
        section("Requires on full application", &requirements.operations),
        section("Runtime operations", &requirements.runtime_builtins),
        requirements
            .unresolved_callbacks
            .then(|| "Callback effects remain unresolved.".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

fn section(label: &str, operations: &[String]) -> Option<String> {
    (!operations.is_empty()).then(|| format!("{label}: {}.", code_list(operations)))
}

fn code_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn json(requirements: &EffectRequirements) -> String {
    format!(
        "{{\"operations\":{},\"runtimeBuiltins\":{},\"unresolvedCallbacks\":{},\"description\":{}}}",
        array(&requirements.operations), array(&requirements.runtime_builtins),
        requirements.unresolved_callbacks, json_str(&description(Some(requirements)).unwrap_or_default()),
    )
}

fn array(items: &[String]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(|item| json_str(item))
            .collect::<Vec<_>>()
            .join(",")
    )
}
