//! Expression lowering — the type-driven walk dispatching on each AST node.
//! Every node returns a [`Value`] carrying its LLVM type, seeded by inference
//! (`osprey-types`) for the things a local walk cannot know: function parameter
//! and return types. Unsupported nodes fail loudly via
//! [`CodegenError::Unsupported`] rather than miscompiling.

use crate::builder::{Codegen, FnSig};
use crate::conv::{as_double, as_i1, as_i64};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use crate::pattern::gen_match;
use crate::runtime::{gen_print, to_string_value};
use osprey_ast::{Expr, InterpolatedPart, NamedArgument, Parameter, Position, Stmt};

mod operators;
use operators::{checked_intrinsic, gen_binary, gen_checked_arith, gen_unary, two_int_args};
pub(crate) use operators::{emit_overflow_arith, gen_comparison};
mod calls;
use calls::{gen_call, gen_method_call};
pub(crate) use calls::{unapplied, with_application};
mod lambdas;
use lambdas::{apply_bound_lambda, apply_lambda, with_lambda_captures};
pub(crate) use lambdas::{apply_lambda_values, fit_lambda_return, inline_sig, reduce_lambda};
mod arguments;
pub(crate) use arguments::call_with_values;
use arguments::{binary_integer_builtin, eval_arg, gen_user_call, is_binary_integer_builtin};

mod block;
mod identifier;
use block::gen_block;
pub(crate) use block::{gen_block_with, gen_body};

pub(crate) fn gen_expr(cg: &mut Codegen, expr: &Expr) -> Result<Value> {
    let inferred = match expr {
        Expr::Integer(_) => Some(osprey_types::Type::con(
            osprey_types::names::INT,
            Vec::new(),
        )),
        Expr::Float(_) => Some(osprey_types::Type::con(
            osprey_types::names::FLOAT,
            Vec::new(),
        )),
        Expr::Bool(_) => Some(osprey_types::Type::con(
            osprey_types::names::BOOL,
            Vec::new(),
        )),
        Expr::Str(_) | Expr::InterpolatedStr(_) => Some(osprey_types::Type::con(
            osprey_types::names::STRING,
            Vec::new(),
        )),
        Expr::Call { function, .. } => cg.callee_fn_type(function).and_then(|ty| match ty {
            osprey_types::Type::Fun { ret, .. } => Some(*ret),
            _ => None,
        }),
        Expr::List(_, position) => cg
            .prog
            .list_elem_type(*position)
            .cloned()
            .map(|element| osprey_types::Type::con(osprey_types::names::LIST, vec![element])),
        Expr::Perform { position, .. } => position
            .and_then(|p| cg.prog.performs.get(&(p.line, p.column)))
            .map(|site| site.op.ret.clone()),
        _ => None,
    };
    let folded = osprey_types::fold_arithmetic(expr).map_err(CodegenError::invalid)?;
    let mut value = gen_expr_raw(cg, folded.as_ref().map_or(expr, |value| value))?;
    if let Some(ty) = inferred.filter(|ty| !osprey_types::has_type_var(ty)) {
        value.inferred_type = Some(ty);
    }
    Ok(value)
}

fn gen_expr_raw(cg: &mut Codegen, expr: &Expr) -> Result<Value> {
    match expr {
        Expr::Integer(n) => Ok(Value::new(n.to_string(), LType::I64)),
        Expr::Float(f) => Ok(Value::new(fmt_double(*f), LType::Double)),
        Expr::Bool(b) => Ok(Value::new(if *b { "1" } else { "0" }, LType::I1)),
        Expr::Str(s) => Ok(cg.string_constant(s)),
        Expr::InterpolatedStr(parts) => gen_interpolation(cg, parts),
        Expr::Identifier(name) => identifier::gen(cg, name),
        Expr::TypeApply {
            function, position, ..
        } => match crate::builtin_values::intrinsic_value(cg, function, *position) {
            Some(value) => value,
            None => with_application(cg, *position, |cg| gen_expr(cg, function)),
        },
        Expr::Binary {
            op, left, right, ..
        } => gen_binary(cg, op, left, right),
        Expr::Unary { op, operand } => gen_unary(cg, op, operand),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => gen_call(cg, function, arguments, named_arguments),
        Expr::MethodCall {
            target,
            method,
            arguments,
            named_arguments,
        } => gen_method_call(cg, target, method, arguments, named_arguments),
        Expr::Match { value, arms } => gen_match(cg, value, arms),
        Expr::Block {
            statements,
            value,
            position,
        } => gen_block(cg, statements, value.as_deref(), *position),
        Expr::TypeConstructor { name, fields, .. } => {
            crate::aggregate::gen_constructor(cg, name, fields)
        }
        Expr::Update { record, fields } => crate::aggregate::gen_update(cg, record, fields),
        Expr::FieldAccess { target, field } => {
            crate::aggregate::gen_field_access(cg, target, field)
        }
        Expr::Object(fields) => crate::aggregate::gen_object(cg, fields),
        Expr::List(elements, position) => crate::listlit::gen_list(cg, elements, *position),
        Expr::Map(entries) => crate::collections::gen_map_literal(cg, entries),
        Expr::Index { target, index } => crate::listlit::gen_index(cg, target, index),
        Expr::Spawn(e) => crate::fiber::gen_spawn(cg, e),
        Expr::Await(e) => crate::fiber::gen_await(cg, e),
        Expr::Yield(e) => crate::fiber::gen_yield(cg, e.as_deref()),
        Expr::Send { channel, value } => crate::fiber::gen_send(cg, channel, value),
        Expr::Recv(e) => crate::fiber::gen_recv(cg, e),
        Expr::Select { arms } => crate::fiber::gen_select(cg, arms),
        Expr::Perform {
            effect,
            operation,
            arguments,
            position,
            ..
        } => crate::effects::gen_perform(cg, effect, operation, arguments, *position),
        Expr::Handler {
            stage: _,
            effect,
            arms,
            return_clause,
            body,
            position,
        } => {
            crate::effects::gen_handler(cg, effect, arms, body, return_clause.as_deref(), *position)
        }
        Expr::Resume(value) => crate::effects::gen_resume(cg, value.as_deref()),
        // A lambda in plain value position (returned, block tail, stored in a
        // field) becomes a closure cell, typed by inference.
        Expr::Lambda {
            parameters,
            body,
            position,
            ..
        } => crate::closure::lambda_value(cg, parameters, body, *position),
        other => Err(CodegenError::unsupported(describe(other))),
    }
}

/// A top-level function's RAW code pointer (`i8*`) — exclusively for C-runtime
/// callback slots (`spawnProcess`/`httpListen` handlers via `extern_call`),
/// where the C side calls back through a plain function-pointer cast and a
/// closure cell would be jumped into as code. The source type of the bitcast is
/// the function's exact emitted signature — built the same way
/// `gen_function`/`coerce_return` spelled its `define` — so the cast is
/// well-typed. Mirrors the handler-pointer bitcast in `effects::gen_perform`.
pub(crate) fn fn_pointer(cg: &mut Codegen, name: &str) -> Value {
    let fty = fn_ptr_type(cg, name);
    let reg = cg.emit_reg(format!("bitcast {fty} @{name} to i8*"));
    Value::new(reg, LType::Ptr)
}

/// The raw code pointer of an emitted callback INSTANTIATION
/// ([`crate::monofn::specialize_callback`]). The instantiation was emitted at
/// the builtin's declared callback type, so that type — not the generic
/// original's — is what spells the bitcast.
pub(crate) fn mono_fn_pointer(
    cg: &mut Codegen,
    symbol: &str,
    declared: &(Vec<osprey_types::Type>, osprey_types::Type),
) -> Value {
    let (param_types, ret_type) = declared;
    let params = crate::llty::comma_join(param_types, |t| {
        crate::builder::ParamSig::of(&cg.prog, t).ty.to_string()
    });
    let ret = crate::llty::ret_spelling(
        crate::types::ltype_of(ret_type),
        crate::types::result_inner(ret_type),
    );
    let reg = cg.emit_reg(format!("bitcast {ret} ({params})* @{symbol} to i8*"));
    Value::new(reg, LType::Ptr)
}

/// The LLVM function-pointer type spelling for a top-level function, e.g.
/// `i64 (i64, i64, i8*)*` — return type (a `{ T, i8 }*` Result block, or the
/// inferred scalar; `Unit` rides as `i64`) then its parameter type list.
fn fn_ptr_type(cg: &Codegen, name: &str) -> String {
    let params = crate::llty::comma_join(&cg.fn_param_ltypes(name).unwrap_or_default(), |t| {
        t.to_string()
    });
    format!("{} ({params})*", cg.fn_ret_spelling(name))
}

/// LLVM requires a decimal point or exponent in a `double` literal; render a
/// whole number as `N.0`.
fn fmt_double(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 {
        format!("{f:.1}")
    } else {
        // Hex float is the exact, locale-free spelling LLVM accepts.
        format!("0x{:016X}", f.to_bits())
    }
}

fn gen_interpolation(cg: &mut Codegen, parts: &[InterpolatedPart]) -> Result<Value> {
    let mut fmt = String::new();
    let mut args: Vec<String> = Vec::new();
    for part in parts {
        match part {
            InterpolatedPart::Text(t) => fmt.push_str(&t.replace('%', "%%")),
            InterpolatedPart::Expr(e) => {
                // Preserve Result's complete Success/Error rendering. Logging
                // or formatting must never erase a failure discriminant.
                let v = gen_expr(cg, e)?;
                let s = to_string_value(cg, v)?;
                fmt.push_str("%s");
                args.push(format!("i8* {}", s.operand));
            }
        }
    }
    // Measure, then format into an exactly-sized buffer. The single pass this
    // replaced `sprintf`d into a fixed 4 KiB block — ~4 KiB wasted on EVERY
    // interpolation (the dominant heap cost of any string-building program) and
    // a silent overflow past it. [STRING-INTERPOLATION]
    Ok(crate::runtime::format_sized(cg, &fmt, &args))
}

pub(crate) fn first_arg<'a>(arguments: &'a [Expr], named: &'a [NamedArgument]) -> Option<&'a Expr> {
    arguments
        .first()
        .or_else(|| named.first().map(|n| &n.value))
}

/// A call's argument expressions in call order — positional, or named in written
/// order — for callees with a fixed parameter list (runtime builtins, indirect
/// calls) that bind by position rather than reordering by parameter name.
pub(crate) fn arg_exprs<'a>(args: &'a [Expr], named: &'a [NamedArgument]) -> Vec<&'a Expr> {
    if named.is_empty() {
        args.iter().collect()
    } else {
        named.iter().map(|n| &n.value).collect()
    }
}

fn describe(expr: &Expr) -> String {
    let kind = match expr {
        Expr::List(..) => "list literal",
        Expr::Map(_) => "map literal",
        Expr::Object(_) => "object literal",
        Expr::Pipe { .. } => "pipe expression",
        Expr::FieldAccess { .. } => "field access",
        Expr::MethodCall { .. } => "method call",
        Expr::Index { .. } => "index expression",
        Expr::Lambda { .. } => "lambda",
        Expr::TypeConstructor { .. } => "type constructor",
        Expr::Update { .. } => "record update",
        Expr::Spawn(_) => "spawn",
        Expr::Await(_) => "await",
        Expr::Perform { .. } => "perform",
        Expr::Handler { .. } => "handler",
        _ => "expression",
    };
    kind.to_string()
}
