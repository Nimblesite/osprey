//! Codegen for the testing built-ins `test` / `expect` / `check`: lowers each
//! call to the TAP-emitting C runtime (`compiler/runtime/test_runtime.c`).
//! Assertion equality is canonical-string equality after Result auto-unwrap
//! [TESTING-EQUALITY]. Implements [TESTING-CODEGEN], [TESTING-BUILTIN-TEST],
//! [TESTING-BUILTIN-EXPECT], [TESTING-BUILTIN-CHECK]
//! (docs/specs/0027-TestingFramework.md).

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::{arg_exprs, gen_expr};
use crate::llty::Value;
use crate::runtime::to_string_value;
use osprey_ast::{Expr, MatchArm, NamedArgument, Pattern};

/// The union type the pure ML-flavor testing surface reports [TESTING-VERDICT].
const VERDICT_TY: &str = "Verdict";

/// Dispatch a testing built-in call; `None` when `name` is not one, or when a
/// user-defined function OR extern shadows the name [TESTING-SHADOWING].
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    if cg.fn_params.contains_key(name) || cg.prog.functions.contains_key(name) {
        return Ok(None);
    }
    let args = arg_exprs(arguments, named);
    match name {
        "test" => gen_test(cg, &args).map(Some),
        "expect" => gen_expect(cg, &args).map(Some),
        "check" => gen_check(cg, &args).map(Some),
        "reportPass" => gen_report(cg, "osp_test_pass", None).map(Some),
        "reportFail" => gen_report(cg, "osp_test_fail", args.first().copied()).map(Some),
        "reportSkip" => gen_report(cg, "osp_test_skip", args.first().copied()).map(Some),
        _ => Ok(None),
    }
}

/// `test(name, body)`: begin (returns whether the case runs, applying the
/// `OSPREY_TEST_FILTER` skip [TESTING-FILTER]), branch around the invoked body,
/// report the body's `Verdict` when it is one ([TESTING-VERDICT]), end (prints
/// the TAP result line). A Unit body (the Default flavor's imperative case)
/// reports nothing extra — its inline `expect`/`check` already recorded.
fn gen_test(cg: &mut Codegen, args: &[&Expr]) -> Result<Value> {
    let [name_expr, body_expr] = args else {
        return Err(CodegenError::invalid("test needs (name, body) arguments"));
    };
    let name_str = eval_to_string(cg, name_expr)?;
    cg.testing_used = true;
    let run = cg.call("i32", "osp_test_begin", "i8*", &[&name_str.operand]);
    let cond = cg.emit_reg(format!("icmp ne i32 {run}, 0"));
    let (run_bb, end_bb) = (cg.fresh_label(), cg.fresh_label());
    cg.emit(format!("br i1 {cond}, label %{run_bb}, label %{end_bb}"));
    cg.start_block(&run_bb);
    // Invoking the body as a synthesized zero-arg call reuses the whole call
    // dispatch: an inline lambda beta-reduces, a named function or closure
    // value calls through its normal path.
    let invoke = Expr::Call {
        function: Box::new((*body_expr).clone()),
        arguments: Vec::new(),
        named_arguments: Vec::new(),
    };
    let body = gen_expr(cg, &invoke)?;
    report_verdict(cg, &body)?;
    cg.call_void("osp_test_end", "i8*", &[&name_str.operand]);
    let _ = cg.snapshot_to(&end_bb);
    cg.start_block(&end_bb);
    Ok(Value::unit())
}

/// Drive a `Verdict`-returning case body into the TAP runtime: pattern-match the
/// value and call exactly one report primitive [TESTING-VERDICT]. A body that is
/// not a `Verdict` (the Default flavor's Unit-returning imperative case) reports
/// nothing here — its inline `expect`/`check` already recorded assertions.
fn report_verdict(cg: &mut Codegen, body: &Value) -> Result<()> {
    if body.osp_ty.as_deref() != Some(VERDICT_TY) {
        return Ok(());
    }
    let subject = format!("__verdict.{}", cg.fresh_label());
    cg.bind(subject.clone(), body.clone());
    let arms = vec![
        verdict_arm("Pass", &[], "reportPass", None),
        verdict_arm("Fail", &["reason"], "reportFail", Some("reason")),
        verdict_arm("Skip", &["why"], "reportSkip", Some("why")),
    ];
    let _ = crate::pattern::gen_match(cg, &Expr::Identifier(subject), &arms)?;
    Ok(())
}

/// One `Verdict` arm: `Ctor field => reportFn(field)` (or `Ctor => reportFn()`).
fn verdict_arm(ctor: &str, fields: &[&str], report_fn: &str, arg: Option<&str>) -> MatchArm {
    MatchArm {
        pattern: Pattern::Constructor {
            name: ctor.to_string(),
            fields: fields.iter().map(|f| (*f).to_string()).collect(),
            sub_patterns: Vec::new(),
        },
        body: Expr::Call {
            function: Box::new(Expr::Identifier(report_fn.to_string())),
            arguments: arg
                .map(|a| vec![Expr::Identifier(a.to_string())])
                .unwrap_or_default(),
            named_arguments: Vec::new(),
        },
    }
}

/// `expect(actual, expected)` — Jest argument order, no label.
fn gen_expect(cg: &mut Codegen, args: &[&Expr]) -> Result<Value> {
    let [actual, expected] = args else {
        return Err(CodegenError::invalid(
            "expect needs (actual, expected) arguments",
        ));
    };
    let a = eval_to_string(cg, actual)?;
    let e = eval_to_string(cg, expected)?;
    Ok(emit_assert(cg, "null", &e, &a))
}

/// `check(label, expected, actual)` — Alcotest argument order.
fn gen_check(cg: &mut Codegen, args: &[&Expr]) -> Result<Value> {
    let [label, expected, actual] = args else {
        return Err(CodegenError::invalid(
            "check needs (label, expected, actual) arguments",
        ));
    };
    let l = eval_to_string(cg, label)?;
    let e = eval_to_string(cg, expected)?;
    let a = eval_to_string(cg, actual)?;
    Ok(emit_assert(cg, &l.operand, &e, &a))
}

/// `reportPass()` / `reportFail(reason)` / `reportSkip(reason)`: the effect
/// boundary of the pure ML-flavor `Verdict` model — the library pattern-matches
/// a `Verdict` and calls exactly one of these to record it with the runtime
/// [TESTING-VERDICT]. Returns Unit; the reason (when present) lowers to a
/// canonical string like any assertion operand.
fn gen_report(cg: &mut Codegen, runtime_fn: &str, reason: Option<&Expr>) -> Result<Value> {
    cg.testing_used = true;
    match reason {
        None => cg.call_void(runtime_fn, "", &[]),
        Some(expr) => {
            let r = eval_to_string(cg, expr)?;
            cg.call_void(runtime_fn, "i8*", &[&r.operand]);
        }
    }
    Ok(Value::unit())
}

/// Evaluate one assertion operand to its canonical string: a Success renders
/// as its bare payload, an Error as `Error(<message>)` (a visible mismatch,
/// never a blind payload load), everything else through the shared `toString`
/// lowering [TESTING-EQUALITY].
fn eval_to_string(cg: &mut Codegen, expr: &Expr) -> Result<Value> {
    let v = gen_expr(cg, expr)?;
    reject_opaque_handle(&v)?;
    if v.result_inner.is_some() {
        return crate::runtime::result_payload_or_error_string(cg, &v);
    }
    to_string_value(cg, v)
}

/// Lists, maps, records, and other runtime handles have no canonical string
/// rendering yet, so an assertion on one would compare raw pointers — reject
/// loudly instead [TESTING-EQUALITY].
fn reject_opaque_handle(v: &Value) -> Result<()> {
    let opaque = (v.result_inner.is_none() && v.ty == crate::llty::LType::Ptr)
        || v.result_inner == Some(crate::llty::LType::Ptr);
    if opaque {
        return Err(CodegenError::unsupported(
            "expect/check on a list, map, or record value; compare scalar fields or elements",
        ));
    }
    Ok(())
}

/// Compare two canonical strings and record the verdict with the runtime.
/// `label_op` is the rendered label operand — `null` for `expect`.
fn emit_assert(cg: &mut Codegen, label_op: &str, expected: &Value, actual: &Value) -> Value {
    cg.testing_used = true;
    let c = cg.call(
        "i32",
        "strcmp",
        "i8*, i8*",
        &[&actual.operand, &expected.operand],
    );
    let ok = cg.emit_reg(format!("icmp eq i32 {c}, 0"));
    let ok32 = cg.emit_reg(format!("zext i1 {ok} to i32"));
    cg.call_void(
        "osp_test_assert",
        "i8*, i32, i8*, i8*",
        &[label_op, &ok32, &expected.operand, &actual.operand],
    );
    Value::unit()
}
