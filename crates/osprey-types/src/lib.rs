//! Hindley-Milner type inference for Osprey, together with the built-in
//! function signatures and the match validation (exhaustiveness, auto-wrap)
//! the language requires.
//!
//! The pipeline is the textbook one: a [`ty::Type`] language, an index-addressed
//! union-find substitution ([`ctx::InferCtx`]), [`unify`](unify::unify)
//! with the Osprey-specific rules (`any`, bare-collection generics, structural
//! records, directional Result Success wrapping), let-polymorphism ([`env`]), and a two-pass
//! [`check::check_program`] driver over the AST.
//!
//! Public surface: [`check_program`] takes a parsed [`osprey_ast::Program`] and
//! returns the list of [`TypeError`]s (empty ⇒ well-typed).

mod builtin_constraints;
mod builtin_docs;
mod builtin_docs_lang;
mod builtin_docs_sys;
mod builtins;
mod check;
mod convert;
mod ctx;
mod effect_rows;
#[cfg(test)]
mod effect_rows_expr_tests;
#[cfg(test)]
mod effect_rows_tests;
mod env;
mod error;
mod expr;
mod info;
mod init_order;
mod pattern;
#[cfg(test)]
mod testutil;
mod ty;
mod unify;
mod variance;

pub use builtin_docs::{
    builtin_doc_view, builtin_hover_markdown, builtin_names, BuiltinDocView, BuiltinParam,
};
pub use builtins::{builtin_callback_type, builtin_signature};
pub use check::{check_program, infer_program};
pub use error::TypeError;
pub use info::{CtorLayout, HandlerSite, OpType, PerformSite, ProgramTypes};
pub use ty::{has_type_var, names, render_with_holes, Scheme, Type, VarId, HOLE};

#[cfg(test)]
#[expect(
    unused_results,
    reason = "tests drive checking for its side effects and discard the returned diagnostics"
)]
mod tests {
    use crate::check_program;
    use crate::testutil::{bad, ok};
    use osprey_syntax::{parse_program_with_flavor, Flavor};

    #[test]
    fn checks_arithmetic_and_let() {
        ok("fn inc(x: int) -> Result<int, MathError> = x + 1\nlet y = inc(41)\n");
    }

    #[test]
    fn non_adjacent_ml_functions_with_one_name_are_duplicates() {
        let parsed = parse_program_with_flavor("f 0 = 1\ng x = x\nf n = n\n", Flavor::Ml);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        let errors = check_program(&parsed.program);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("duplicate definition `f`")),
            "expected duplicate-definition error, got: {errors:?}"
        );
    }

    #[test]
    fn string_concatenation_infers_string() {
        ok("fn greet(name: string) -> string = \"hi \" + name\n");
    }

    #[test]
    fn lambda_param_is_inferred_from_use() {
        // `s` has no annotation; `s + \"!\"` forces it to string.
        ok("let exclaim = fn(s) => s + \"!\"\nlet r = exclaim(\"hi\")\n");
    }

    #[test]
    fn records_field_access_and_update() {
        ok("type Point = { x: int, y: int }\n\
            let p = Point { x: 1, y: 2 }\n\
            let q = p { x: 10 }\n\
            fn px(pt: Point) -> int = pt.x\n");
    }

    #[test]
    fn result_pattern_binds_payload_type() {
        ok("fn unwrap(r: Result<int, Error>) -> int = match r {\n\
              Success { value } => value\n\
              Error { message } => 0\n\
            }\n");
    }

    #[test]
    fn user_variant_shadowing_success_still_matches_real_results() {
        // `Success` names both a user variant and the built-in Result ok-arm:
        // a pattern over a real `Result` must mean the builtin, while bare
        // `Success` as a value builds the user union.
        ok("type TaskResult = Success | Warning | Failed\n\
            fn pick(n: int) -> TaskResult = match n {\n\
              0 => Success\n\
              1 => Warning\n\
              _ => Failed\n\
            }\n\
            fn total(r: Result<int, Error>) -> int = match r {\n\
              Success { value } => value\n\
              Error { message } => 0\n\
            }\n");
    }

    #[test]
    fn constructor_field_set_is_validated() {
        // Missing and unknown fields are both errors.
        let errs = bad("type R = Ok { value: int } | No { message: string }\n\
                        let r = Ok { data: 42 }\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("requires field `value`")));
        assert!(errs.iter().any(|e| e.message.contains("no field `data`")));
    }

    #[test]
    fn unknown_variant_in_match_is_an_error() {
        let errs = bad("type Color = Red | Green | Blue\n\
                        let c = Red\n\
                        let d = match c {\n\
                          Red => 1\n\
                          Maybe => 2\n\
                          _ => 0\n\
                        }\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("`Maybe` is not defined in type `Color`")));
    }

    #[test]
    fn builtin_redefinition_is_an_error() {
        let errs = bad("fn toString(x: int) -> string = \"custom\"\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("redefine built-in function `toString`")));
    }

    #[test]
    fn assignment_to_immutable_binding_is_an_error() {
        let errs = bad("fn main() -> Unit = {\n  let x = 42\n  x = 100\n}\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("immutable variable `x`")));
    }

    #[test]
    fn mutable_assignment_requires_an_effect_handler_arm() {
        let errs = bad("fn main() -> Unit = {\n  mut cell = 0\n  cell = 1\n}\n");
        assert!(errs.iter().any(|e| {
            e.message
                .contains("state mutation is only allowed inside an effect handler arm")
        }));

        ok("effect State { set: fn(int) -> Unit }\n\
            fn main() -> Unit = {\n\
              mut cell = 0\n\
              handle State\n\
                set value => { cell = value }\n\
              in { perform State.set(1) }\n\
            }\n");
    }

    #[test]
    fn handled_client_body_does_not_gain_mutation_authority() {
        let errs = bad("effect State { set: fn(int) -> Unit }\n\
            fn main() -> Unit = {\n\
              mut cell = 0\n\
              handle State\n\
                set value => { cell = value }\n\
              in { cell = 1 }\n\
            }\n");
        assert!(errs.iter().any(|e| {
            e.message
                .contains("state mutation is only allowed inside an effect handler arm")
        }));
    }

    #[test]
    fn ml_mutable_assignment_requires_an_effect_handler_arm() {
        let parsed = parse_program_with_flavor("mut cell = 0\ncell := 1\n", Flavor::Ml);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        let errors = check_program(&parsed.program);
        assert!(errors.iter().any(|e| {
            e.message
                .contains("state mutation is only allowed inside an effect handler arm")
        }));
    }

    #[test]
    fn elvis_on_result_defaults_error_and_yields_success_payload() {
        // `r ?: fallback` desugars to an explicit exhaustive Result match:
        // Success yields its payload and Error yields the fallback.
        ok("let okCalc = intDiv(a: 10, b: 5)\n\
            let okElvis = okCalc ?: -1\n\
            fn keep(x: int) -> int = (x + okElvis) ?: 0\n");
    }

    #[test]
    fn elvis_rejects_a_scrutinee_that_is_not_a_result() {
        // [PATTERN-RESULT-DEFAULT]: "The scrutinee must be a `Result`; `?:` is
        // not a boolean operator and never reinterprets a plain value as
        // `Success`." `?:` desugars to a Success/Error match, so it used to
        // inherit the ORDINARY match auto-wrap rule — under which any value may
        // be matched as if wrapped in `Success` — and `5 ?: -1` type-checked and
        // printed `5`, with the fallback silently unreachable.
        let errs = bad("let dead = 5 ?: -1\n");
        assert!(errs.iter().any(|e| e.message.contains("`?:`")), "{errs:?}");
    }

    #[test]
    fn an_ordinary_success_arm_still_auto_wraps_a_plain_scrutinee() {
        // The rule above is specific to `?:`. A hand-written `Success` arm keeps
        // the documented auto-wrap, which is what lets a validated record
        // construction be matched without a real Result.
        ok("let label = match 5 {\n\
              Success { value } => toString(value)\n\
              Error { message } => message\n\
            }\n");
    }

    #[test]
    fn generic_union_flows_type_argument() {
        ok("type Box<T> = Empty | Full { value: T }\n\
            let b = Full { value: 7 }\n\
            let s = match b {\n\
              Full { value } => toString(value)\n\
              Empty => \"empty\"\n\
            }\n");
    }

    #[test]
    fn higher_order_function_application() {
        ok(
            "fn applyFn(value: int, func: (int) -> int) -> int = func(value)\n\
            fn double(x: int) -> int = (x * 2) ?: 0\n\
            let r = applyFn(value: 10, func: double)\n",
        );
    }

    #[test]
    fn reports_type_mismatch_in_call() {
        bad("fn inc(x: int) -> int = x + 1\nlet r = inc(\"not an int\")\n");
    }

    #[test]
    fn reports_non_exhaustive_bool_match() {
        let errs = bad("fn f(b: bool) -> int = match b { true => 1 }\n");
        assert!(errs.iter().any(|e| e.message.contains("non-exhaustive")));
    }

    #[test]
    fn reports_unknown_identifier() {
        let errs = bad("let x = totallyUndefinedThing\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("unknown identifier")));
    }
}
