//! Type checking: driver.
use super::{
    base_env, effect_instances, publish_program, refine_handler_arguments, Checker, Program,
    TypeError,
};

// Whole-program effect proofs run, counted so the build path can assert it
// solves once across the gate and codegen. [TYPE-EFFECT-PROOF-COUNT]
std::thread_local! {
    static EFFECT_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The number of whole-program effect proofs run on this thread. Hidden: an
/// observability hook for the build-path reuse test, not a stable API.
#[doc(hidden)]
#[must_use]
pub fn effect_check_count() -> usize {
    EFFECT_CHECKS.with(std::cell::Cell::get)
}

/// One whole-program solve, retained for codegen: the lowered backend program
/// (prelude attached, static handlers discharged) and the published backend
/// types. Codegen consumes this instead of re-running [`infer_program`].
#[derive(Debug)]
pub struct CheckedProgram {
    source: Program,
    source_debug: String,
    lowered: Program,
    types: crate::info::ProgramTypes,
}

impl CheckedProgram {
    fn new(source: &Program, lowered: Program, types: crate::info::ProgramTypes) -> Self {
        Self {
            source: osprey_ast::with_stack(|| source.clone()),
            source_debug: osprey_ast::with_stack(|| format!("{source:?}")),
            lowered,
            types,
        }
    }

    /// Whether these checked artifacts belong to exactly this source program.
    #[must_use]
    pub fn matches_source(&self, program: &Program) -> bool {
        osprey_ast::with_stack(|| {
            // PartialEq equates signed zeros; derived Debug retains their sign
            // throughout the AST, including literal patterns. NaNs fail PartialEq.
            self.source == *program && self.source_debug == format!("{program:?}")
        })
    }

    /// Split into the lowered program and its published types.
    #[must_use]
    pub fn into_parts(self) -> (Program, crate::info::ProgramTypes) {
        (self.lowered, self.types)
    }
}

/// Type-check a program. Returns every type error found (empty ⇒ well-typed).
#[must_use]
pub fn check_program(program: &Program) -> Vec<TypeError> {
    check_program_exports(program, &[])
}
/// Check a program and require each named library export to discharge its own
/// effects, independently of handlers installed by `main`. [IOS-HOST-ABI]
#[must_use]
pub fn check_program_exports(program: &Program, exports: &[&str]) -> Vec<TypeError> {
    match check_and_publish(program, exports) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

/// Lower, check and publish a program in one solve, retaining the artifacts
/// codegen needs so the build path never re-runs the whole-program check.
/// Callers that only need diagnostics go through [`check_program`].
///
/// # Errors
/// Returns lowering errors or the program's type and effect errors.
pub fn check_and_publish(
    program: &Program,
    exports: &[&str],
) -> Result<CheckedProgram, Vec<TypeError>> {
    let lowered = crate::staging::lower_static_checked(program)?;
    let checker = checked_program_with_exports(
        &lowered,
        exports,
        crate::staging::has_staging(program),
        false,
    );
    if checker.errors.is_empty() {
        let types = publish_program(&lowered, checker, true);
        Ok(CheckedProgram::new(program, lowered, types))
    } else {
        Err(checker.errors)
    }
}
/// Run inference and publish the resolved signatures, constructor layouts and
/// union tags for the code generator. Type errors are intentionally dropped
/// here — codegen runs after `check_program` has gated correctness — so the
/// backend always receives the best-effort resolved shape of every declaration.
#[must_use]
pub fn infer_program(program: &Program) -> crate::info::ProgramTypes {
    let program = crate::prelude::with_prelude(program);
    publish_program(&program, checked_program(&program), true)
}
/// Diagnostics need one checked solve and no backend call elaboration.
pub(crate) fn infer_checked(
    program: &Program,
) -> Result<crate::info::ProgramTypes, Vec<TypeError>> {
    infer_checked_with_rejection(program, false)
}

/// Annotation trials need a verdict, not further diagnostics after rejection.
pub(crate) fn infer_annotation_candidate(
    program: &Program,
) -> Result<crate::info::ProgramTypes, Vec<TypeError>> {
    infer_checked_with_rejection(program, true)
}

fn infer_checked_with_rejection(
    program: &Program,
    reject_invalid: bool,
) -> Result<crate::info::ProgramTypes, Vec<TypeError>> {
    let program = crate::prelude::with_prelude(program);
    let checker = checked_program_with_exports(&program, &[], false, reject_invalid);
    if checker.errors.is_empty() {
        Ok(publish_program(&program, checker, false))
    } else {
        Err(checker.errors)
    }
}
/// Collect declarations and type-check a program before either caller consumes
/// diagnostics or publishes inferred backend metadata.
pub(super) fn checked_program(program: &Program) -> Checker {
    checked_program_with_exports(program, &[], false, false)
}
pub(super) fn checked_program_with_exports(
    program: &Program,
    exports: &[&str],
    source_validated: bool,
    reject_invalid: bool,
) -> Checker {
    let mut checker = inferred_data_contracts(program, source_validated);
    loop {
        checker.resolve_field_uses();
        if reject_invalid && !checker.errors.is_empty() {
            return checker;
        }
        if settle_effect_contracts(&mut checker, program, exports) {
            break;
        }
    }
    finish_data_contracts(&mut checker, program);
    checker
}

fn settle_effect_contracts(checker: &mut Checker, program: &Program, exports: &[&str]) -> bool {
    let instances = effect_instances(checker);
    EFFECT_CHECKS.with(|calls| calls.set(calls.get() + 1));
    let effect_proof = crate::effect_rows::check(program, &instances, exports);
    if refine_handler_arguments(checker, &instances) {
        return false;
    }
    checker.errors.extend(effect_proof.errors);
    checker.signal_queries = effect_proof.signal_queries;
    checker.function_effects = effect_proof.functions;
    true
}
pub(super) fn inferred_data_contracts(program: &Program, source_validated: bool) -> Checker {
    let mut checker = Checker::new();
    checker.source_contracts_validated = source_validated;
    let mut env = base_env();
    checker.collect(program, &mut env);
    checker.check(program, &mut env);
    checker.resolve_deferred_arithmetic();
    checker.default_untracked_receivers();
    checker
}
pub(super) fn finish_data_contracts(checker: &mut Checker, program: &Program) {
    checker.validate_builtin_uses();
    checker.validate_discards();
    checker.errors.extend(crate::init_order::check(program));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Program {
        let parsed = osprey_syntax::parse_program(source);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        parsed.program
    }

    fn oracle_checks(source: &str) -> (bool, usize) {
        let program = program(source);
        EFFECT_CHECKS.with(|calls| calls.set(0));
        let accepted = crate::redundant::published(&program).is_some();
        (accepted, EFFECT_CHECKS.with(std::cell::Cell::get))
    }

    #[test]
    fn invalid_annotation_trials_reject_before_effect_analysis() {
        assert_eq!(
            oracle_checks("fn answer() -> int = \"wrong\"\n"),
            (false, 0)
        );
    }

    #[test]
    fn valid_annotation_trials_still_run_effect_analysis() {
        let (accepted, checks) = oracle_checks("fn answer() -> int = 7\n");
        assert!(accepted);
        assert!(checks > 0);
    }

    #[test]
    fn annotation_trials_still_reject_unhandled_effects() {
        let (accepted, checks) =
            oracle_checks("effect Read { get: fn() -> int }\nlet value = perform Read.get()\n");
        assert!(!accepted);
        assert!(checks > 0);
    }

    #[test]
    fn full_diagnostics_keep_effect_errors_after_type_errors() {
        let program = program(
            "effect Read { get: fn() -> int }\nfn answer() -> int = \"wrong\"\nperform Read.get()\n",
        );
        let errors = check_program(&program);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("cannot unify")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("unhandled effect operations")),
            "{errors:?}"
        );
    }
}
