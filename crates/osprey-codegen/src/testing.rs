//! Codegen for the testing built-ins `test` / `expect` / `check`: lowers each
//! call to the TAP-emitting C runtime (`compiler/runtime/test_runtime.c`).
//! Assertion equality is canonical-string equality; a successful Result uses
//! its payload string while an Error stays visible as `Error(<message>)`
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
        "expectAll" => gen_all(cg, &args, false).map(Some),
        "expectTrue" => gen_bool_expect(cg, &args, true, None).map(Some),
        "expectFalse" => gen_bool_expect(cg, &args, false, None).map(Some),
        "check" => gen_check(cg, &args).map(Some),
        "checkAll" => gen_all(cg, &args, true).map(Some),
        "checkTrue" => gen_bool_expect(cg, &args, true, Some("checkTrue")).map(Some),
        "checkFalse" => gen_bool_expect(cg, &args, false, Some("checkFalse")).map(Some),
        "reportPass" => Ok(Some(gen_pass_report(cg))),
        "reportFail" => gen_reason_report(cg, "osp_test_fail", args.first().copied()).map(Some),
        "reportSkip" => gen_reason_report(cg, "osp_test_skip", args.first().copied()).map(Some),
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

/// The report primitive each `Verdict` state drives. `Pass` takes no reason —
/// `osp_test_pass` has nothing to say — so only the two failing states read a
/// payload [TESTING-VERDICT].
const VERDICT_REPORTS: [(&str, &str, bool); 3] = [
    ("Pass", "reportPass", false),
    ("Fail", "reportFail", true),
    ("Skip", "reportSkip", true),
];

/// Drive a `Verdict`-returning case body into the TAP runtime: pattern-match the
/// value and call exactly one report primitive [TESTING-VERDICT]. A body that is
/// not a `Verdict` (the Default flavor's Unit-returning imperative case) reports
/// nothing here — its inline `expect`/`check` already recorded assertions.
fn report_verdict(cg: &mut Codegen, body: &Value) -> Result<()> {
    if body.osp_ty.as_deref() != Some(VERDICT_TY) {
        return Ok(());
    }
    let arms = verdict_arms(cg)?;
    let subject = format!("__verdict.{}", cg.fresh_label());
    cg.bind(subject.clone(), body.clone());
    let _ = crate::pattern::gen_match(cg, &Expr::Identifier(subject), &arms)?;
    Ok(())
}

/// One arm per state the program's OWN `Verdict` declares. Reading the
/// declaration — instead of assuming `Fail(reason)` and `Skip(why)` — is what
/// lets a payload-free state compile: the synthesized pattern binds exactly the
/// fields that exist, so `Skip` with no reason reports none rather than failing
/// codegen on an invented binder. Implements [TESTING-VERDICT].
fn verdict_arms(cg: &Codegen) -> Result<Vec<MatchArm>> {
    let states = cg.union_variants(VERDICT_TY).ok_or_else(|| {
        CodegenError::unsupported(format!(
            "a test case answering `{VERDICT_TY}` needs a `type {VERDICT_TY}` union with Pass, Fail and Skip states"
        ))
    })?;
    let states = states.to_vec();
    verdict_states_complete(&states)?;
    states.iter().map(|state| verdict_arm(cg, state)).collect()
}

/// Reject a `Verdict` that omits one of the three states. Generating arms only
/// for what happens to be declared made `type Verdict = Pass` compile and
/// report a passing case: the missing constructors simply never appeared in the
/// match, so a suite that could not express failure still looked green. Extra
/// states are caught by [`verdict_report`]; this is the other direction.
/// Implements [TESTING-VERDICT].
fn verdict_states_complete(states: &[String]) -> Result<()> {
    let missing: Vec<&str> = VERDICT_REPORTS
        .iter()
        .map(|(name, _, _)| *name)
        .filter(|name| !states.iter().any(|state| state == name))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(CodegenError::unsupported(format!(
        "`type {VERDICT_TY}` must declare Pass, Fail and Skip; it is missing {}",
        missing.join(", ")
    )))
}

/// One `Verdict` arm: `Ctor reason => reportFn(reason)` when the state declares
/// a payload, `Ctor => reportFn()` when it declares none.
fn verdict_arm(cg: &Codegen, state: &str) -> Result<MatchArm> {
    let (report_fn, takes_reason) = verdict_report(state)?;
    let binder: Vec<String> = takes_reason
        .then(|| verdict_reason(cg, state))
        .transpose()?
        .flatten()
        .into_iter()
        .collect();
    Ok(MatchArm {
        pattern: Pattern::Constructor {
            name: state.to_string(),
            fields: binder.clone(),
            sub_patterns: Vec::new(),
        },
        body: Expr::Call {
            function: Box::new(Expr::Identifier(report_fn.to_string())),
            arguments: binder.into_iter().map(Expr::Identifier).collect(),
            named_arguments: Vec::new(),
        },
    })
}

/// The report primitive a `Verdict` state drives, and whether it carries a
/// reason. A state `test` has no primitive for is named in the rejection rather
/// than reaching pattern lowering as a binder nobody wrote.
fn verdict_report(state: &str) -> Result<(&'static str, bool)> {
    VERDICT_REPORTS
        .iter()
        .find(|(name, _, _)| *name == state)
        .map(|(_, report_fn, takes_reason)| (*report_fn, *takes_reason))
        .ok_or_else(|| {
            CodegenError::unsupported(format!(
                "`test` reports only the Pass, Fail and Skip states of `{VERDICT_TY}`; it has no report for `{state}`"
            ))
        })
}

/// The single field a failing `Verdict` state carries, whatever the declaration
/// spells it, or `None` when it carries no payload. A state with several fields
/// has no one reason to report, so say that rather than silently picking one.
fn verdict_reason(cg: &Codegen, state: &str) -> Result<Option<String>> {
    let fields = cg.ctor_layout(state).map(|c| c.fields).unwrap_or_default();
    match fields.as_slice() {
        [] => Ok(None),
        [(only, _)] => Ok(Some(only.clone())),
        many => Err(CodegenError::unsupported(format!(
            "`{VERDICT_TY}` state `{state}` declares {} fields; `test` reports a single reason",
            many.len()
        ))),
    }
}

/// `expect(actual, expected)` — computed value before expected value, no label.
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

/// `check(label, expected, actual)` — expected value before computed value.
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

/// Grouped soft assertions. Each boolean in the source list literal is
/// evaluated and reported independently, so a failed item never masks later
/// checks. Keeping this as a compiler form avoids allocating a runtime list
/// solely to iterate assertions.
fn gen_all(cg: &mut Codegen, args: &[&Expr], labeled: bool) -> Result<Value> {
    let (label, conditions) = match (labeled, args) {
        (false, [conditions]) => (None, *conditions),
        (true, [label, conditions]) => (Some(*label), *conditions),
        (false, _) => {
            return Err(CodegenError::invalid(
                "expectAll needs one list-literal argument",
            ));
        }
        (true, _) => {
            return Err(CodegenError::invalid(
                "checkAll needs (label, conditions) arguments",
            ));
        }
    };
    let Expr::List(items, _) = conditions else {
        return Err(CodegenError::invalid(
            "expectAll/checkAll conditions must be a list literal",
        ));
    };
    if items.is_empty() {
        return Err(CodegenError::invalid(
            "expectAll/checkAll needs at least one condition",
        ));
    }
    let label_value = label.map(|expr| eval_to_string(cg, expr)).transpose()?;
    for condition in items {
        let expected = eval_to_string(cg, &Expr::Bool(true))?;
        let actual = eval_to_string(cg, condition)?;
        let _ = emit_assert(
            cg,
            label_value.as_ref().map_or("null", |value| &value.operand),
            &expected,
            &actual,
        );
    }
    Ok(Value::unit())
}

/// Compact boolean assertions: `expectTrue(actual)` / `expectFalse(actual)`,
/// plus labeled `checkTrue(label, actual)` / `checkFalse(label, actual)`.
fn gen_bool_expect(
    cg: &mut Codegen,
    args: &[&Expr],
    expected: bool,
    labeled_name: Option<&str>,
) -> Result<Value> {
    let (label, actual) = match (labeled_name, args) {
        (None, [actual]) => (None, *actual),
        (Some(_), [label, actual]) => (Some(*label), *actual),
        (None, _) => {
            return Err(CodegenError::invalid(
                "expectTrue/expectFalse needs one boolean argument",
            ));
        }
        (Some(name), _) => {
            return Err(CodegenError::invalid(format!(
                "{name} needs (label, actual) arguments"
            )));
        }
    };
    let label_value = label.map(|expr| eval_to_string(cg, expr)).transpose()?;
    let expected_value = eval_to_string(cg, &Expr::Bool(expected))?;
    let actual_value = eval_to_string(cg, actual)?;
    Ok(emit_assert(
        cg,
        label_value.as_ref().map_or("null", |value| &value.operand),
        &expected_value,
        &actual_value,
    ))
}

/// `reportPass()`: the one report primitive with nothing to say — `osp_test_pass`
/// takes no arguments [TESTING-VERDICT].
fn gen_pass_report(cg: &mut Codegen) -> Value {
    cg.testing_used = true;
    cg.call_void("osp_test_pass", "", &[]);
    Value::unit()
}

/// `reportFail(reason)` / `reportSkip(reason)`: the effect boundary of the pure
/// ML-flavor `Verdict` model — the library pattern-matches a `Verdict` and calls
/// exactly one report primitive to record it with the runtime [TESTING-VERDICT].
/// The reason lowers to a canonical string like any assertion operand.
///
/// A state whose declaration carries no payload passes an explicit `null`, which
/// both primitives read as "no reason given". Calling the one-argument C function
/// with no argument instead left its reason register holding whatever the caller
/// last put there, and `osp_test_skip` then formatted that as a string.
fn gen_reason_report(cg: &mut Codegen, runtime_fn: &str, reason: Option<&Expr>) -> Result<Value> {
    cg.testing_used = true;
    let operand = match reason {
        None => "null".to_string(),
        Some(expr) => eval_to_string(cg, expr)?.operand,
    };
    cg.call_void(runtime_fn, "i8*", &[&operand]);
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
