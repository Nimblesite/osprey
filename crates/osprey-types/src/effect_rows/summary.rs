//! Effect row summary.
use super::{
    map_parameter_use_values, widen_value_budget, CallArguments, HashSet, ParameterUse, Projection,
    Requirement, Requirements, Summary, MAX_PROVENANCE_DEPTH, MAX_PROVENANCE_NODES,
};

impl Requirement {
    /// A requirement keyed by effect identity: the base name plus the
    /// resolved arguments, so the written spelling `Stash<int>` and `Stash`
    /// name one effect. Implements [EFFECTS-GENERIC-INSTANTIATION].
    pub(super) fn new(effect: &str, operation: &str, arguments: Vec<String>) -> Self {
        Self {
            effect: osprey_ast::effect_name::base(effect).to_owned(),
            operation: operation.to_owned(),
            arguments,
        }
    }
}

impl CallArguments {
    /// Function-value slots have no parameter names. Do this at the call site,
    /// before substitution reveals a callback's declaration and its names.
    pub(super) fn in_written_order(mut self) -> Self {
        self.positional
            .extend(self.named.drain(..).map(|(_, value)| value));
        self
    }
}

impl Summary {
    pub(super) fn union(&mut self, other: Self) {
        self.required.extend(other.required);
        self.runtime_builtins.extend(other.runtime_builtins);
        self.parameter_uses.extend(other.parameter_uses);
        self.unresolved_dynamic_call |= other.unresolved_dynamic_call;
    }

    pub(super) fn without_operations(
        mut self,
        effect: &str,
        arguments: &[String],
        operations: &HashSet<String>,
    ) -> Self {
        let excluded: Requirements = self
            .required
            .iter()
            .filter(|r| {
                r.effect == effect && r.arguments == arguments && operations.contains(&r.operation)
            })
            .cloned()
            .collect();
        self.required.retain(|r| !excluded.contains(r));
        if !operations.is_empty() {
            self.parameter_uses =
                self.parameter_uses
                    .into_iter()
                    .map(|mut use_| {
                        // A callback requirement is not concrete until its call
                        // site. Record every operation this handler can discharge.
                        use_.excluded.extend(operations.iter().map(|operation| {
                            Requirement::new(effect, operation, arguments.to_vec())
                        }));
                        use_
                    })
                    .collect();
        }
        self
    }

    pub(super) fn excluding(mut self, excluded: &Requirements) -> Self {
        self.required.retain(|r| !excluded.contains(r));
        self.parameter_uses = self
            .parameter_uses
            .into_iter()
            .map(|mut use_| {
                use_.excluded.extend(excluded.iter().cloned());
                use_
            })
            .collect();
        self
    }

    pub(super) fn widen(&mut self) {
        self.widen_at(0);
    }

    pub(super) fn widen_at(&mut self, depth: usize) {
        self.widen_at_budget(depth, MAX_PROVENANCE_NODES);
    }

    pub(super) fn widen_at_budget(&mut self, depth: usize, budget: usize) {
        let before = self.parameter_uses.len();
        self.parameter_uses
            .retain(|use_| bounded_parameter_use(use_, depth));
        if self.parameter_uses.len() != before {
            self.unresolved_dynamic_call = true;
        }
        self.parameter_uses = std::mem::take(&mut self.parameter_uses)
            .into_iter()
            .map(|mut use_| {
                map_parameter_use_values(&mut use_, |value| {
                    *value = widen_value_budget(value.clone(), depth + 1, budget);
                });
                use_
            })
            .collect();
    }
}

pub(super) fn bounded_parameter_use(use_: &ParameterUse, depth: usize) -> bool {
    use_.level < MAX_PROVENANCE_DEPTH
        && use_.projection.len() < MAX_PROVENANCE_DEPTH
        && (depth < MAX_PROVENANCE_DEPTH
            || (use_.arguments.positional.is_empty()
                && use_.arguments.named.is_empty()
                && !use_.projection.iter().any(|part| {
                    matches!(
                        part,
                        Projection::Method(_) | Projection::Returned(_) | Projection::Handled(_)
                    )
                })))
}
