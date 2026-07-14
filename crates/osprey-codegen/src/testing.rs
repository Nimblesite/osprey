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
use osprey_ast::{Expr, NamedArgument};

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
        _ => Ok(None),
    }
}

/// `test(name, body)`: begin (returns whether the case runs, applying the
/// `OSPREY_TEST_FILTER` skip [TESTING-FILTER]), branch around the invoked
/// body, end (prints the TAP result line).
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
    let _ = gen_expr(cg, &invoke)?;
    cg.call_void("osp_test_end", "i8*", &[&name_str.operand]);
    let _ = cg.snapshot_to(&end_bb);
    cg.start_block(&end_bb);
    Ok(Value::unit())
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
