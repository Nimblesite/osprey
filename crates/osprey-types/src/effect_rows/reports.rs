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
            let row = full_application(function.body, own, returned.as_ref());
            (function.qualified.clone(), report(instances, row))
        })
        .collect()
}

fn report(instances: &Instances, row: Summary) -> EffectRequirements {
    let operations: std::collections::BTreeSet<_> = row
        .required
        .iter()
        .map(|requirement| operation_name(instances, requirement))
        .collect();
    EffectRequirements {
        operations: operations.into_iter().collect(),
        runtime_builtins: row.runtime_builtins.into_iter().collect(),
        unresolved_callbacks: row.unresolved_dynamic_call || !row.parameter_uses.is_empty(),
    }
}

fn operation_name(instances: &Instances, requirement: &Requirement) -> String {
    let mut display = requirement.clone();
    display.arguments = instances
        .display_arguments
        .get(&requirement.arguments)
        .cloned()
        .unwrap_or_else(|| vec![crate::ty::HOLE.to_owned(); requirement.arguments.len()]);
    requirement_name(&display)
}

#[derive(Default)]
pub(crate) struct CheckedEffects {
    pub(crate) errors: Vec<crate::TypeError>,
    pub(crate) functions: HashMap<String, EffectRequirements>,
}
