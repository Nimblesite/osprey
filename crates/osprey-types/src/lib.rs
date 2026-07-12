//! Hindley-Milner type inference for Osprey, together with the built-in
//! function signatures and the match validation (exhaustiveness, auto-wrap)
//! the language requires.
//!
//! The pipeline is the textbook one: a [`ty::Type`] language, an index-addressed
//! union-find substitution ([`ctx::InferCtx`]), [`unify`](unify::unify)
//! with the Osprey-specific rules (`any`, bare-collection generics, structural
//! records, Result auto-unwrap), let-polymorphism ([`env`]), and a two-pass
//! [`check::check_program`] driver over the AST.
//!
//! Public surface: [`check_program`] takes a parsed [`osprey_ast::Program`] and
//! returns the list of [`TypeError`]s (empty ⇒ well-typed).

mod builtin_docs;
mod builtin_docs_lang;
mod builtin_docs_sys;
mod builtins;
mod check;
mod convert;
mod ctx;
mod env;
mod error;
mod expr;
mod info;
mod pattern;
#[cfg(test)]
mod testutil;
mod ty;
mod unify;
mod variance;

pub use builtin_docs::{
    builtin_doc_view, builtin_hover_markdown, builtin_names, BuiltinDocView, BuiltinParam,
};
pub use builtins::builtin_signature;
pub use check::{check_program, infer_program};
pub use error::TypeError;
pub use info::{CtorLayout, HandlerSite, OpType, PerformSite, ProgramTypes};
pub use ty::{has_type_var, names, Scheme, Type, VarId};

#[cfg(test)]
#[expect(
    unused_results,
    reason = "tests drive checking for its side effects and discard the returned diagnostics"
)]
mod tests {
    use crate::testutil::{bad, ok};

    #[test]
    fn checks_arithmetic_and_let() {
        ok("fn inc(x: int) -> int = x + 1\nlet y = inc(41)\n");
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
        // `mut` bindings stay assignable.
        ok("fn main() -> Unit = {\n  mut y = 1\n  y = 2\n}\n");
    }

    #[test]
    fn elvis_on_result_is_a_truth_test_yielding_the_payload() {
        // `r ?: fallback` desugars to `match r { true => r  false => fallback }`;
        // over a `Result` that is a discriminant test whose value is the
        // unwrapped payload — it must not unify the payload with `bool`.
        ok("let okCalc = 10 + 5\n\
            let okElvis = okCalc ?: -1\n\
            fn keep(x: int) -> int = x + okElvis\n");
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
            fn double(x: int) -> int = x * 2\n\
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
