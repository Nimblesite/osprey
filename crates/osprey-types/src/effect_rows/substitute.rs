//! Effect row substitute.
use super::{
    map_parameter_use_values, map_projection_values, shift_summary_levels, shift_value_levels,
    Analyzer, Callable, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn substitute_summary_at(
        &self,
        mut summary: Summary,
        target_level: usize,
        arguments: &[Option<Value>],
    ) -> Summary {
        let uses = std::mem::take(&mut summary.parameter_uses);
        for mut use_ in uses {
            map_parameter_use_values(&mut use_, |value| {
                *value = self.substitute_value_at(value.clone(), target_level, arguments);
            });
            if use_.level == target_level {
                let callback = arguments
                    .get(use_.index)
                    .and_then(Option::as_ref)
                    .cloned()
                    .and_then(|value| self.project_path(value, &use_.projection))
                    .and_then(|value| value.callable);
                if let Some(callback) = callback {
                    let mut invoked = self
                        .invoke_with_arguments(callback, use_.arguments)
                        .excluding(&use_.excluded);
                    shift_summary_levels(&mut invoked, target_level);
                    summary.union(invoked);
                } else if use_.index >= arguments.len() {
                    // No argument occupies this slot yet — the call is still
                    // under-applied, so the parameter stays symbolic for an
                    // enclosing scope to resolve. Keep its level: decrementing
                    // here is what re-attributes it to the wrong binder.
                    let _ = summary.parameter_uses.insert(use_);
                } else {
                    // An argument IS supplied but carries no callable
                    // provenance, so nothing proves what this slot will invoke.
                    // Re-attributing it to the caller's parameter at the same
                    // index — which `target_level.saturating_sub(1)` did, since
                    // a direct call has `target_level == 0` and 0 - 1 saturates
                    // back to 0 — hands the requirement to an unrelated binder,
                    // and any pure argument the caller passes in that slot then
                    // discharges an effect it never supplied. That is fail-OPEN
                    // and it breaks [EFFECTS-STATIC-DISCHARGE]: the program
                    // compiles and aborts at runtime with `unhandled effect`.
                    summary.unresolved_dynamic_call = true;
                }
            } else {
                if use_.level > target_level {
                    use_.level -= 1;
                }
                let _ = summary.parameter_uses.insert(use_);
            }
        }
        summary
    }

    pub(super) fn substitute_value_at(
        &self,
        mut value: Value,
        target_level: usize,
        arguments: &[Option<Value>],
    ) -> Value {
        // Substitute the callee's children before inserting caller provenance.
        // A replacement already belongs to the caller: walking it again can
        // replace its parameter with itself indefinitely for nested records,
        // and can also attribute a callback to the wrong lexical binder.
        for nested in value.fields.values_mut() {
            *nested = self.substitute_value_at(nested.clone(), target_level, arguments);
        }
        self.substitute_payload_at(&mut value.element, target_level, arguments);
        self.substitute_payload_at(&mut value.result_payload, target_level, arguments);
        self.substitute_payload_at(&mut value.fiber_payload, target_level, arguments);
        value.deferred = self.substitute_summary_at(value.deferred, target_level, arguments);
        if let Some(Callable::Parameter { projection, .. }) = &mut value.callable {
            map_projection_values(projection, |value| {
                *value = self.substitute_value_at(value.clone(), target_level, arguments);
            });
        }
        if let Some(callable) = value.callable.take() {
            match callable {
                Callable::Parameter {
                    level,
                    index,
                    projection,
                } if level == target_level => {
                    if let Some(mut replacement) = arguments
                        .get(index)
                        .and_then(Option::as_ref)
                        .cloned()
                        .and_then(|argument| self.project_path(argument, &projection))
                    {
                        shift_value_levels(&mut replacement, target_level, 0);
                        value.field_names.clone_from(&replacement.field_names);
                        value.union(replacement);
                    } else if index >= arguments.len() {
                        // Only an absent argument keeps the original binder.
                        // A supplied opaque value or failed projection must
                        // never be resolved through an unrelated caller slot.
                        value.callable = Some(Callable::Parameter {
                            level,
                            index,
                            projection,
                        });
                    } else {
                        value.callable = Some(Callable::Unknown);
                    }
                }
                Callable::Parameter {
                    mut level,
                    index,
                    projection,
                } => {
                    if level > target_level {
                        level -= 1;
                    }
                    value.callable = Some(Callable::Parameter {
                        level,
                        index,
                        projection,
                    });
                }
                Callable::Known(mut known) => {
                    known.summary =
                        self.substitute_summary_at(known.summary, target_level + 1, arguments);
                    known.returned = known.returned.map(|returned| {
                        Box::new(self.substitute_value_at(*returned, target_level + 1, arguments))
                    });
                    value.callable = Some(Callable::Known(known));
                }
                Callable::Unknown => value.callable = Some(Callable::Unknown),
            }
        }
        value
    }

    /// Substitute through one optional boxed payload slot. The element, result
    /// and fiber payloads are all single nested values, so each descends the
    /// same way — an absent payload stays absent.
    pub(super) fn substitute_payload_at(
        &self,
        slot: &mut Option<Box<Value>>,
        target_level: usize,
        arguments: &[Option<Value>],
    ) {
        if let Some(payload) = slot.take() {
            *slot = Some(Box::new(self.substitute_value_at(
                *payload,
                target_level,
                arguments,
            )));
        }
    }
}
