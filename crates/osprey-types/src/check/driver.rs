//! Type checking: driver.
use super::{
    base_env, effect_instances, publish_program, refine_handler_arguments, Checker, Program,
    TypeError,
};

/// Type-check a program. Returns every type error found (empty ⇒ well-typed).
#[must_use]
pub fn check_program(program: &Program) -> Vec<TypeError> {
    check_program_exports(program, &[])
}
/// Check a program and require each named library export to discharge its own
/// effects, independently of handlers installed by `main`. [IOS-HOST-ABI]
#[must_use]
pub fn check_program_exports(program: &Program, exports: &[&str]) -> Vec<TypeError> {
    match crate::staging::lower_static_checked(program) {
        Ok(lowered) => {
            checked_program_with_exports(&lowered, exports, crate::staging::has_staging(program))
                .errors
        }
        Err(errors) => errors,
    }
}
/// Run inference and publish the resolved signatures, constructor layouts and
/// union tags for the code generator. Type errors are intentionally dropped
/// here — codegen runs after `check_program` has gated correctness — so the
/// backend always receives the best-effort resolved shape of every declaration.
#[must_use]
pub fn infer_program(program: &Program) -> crate::info::ProgramTypes {
    publish_program(program, checked_program(program), true)
}
/// Diagnostics need one checked solve and no backend call elaboration.
pub(crate) fn infer_checked(
    program: &Program,
) -> Result<crate::info::ProgramTypes, Vec<TypeError>> {
    let checker = checked_program(program);
    if checker.errors.is_empty() {
        Ok(publish_program(program, checker, false))
    } else {
        Err(checker.errors)
    }
}
/// Collect declarations and type-check a program before either caller consumes
/// diagnostics or publishes inferred backend metadata.
pub(super) fn checked_program(program: &Program) -> Checker {
    checked_program_with_exports(program, &[], false)
}
pub(super) fn checked_program_with_exports(
    program: &Program,
    exports: &[&str],
    source_validated: bool,
) -> Checker {
    let mut checker = inferred_data_contracts(program, source_validated);
    loop {
        checker.resolve_field_uses();
        let instances = effect_instances(&mut checker);
        let effect_proof = crate::effect_rows::check(program, &instances, exports);
        if !refine_handler_arguments(&mut checker, &instances) {
            checker.errors.extend(effect_proof.errors);
            checker.function_effects = effect_proof.functions;
            break;
        }
    }
    finish_data_contracts(&mut checker, program);
    checker
}
pub(super) fn inferred_data_contracts(program: &Program, source_validated: bool) -> Checker {
    let mut checker = Checker::new();
    checker.source_contracts_validated = source_validated;
    let mut env = base_env();
    checker.collect(program, &mut env);
    checker.check(program, &mut env);
    checker.resolve_deferred_arithmetic();
    checker
}
pub(super) fn finish_data_contracts(checker: &mut Checker, program: &Program) {
    checker.validate_builtin_uses();
    checker.validate_discards();
    checker.errors.extend(crate::init_order::check(program));
}
