//! Publish the final checker proof; tooling never re-derives operation requirements.
use super::{full_application, requirement_name, Index, Instances, Requirement, Summary, Value};
use crate::info::EffectRequirements;
use std::collections::HashMap;

pub(super) fn collect(
    index: &Index<'_>,
    instances: &Instances,
    rows: &[Summary],
    returns: &[Option<Value>],
) -> HashMap<String, EffectRequirements> {
    index
        .functions
        .iter()
        .zip(rows)
        .zip(returns)
        .map(|((function, own), returned)| {
            let result_type = instances.signatures.get(&function.name).map(|(_, ret)| ret);
            let row = full_application(function.body, own, returned.as_ref(), result_type);
            (function.qualified.clone(), report(instances, row))
        })
        .collect()
}

pub(super) fn report(instances: &Instances, row: Summary) -> EffectRequirements {
    let operations: std::collections::BTreeSet<_> = row
        .required
        .iter()
        .map(|requirement| operation_name(instances, requirement))
        .collect();
    let unresolved_instances = row.required.iter().any(|requirement| {
        !requirement.arguments.is_empty()
            && instances
                .argument_types
                .get(&requirement.arguments)
                .is_none_or(|arguments| arguments.iter().any(crate::has_type_var))
    });
    EffectRequirements {
        unresolved_instances,
        operations: operations.into_iter().collect(),
        runtime_builtins: row.runtime_builtins.into_iter().collect(),
        unresolved_callbacks: row.unresolved_dynamic_call || !row.parameter_uses.is_empty(),
    }
}

pub(super) fn operation_name(instances: &Instances, requirement: &Requirement) -> String {
    let mut display = requirement.clone();
    display.arguments = instances
        .display_arguments
        .get(&requirement.arguments)
        .cloned()
        .unwrap_or_else(|| requirement.arguments.clone());
    requirement_name(&display)
}

#[derive(Default)]
pub(crate) struct CheckedEffects {
    pub(crate) errors: Vec<crate::TypeError>,
    pub(crate) signal_queries: HashMap<usize, EffectRequirements>,
    pub(crate) functions: HashMap<String, EffectRequirements>,
}
