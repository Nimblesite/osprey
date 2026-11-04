//! Type checking: refine.
use super::{type_is_resolved, Checker};

/// A handler whose arms leave its binder open learns that binder from the
/// operations its body requires, including requirements behind function calls.
/// Only one candidate may refine it; incompatible instantiations stay
/// distinct and the ordinary discharge proof rejects the remaining operation.
pub(super) fn refine_handler_arguments(
    checker: &mut Checker,
    instances: &crate::effect_rows::Instances,
) -> bool {
    let before = checker.ctx.bound_count();
    let candidates = instances.handler_inference.borrow();
    for (position, arguments, _) in checker.handler_tys.clone() {
        if arguments
            .iter()
            .all(|ty| type_is_resolved(&checker.ctx.apply(ty)))
        {
            continue;
        }
        let Some(choices) = candidates.get(&(position.line, position.column)) else {
            continue;
        };
        if choices.len() != 1 {
            continue;
        }
        let Some(choice) = choices.first() else {
            continue;
        };
        if arguments.len() != choice.len() {
            continue;
        }
        let Some(types) = instances.argument_types.get(choice) else {
            continue;
        };
        for (argument, concrete) in arguments.iter().zip(types) {
            checker.push_unify(argument, concrete);
        }
    }
    checker.ctx.bound_count() != before
}
