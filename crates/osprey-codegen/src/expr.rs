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
use osprey_ast::{Expr, InterpolatedPart, NamedArgument, Parameter, Stmt};

pub(crate) fn gen_expr(cg: &mut Codegen, expr: &Expr) -> Result<Value> {
    match expr {
        Expr::Integer(n) => Ok(Value::new(n.to_string(), LType::I64)),
        Expr::Float(f) => Ok(Value::new(fmt_double(*f), LType::Double)),
        Expr::Bool(b) => Ok(Value::new(if *b { "1" } else { "0" }, LType::I1)),
        Expr::Str(s) => Ok(cg.string_constant(s)),
        Expr::InterpolatedStr(parts) => gen_interpolation(cg, parts),
        // A handler-captured mutable promoted to a heap cell reads through a
        // `load`; checked before the scope lookup since a cell is not scope-bound.
        Expr::Identifier(name) if cg.cell_slots.contains_key(name) => match cg.cell_read(name) {
            Some(v) => Ok(v),
            None => Err(CodegenError::unknown(name)),
        },
        Expr::Identifier(name) => match cg.lookup(name) {
            Some(v) => Ok(v),
            // A file-scope binding read from inside a function body: the
            // enclosing `main` frame is not on this stack, so the value comes
            // from its module global ([`crate::globals`]).
            None if cg.module_globals.contains_key(name) => {
                crate::globals::read(cg, name).ok_or_else(|| CodegenError::unknown(name))
            }
            // A bare name that is a nullary constructor (`Active`, `Red`, …) is a
            // zero-field variant value.
            None if cg.is_ctor(name) => crate::aggregate::gen_constructor(cg, name, &[]),
            // A bare top-level function name used as a value becomes its closure
            // cell — the one function-value representation. (C-runtime callback
            // slots request a raw code pointer explicitly via `fn_pointer`.)
            None if cg.fn_params.contains_key(name) => crate::closure::named_fn_cell(cg, name),
            // A call alias (`let g = identity`) used as a value resolves to its
            // target's cell; a still-generic target bails loudly in
            // `named_fn_cell` when no consuming slot fixes its ABI.
            None => match cg.call_aliases.get(name).cloned() {
                Some(target) => crate::closure::named_fn_cell(cg, &target),
                None => Err(CodegenError::unknown(name)),
            },
        },
        Expr::Binary { op, left, right } => gen_binary(cg, op, left, right),
        Expr::Unary { op, operand } => gen_unary(cg, op, operand),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => gen_call(cg, function, arguments, named_arguments),
        Expr::Match { value, arms } => gen_match(cg, value, arms),
        Expr::Block { statements, value } => gen_block(cg, statements, value.as_deref()),
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
            body,
            position,
        } => crate::effects::gen_handler(cg, effect, arms, body, *position),
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

fn gen_block(cg: &mut Codegen, statements: &[Stmt], value: Option<&Expr>) -> Result<Value> {
    // A child scope preserves outer bindings across nested blocks [BLOCK-SCOPE].
    cg.push_scope();
    let result = (|| {
        for (i, s) in statements.iter().enumerate() {
            crate::stmt::gen_local_stmt(cg, s)?;
            // Last-use drops: names the continuation no longer references die
            // here, not at function end [GC-ARC-PERCEUS].
            crate::arc::release_dead_after(cg, statements.get(i + 1..).unwrap_or(&[]), value);
        }
        value.map_or_else(|| Ok(Value::unit()), |e| gen_expr(cg, e))
    })();
    cg.pop_scope();
    result
}

fn gen_binary(cg: &mut Codegen, op: &str, left: &Expr, right: &Expr) -> Result<Value> {
    if op == "&&" || op == "||" {
        return gen_short_circuit(cg, op, left, right);
    }

    let l = gen_expr(cg, left)?;
    let r = gen_expr(cg, right)?;
    match op {
        "+" | "-" | "*" | "/" | "%" => gen_arith_propagating(cg, op, l, r),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
            if l.result_inner.is_some() || r.result_inner.is_some() {
                return Err(CodegenError::invalid("cannot compare an unhandled Result"));
            }
            gen_comparison(cg, op, l, r)
        }
        other => Err(CodegenError::unsupported(format!(
            "binary operator `{other}`"
        ))),
    }
}

fn gen_short_circuit(cg: &mut Codegen, op: &str, left: &Expr, right: &Expr) -> Result<Value> {
    // Branch before lowering the right operand [BOOL-SHORT-CIRCUIT].
    let (left, short, end) = open_short_circuit(cg, op, left)?;
    let right_value = gen_expr(cg, right)?;
    let right = as_i1(cg, right_value)?;
    let instruction = if op == "&&" { "and" } else { "or" };
    let combined = cg.emit_reg(format!("{instruction} i1 {left}, {}", right.operand));
    let right_block = cg.cur_block().to_string();
    cg.emit(format!("br label %{end}"));
    cg.start_block(&end);
    let short_value = if op == "&&" { "0" } else { "1" };
    let value = cg.emit_reg(format!(
        "phi i1 [ {short_value}, %{short} ], [ {combined}, %{right_block} ]"
    ));
    Ok(Value::new(value, LType::I1))
}

fn open_short_circuit(cg: &mut Codegen, op: &str, left: &Expr) -> Result<(String, String, String)> {
    let left_value = gen_expr(cg, left)?;
    let left = as_i1(cg, left_value)?;
    let (rhs, short, end) = (cg.fresh_label(), cg.fresh_label(), cg.fresh_label());
    let targets = if op == "&&" {
        (&rhs, &short)
    } else {
        (&short, &rhs)
    };
    cg.emit(format!(
        "br i1 {}, label %{}, label %{}",
        left.operand, targets.0, targets.1
    ));
    cg.start_block(&short);
    cg.emit(format!("br label %{end}"));
    cg.start_block(&rhs);
    Ok((left.operand, short, end))
}

/// Arithmetic whose operands may themselves carry an error channel. The
/// enclosing expression is ONE `Result`: an erroring operand makes the whole
/// expression `Error` instead of contributing a fabricated success payload, so
/// `(10 / 0) + 1.0` is `Error(division by zero)` and not `Success(1.0)`.
/// With no `Result` operand this is exactly [`gen_arith`].
fn gen_arith_propagating(cg: &mut Codegen, op: &str, l: Value, r: Value) -> Result<Value> {
    let Some((bad, msg)) = operand_error(cg, &l, &r) else {
        return gen_arith(cg, op, l, r);
    };
    gen_propagated_result(cg, &bad, &msg, |cg| {
        let lv = crate::result::unwrap(cg, l);
        let rv = crate::result::unwrap(cg, r);
        gen_arith(cg, op, lv, rv)
    })
}

/// The "an operand already failed" flag and the message to carry onward, or
/// `None` when neither operand has an error channel and no propagation is
/// needed. The left operand's message wins when both failed — it failed first.
fn operand_error(cg: &mut Codegen, l: &Value, r: &Value) -> Option<(String, String)> {
    if l.result_inner.is_none() && r.result_inner.is_none() {
        return None;
    }
    let lbad = result_failed(cg, l);
    let rbad = result_failed(cg, r);
    let bad = cg.emit_reg(format!("or i1 {lbad}, {rbad}"));
    let lmsg = crate::result::load_errmsg(cg, l);
    let rmsg = crate::result::load_errmsg(cg, r);
    let msg = cg.emit_reg(format!(
        "select i1 {lbad}, i8* {}, i8* {}",
        lmsg.operand, rmsg.operand
    ));
    Some((bad, msg))
}

/// `true` when `v` is a Result holding an Error; the constant `false` for a
/// value with no error channel at all.
fn result_failed(cg: &mut Codegen, v: &Value) -> String {
    if v.result_inner.is_none() {
        return "false".to_owned();
    }
    let disc = crate::result::load_disc(cg, v);
    cg.emit_reg(format!("icmp ne i8 {disc}, 0"))
}

/// The typed zero literal for the unread payload slot of an `Error` block.
/// Arithmetic. Float if either operand is a float (the other is promoted),
/// otherwise integer. Division ALWAYS returns float (the Osprey spec); modulo
/// stays integer.
fn gen_arith(cg: &mut Codegen, op: &str, l: Value, r: Value) -> Result<Value> {
    // `+` on list handles is concatenation (`a + b` ≡ `listConcat(a, b)`); on
    // map handles it is a right-biased merge (`a + b` ≡ `mapMerge(a, b)`).
    // Either operand carrying the owner tag selects the collection meaning.
    //
    // A flat list *literal* means the same thing but is a different layout, so
    // each operand is normalised to a runtime list first (a no-op for one that
    // already is). Without that, `xs + [1]` handed `osprey_list_concat` the
    // literal's foreign `{ i64, i8* }` header and **segfaulted**, while
    // `[1] + [2]` — where neither operand carries the owner tag — fell past this
    // arm into integer arithmetic and failed with "expected an integer".
    if op == "+" {
        let list_like = |v: &Value| {
            v.osp_ty
                .as_deref()
                .is_some_and(crate::collections::is_list_owner)
                || crate::listlit::is_lit(v)
        };
        if list_like(&l) || list_like(&r) {
            let l = crate::listlit::to_runtime_list(cg, l);
            let r = crate::listlit::to_runtime_list(cg, r);
            return Ok(crate::collections::concat_handles(cg, &l, &r));
        }
        let map_like = |v: &Value| {
            v.osp_ty
                .as_deref()
                .is_some_and(crate::collections::is_map_owner)
        };
        if map_like(&l) || map_like(&r) {
            return Ok(crate::collections::merge_handles(cg, &l, &r));
        }
    }
    // `+` with a string operand is concatenation: osp_strlen/strcpy/strcat
    // into a fresh malloc'd buffer. [BUILTIN-STRING-CONCAT]
    if op == "+" && (l.ty == LType::Str || r.ty == LType::Str) {
        return gen_str_concat(cg, l, r);
    }
    // `/` and `%` can be handed a divisor with no representable result, so they
    // are typed `Result<…, MathError>` and build a Success/Error block. The
    // Fallible operators preserve the wrapper until explicit handling.
    if op == "/" {
        return gen_division(cg, l, r);
    }
    if op == "%" {
        return gen_remainder(cg, l, r);
    }
    // IEEE-754 arithmetic stays plain. Integer `+ - *` below are fallible and
    // return a Result when their exact mathematical result is outside i64.
    if l.ty == LType::Double || r.ty == LType::Double {
        let ld = as_double(cg, l)?;
        let rd = as_double(cg, r)?;
        let opc = match op {
            "+" => "fadd",
            "-" => "fsub",
            _ => "fmul",
        };
        let reg = cg.emit_reg(format!("{opc} double {}, {}", ld.operand, rd.operand));
        return Ok(Value::new(reg, LType::Double));
    }
    let intrinsic = match op {
        "+" => "sadd",
        "-" => "ssub",
        _ => "smul",
    };
    gen_checked_arith(cg, intrinsic, l, r)
}

/// The `/` operator — always float, divide-by-zero checked.
fn gen_division(cg: &mut Codegen, l: Value, r: Value) -> Result<Value> {
    let ld = as_double(cg, l)?;
    let rd = as_double(cg, r)?;
    gen_zero_checked(
        cg,
        &ld.operand,
        &rd.operand,
        LType::Double,
        "fdiv double",
        "fcmp oeq double",
        "0.0",
    )
}

/// The `%` operator — remainder, modulo-by-zero checked.
/// `int % int` stays `int`-valued; a float operand promotes both to `float`.
/// The guard replaces a bare `srem`, whose behaviour on a zero divisor is
/// undefined.
fn gen_remainder(cg: &mut Codegen, l: Value, r: Value) -> Result<Value> {
    if l.ty == LType::Double || r.ty == LType::Double {
        let ld = as_double(cg, l)?;
        let rd = as_double(cg, r)?;
        return gen_zero_checked(
            cg,
            &ld.operand,
            &rd.operand,
            LType::Double,
            "frem double",
            "fcmp oeq double",
            "0.0",
        );
    }
    // LLVM `srem INT64_MIN, -1` is poison even though the mathematical
    // remainder is representable as zero. Substitute divisor 1 for that pair;
    // both remainders are zero, preserving the defined result without executing
    // undefined behaviour.
    let (li, ri, div_zero, overflow_pair) = i64_div_guards(cg, l, r)?;
    let safe_divisor = cg.emit_reg(format!(
        "select i1 {overflow_pair}, i64 1, i64 {}",
        ri.operand
    ));
    let remainder = format!("srem i64 {}, {safe_divisor}", li.operand);
    gen_guarded(cg, &div_zero, LType::I64, "0", DIVIDE_BY_ZERO, |cg| {
        cg.emit_reg(remainder)
    })
}

/// The `intDiv(a, b)` builtin — truncating integer division, divide-by-zero
/// checked. The integer sibling of `/` (which the spec fixes to float).
/// Implements [BUILTIN-INTDIV].
fn gen_int_division(cg: &mut Codegen, l: Value, r: Value) -> Result<Value> {
    let (li, ri, div_zero, overflow) = i64_div_guards(cg, l, r)?;
    let invalid = cg.emit_reg(format!("or i1 {div_zero}, {overflow}"));
    let zero_message = cg.string_constant(DIVIDE_BY_ZERO);
    let overflow_message = cg.string_constant("integer overflow");
    let message = cg.emit_reg(format!(
        "select i1 {div_zero}, i8* {}, i8* {}",
        zero_message.operand, overflow_message.operand
    ));
    gen_guarded_with_message(cg, &invalid, LType::I64, "0", &message, |cg| {
        cg.emit_reg(format!("sdiv i64 {}, {}", li.operand, ri.operand))
    })
}

/// Load both operands as `i64` and emit the two guards every checked integer
/// division shares: `div_zero` (`ri == 0`) and `overflow` (the `INT64_MIN ÷ -1`
/// pair whose `sdiv`/`srem` is LLVM poison). Emit order matches the sequence
/// `%` and intDiv both hand-wrote, so register numbering is unchanged.
fn i64_div_guards(cg: &mut Codegen, l: Value, r: Value) -> Result<(Value, Value, String, String)> {
    let li = as_i64(cg, l)?;
    let ri = as_i64(cg, r)?;
    let div_zero = cg.emit_reg(format!("icmp eq i64 {}, 0", ri.operand));
    let lhs_min = cg.emit_reg(format!("icmp eq i64 {}, -9223372036854775808", li.operand));
    let rhs_neg_one = cg.emit_reg(format!("icmp eq i64 {}, -1", ri.operand));
    let overflow = cg.emit_reg(format!("and i1 {lhs_min}, {rhs_neg_one}"));
    Ok((li, ri, div_zero, overflow))
}

/// `abs(n: int)`, lowered in the language's i64 ABI instead of falling through
/// to libc's `int abs(int)`. The one unrepresentable magnitude (`INT64_MIN`)
/// returns `Error(integer overflow)`. [BUILTIN-ABS]
fn gen_abs(cg: &mut Codegen, argument: &Expr) -> Result<Value> {
    let value = gen_expr(cg, argument)?;
    gen_unary_propagating(cg, value, gen_abs_value)
}

/// Absolute value for an already-unwrapped integer operand.
fn gen_abs_value(cg: &mut Codegen, value: Value) -> Result<Value> {
    let value = as_i64(cg, value)?;
    let negative = cg.emit_reg(format!("icmp slt i64 {}, 0", value.operand));
    let (negated, overflow) =
        emit_overflow_arith(cg, "ssub", Value::new("0", LType::I64), value.clone())?;
    gen_guarded(cg, &overflow, LType::I64, "0", INTEGER_OVERFLOW, |cg| {
        let negated = cg.emit_reg(negated);
        cg.emit_reg(format!(
            "select i1 {negative}, i64 {negated}, i64 {}",
            value.operand
        ))
    })
}

/// Shared zero-divisor skeleton for `/`, `%` and `intDiv`: a zero divisor
/// yields `Error` (`Result<_, MathError>` disc 1), else `Success(result)`.
/// `div`/`cmp` carry their LLVM type, `zero` is the typed zero literal.
fn gen_zero_checked(
    cg: &mut Codegen,
    lop: &str,
    rop: &str,
    inner: LType,
    div: &str,
    cmp: &str,
    zero: &str,
) -> Result<Value> {
    let bad = cg.emit_reg(format!("{cmp} {rop}, {zero}"));
    let quotient = format!("{div} {lop}, {rop}");
    gen_guarded(cg, &bad, inner, zero, DIVIDE_BY_ZERO, |cg| {
        cg.emit_reg(quotient)
    })
}

/// The one `MathError` reason for a zero divisor, shared by `/`, `%` and
/// `intDiv` so the three never drift apart in golden output.
const DIVIDE_BY_ZERO: &str = "division by zero";
const INTEGER_OVERFLOW: &str = "integer overflow";

/// Checked integer arithmetic shared by the `+ - *` operators and the
/// `checkedAdd` / `checkedSub` / `checkedMul` compatibility builtins.
/// `llvm.s{add,sub,mul}.with.overflow.i64` returns the wrapped
/// value paired with an overflow bit; the bit selects `Error`.
fn gen_checked_arith(cg: &mut Codegen, intrinsic: &str, l: Value, r: Value) -> Result<Value> {
    let (wrapped, bad) = emit_overflow_arith(cg, intrinsic, l, r)?;
    gen_guarded(cg, &bad, LType::I64, "0", INTEGER_OVERFLOW, |cg| {
        cg.emit_reg(wrapped)
    })
}

/// Emit one LLVM signed-overflow intrinsic and return its wrapped-value
/// extraction instruction plus the overflow flag. Callers decide how the
/// successful value is shaped before joining it with the Error path.
fn emit_overflow_arith(
    cg: &mut Codegen,
    intrinsic: &str,
    l: Value,
    r: Value,
) -> Result<(String, String)> {
    const PAIR: &str = "{ i64, i1 }";
    let li = as_i64(cg, l)?;
    let ri = as_i64(cg, r)?;
    cg.add_extern(format!(
        "declare {PAIR} @llvm.{intrinsic}.with.overflow.i64(i64, i64)"
    ));
    let pair = cg.emit_reg(format!(
        "call {PAIR} @llvm.{intrinsic}.with.overflow.i64(i64 {}, i64 {})",
        li.operand, ri.operand
    ));
    let bad = cg.emit_reg(format!("extractvalue {PAIR} {pair}, 1"));
    let wrapped = format!("extractvalue {PAIR} {pair}, 0");
    Ok((wrapped, bad))
}

/// The LLVM overflow-intrinsic stem behind each `checked*` builtin.
fn checked_intrinsic(name: &str) -> &'static str {
    match name {
        "checkedSub" => "ssub",
        "checkedMul" => "smul",
        _ => "sadd",
    }
}

/// The two evaluated plain operands of a binary integer builtin. The checker
/// normally proves this shape; the backend also rejects an unhandled Result
/// instead of extracting its success slot.
fn two_int_args(
    cg: &mut Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<(Value, Value)> {
    let args = arg_exprs(arguments, named);
    let missing = || CodegenError::invalid(format!("{name} needs two arguments"));
    let (an, bn) = (
        args.first().ok_or_else(missing)?,
        args.get(1).ok_or_else(missing)?,
    );
    let l = gen_expr(cg, an)?;
    let l = crate::cast::coerce_to(cg, l, LType::I64)?;
    let r = gen_expr(cg, bn)?;
    let r = crate::cast::coerce_to(cg, r, LType::I64)?;
    Ok((l, r))
}

/// The Success/Error join every guarded arithmetic builtin shares: `bad`
/// selects the error path carrying `message`, otherwise `ok_value` runs and its
/// register becomes the `Success` payload. `zero` is the typed zero the error
/// block stores in the unread payload slot.
fn gen_guarded(
    cg: &mut Codegen,
    bad: &str,
    inner: LType,
    zero: &str,
    message: &str,
    ok_value: impl FnOnce(&mut Codegen) -> String,
) -> Result<Value> {
    let msg = cg.string_constant(message);
    gen_guarded_with_message(cg, bad, inner, zero, &msg.operand, ok_value)
}

/// [`gen_guarded`] with a precomputed error-message operand. This lets a
/// guarded operation select a precise reason before branching (notably
/// [BUILTIN-INTDIV]'s zero-divisor and signed-overflow cases).
fn gen_guarded_with_message(
    cg: &mut Codegen,
    bad: &str,
    inner: LType,
    zero: &str,
    message: &str,
    ok_value: impl FnOnce(&mut Codegen) -> String,
) -> Result<Value> {
    use crate::result::{make_result, NO_MSG};
    let guard = open_result_guard(cg, bad);
    let value = ok_value(cg);
    let ok = make_result(cg, Value::new(value, inner), inner, "0", NO_MSG)?;
    finish_result_guard(cg, &guard, &ok, Value::new(zero, inner), message)
}

struct ResultGuard {
    bad: String,
    end: String,
    /// Ownership-ledger depth before either arm allocated its `Result` block,
    /// so the join can tell the two arm-produced owners from values that
    /// already existed. [GC-ARC-PERCEUS]
    mark: usize,
}

fn open_result_guard(cg: &mut Codegen, bad: &str) -> ResultGuard {
    let mark = crate::arc::frame_mark(cg);
    let (bad_block, good_block, end) = (cg.fresh_label(), cg.fresh_label(), cg.fresh_label());
    cg.emit(format!(
        "br i1 {bad}, label %{bad_block}, label %{good_block}"
    ));
    cg.start_block(&good_block);
    // Everything emitted until the join runs on one path only, so owners that
    // existed before the branch must not be retired at a use inside it.
    crate::arc::open_conditional(cg, mark);
    ResultGuard {
        bad: bad_block,
        end,
        mark,
    }
}

fn gen_propagated_result(
    cg: &mut Codegen,
    bad: &str,
    message: &str,
    success: impl FnOnce(&mut Codegen) -> Result<Value>,
) -> Result<Value> {
    let guard = open_result_guard(cg, bad);
    let value = success(cg)?;
    let (ok, inner) = if let Some(inner) = value.result_inner {
        (value, inner)
    } else {
        let inner = value.ty;
        (crate::result::make_ok(cg, value, inner)?, inner)
    };
    let zero = Value::new(crate::llty::zero_literal(inner), inner);
    finish_result_guard(cg, &guard, &ok, zero, message)
}

fn finish_result_guard(
    cg: &mut Codegen,
    guard: &ResultGuard,
    ok: &Value,
    zero: Value,
    message: &str,
) -> Result<Value> {
    let inner = zero.ty;
    let ok_block = cg.snapshot_to(&guard.end);
    cg.start_block(&guard.bad);
    let err = crate::result::make_result(cg, zero, inner, "1", message)?;
    let err_block = cg.snapshot_to(&guard.end);
    cg.start_block(&guard.end);
    crate::arc::close_conditional(cg);
    let reg = cg.emit_reg(format!(
        "phi {0}* [ {1}, %{ok_block} ], [ {2}, %{err_block} ]",
        crate::llty::result_struct_ty(inner),
        ok.operand,
        err.operand
    ));
    let out = Value::result(reg, inner);
    // Exactly one arm allocates on any path, so the join owns one block rather
    // than two. Merging them here is what lets an immediate unwrap retire the
    // block at its use (`consume_fresh`) instead of at region end — the latter
    // sinks the drop past any call in the same expression and would cost a
    // self-call its tail position. [GC-ARC-PERCEUS]
    crate::arc::move_phi_owners(
        cg,
        &[ok.operand.clone(), err.operand.clone()],
        &out,
        guard.mark,
    );
    Ok(out)
}

/// String concatenation: `malloc(osp_strlen a + osp_strlen b + 1)` then
/// `strcpy`+`strcat`, promoting a non-string operand through `toString` first.
/// Length comes from the runtime's `osp_strlen` (returns `i64` on every target)
/// rather than libc `strlen` (returns `size_t`, which is 32-bit on wasm32) so
/// the emitted IR is pointer-width-stable. [BUILTIN-STRING-LENGTH]
fn gen_str_concat(cg: &mut Codegen, l: Value, r: Value) -> Result<Value> {
    let ls = to_string_value(cg, l)?;
    let rs = to_string_value(cg, r)?;
    let ll = cg.call("i64", "osp_strlen", "i8*", &[&ls.operand]);
    let rl = cg.call("i64", "osp_strlen", "i8*", &[&rs.operand]);
    let sum = cg.emit_reg(format!("add i64 {ll}, {rl}"));
    let total = cg.emit_reg(format!("add i64 {sum}, 1"));
    let buf = cg.heap_alloc(&total);
    let _ = cg.call("i8*", "strcpy", "i8*, i8*", &[&buf, &ls.operand]);
    let _ = cg.call("i8*", "strcat", "i8*, i8*", &[&buf, &rs.operand]);
    let v = Value::new(buf, LType::Str);
    crate::arc::own(cg, &v);
    Ok(v)
}

/// The LLVM condition code for a comparison `op`. `float` picks the ordered
/// `fcmp` codes (`oeq`, `olt`, …); otherwise the signed-integer / `icmp` codes
/// (`eq`, `slt`, …) — also used on a `strcmp` result.
fn cmp_code(op: &str, float: bool) -> &'static str {
    match (op, float) {
        ("==", false) => "eq",
        ("!=", false) => "ne",
        ("<", false) => "slt",
        ("<=", false) => "sle",
        (">", false) => "sgt",
        (_, false) => "sge",
        ("==", true) => "oeq",
        ("!=", true) => "one",
        ("<", true) => "olt",
        ("<=", true) => "ole",
        (">", true) => "ogt",
        (_, true) => "oge",
    }
}

pub(crate) fn gen_comparison(cg: &mut Codegen, op: &str, l: Value, r: Value) -> Result<Value> {
    let reg = cg.fresh_reg();
    let is_str = |t: LType| t == LType::Str || t == LType::Ptr;
    if is_str(l.ty) && is_str(r.ty) {
        let c = cg.call("i32", "strcmp", "i8*, i8*", &[&l.operand, &r.operand]);
        cg.emit(format!("{reg} = icmp {} i32 {c}, 0", cmp_code(op, false)));
        return Ok(Value::new(reg, LType::I1));
    }
    if l.ty == LType::Double || r.ty == LType::Double {
        let ld = as_double(cg, l)?;
        let rd = as_double(cg, r)?;
        cg.emit(format!(
            "{reg} = fcmp {} double {}, {}",
            cmp_code(op, true),
            ld.operand,
            rd.operand
        ));
        return Ok(Value::new(reg, LType::I1));
    }
    let cc = cmp_code(op, false);
    let li = as_i64(cg, l)?;
    let ri = as_i64(cg, r)?;
    cg.emit(format!(
        "{reg} = icmp {cc} i64 {}, {}",
        li.operand, ri.operand
    ));
    Ok(Value::new(reg, LType::I1))
}

fn gen_unary(cg: &mut Codegen, op: &str, operand: &Expr) -> Result<Value> {
    let v = gen_expr(cg, operand)?;
    match op {
        "-" => gen_unary_propagating(cg, v, gen_negated_value),
        "!" => {
            let b = as_i1(cg, v)?;
            Ok(Value::new(
                cg.emit_reg(format!("xor i1 {}, true", b.operand)),
                LType::I1,
            ))
        }
        other => Err(CodegenError::unsupported(format!(
            "unary operator `{other}`"
        ))),
    }
}

/// Preserve an operand's existing error channel across a unary operation. The
/// operation runs only on Success; its own Result is used directly, while a
/// plain result is wrapped so the inherited Error and Success paths share one
/// flattened Result.
fn gen_unary_propagating(
    cg: &mut Codegen,
    value: Value,
    operation: fn(&mut Codegen, Value) -> Result<Value>,
) -> Result<Value> {
    if value.result_inner.is_none() {
        return operation(cg, value);
    }

    let bad = result_failed(cg, &value);
    let msg = crate::result::load_errmsg(cg, &value);
    gen_propagated_result(cg, &bad, &msg.operand, |cg| {
        let payload = crate::result::unwrap(cg, value);
        operation(cg, payload)
    })
}

/// Unary numeric negation on an already-unwrapped value. IEEE-754 negation is
/// total and remains plain; integer negation reports the `INT64_MIN` overflow.
fn gen_negated_value(cg: &mut Codegen, value: Value) -> Result<Value> {
    if value.ty == LType::Double {
        return Ok(Value::new(
            cg.emit_reg(format!("fneg double {}", value.operand)),
            LType::Double,
        ));
    }
    gen_checked_arith(cg, "ssub", Value::new("0", LType::I64), value)
}

/// One runtime-builtin dispatcher: returns `None` when `name` is not its
/// builtin, so a chain of them falls through to a user call.
type BuiltinDispatch = fn(&mut Codegen, &str, &[Expr], &[NamedArgument]) -> Result<Option<Value>>;

/// The fiber dispatcher predates named arguments and takes none; this adapts it
/// to the shared shape rather than making the table's element type optional.
fn gen_fiber_builtin(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Option<Value>> {
    crate::fiber::gen_builtin(cg, name, args)
}

/// Runtime builtin dispatchers IN RESOLUTION ORDER. A bare name shared by the
/// string and collection runtimes resolves on the RECEIVER, so
/// `gen_receiver_directed` must come before the name-keyed string/collection
/// dispatchers — reordering this table changes which builtin a shared name
/// means.
const BUILTIN_DISPATCH: [BuiltinDispatch; 8] = [
    crate::testing::gen,
    crate::collections::gen_receiver_directed,
    crate::strings::gen,
    crate::collections::gen,
    crate::iter::gen,
    crate::gpu::gen,
    gen_fiber_builtin,
    crate::extern_call::gen,
];

fn gen_call(
    cg: &mut Codegen,
    function: &Expr,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    // A directly-applied lambda (`x |> fn(y) => …`, `(fn(y) => …)(x)`) is
    // beta-reduced inline.
    if let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = function
    {
        return apply_lambda(cg, parameters, body, *position, arguments);
    }
    // `applyCurried g 3 4` — the callee is an application spine headed by a
    // GENERIC user function, whose intermediate lambdas exist only inside an
    // inlined specialisation and have no closure ABI to materialise
    // [FLAVOR-ML-CURRY]. This runs before the closure-cell path below because
    // that path would build exactly the cell that cannot exist.
    if let Some(v) = crate::curry::try_spine(cg, function, arguments, named)? {
        return Ok(v);
    }
    // `makeAdder(5)(3)` — the callee is itself a call producing a function
    // value: evaluate it to a closure handle and call through the cell.
    if let Expr::Call {
        function: inner, ..
    } = function
    {
        if let Expr::Identifier(f) = &**inner {
            let sig = cg
                .call_result_fn_type(f)
                .as_ref()
                .and_then(|t| Codegen::fn_value_sig(&cg.prog, t));
            if let Some(sig) = sig {
                return call_fn_value(cg, function, &sig, arguments, named);
            }
        }
    }
    let Expr::Identifier(ident) = function else {
        // A higher-order callee that is an arbitrary expression — a chained
        // application (`add3(1)(2)(3)`) or a function held in a record field.
        // Recover its signature from the type table and dispatch through the
        // closure cell; fail loudly only when the callee is not a function value.
        if let Some(sig) = cg
            .callee_fn_type(function)
            .as_ref()
            .and_then(|t| Codegen::fn_value_sig(&cg.prog, t))
        {
            return call_fn_value(cg, function, &sig, arguments, named);
        }
        return Err(CodegenError::unsupported("indirect / higher-order call"));
    };
    // A function-valued parameter (bound while inlining a generic function)
    // redirects to its real callee, so `f(x)` becomes `toString(x)` / `addOne(x)`.
    let name: String = cg
        .call_aliases
        .get(ident)
        .cloned()
        .unwrap_or_else(|| ident.clone());
    let name = name.as_str();
    // A call through a function-typed local (`f(x)` where `f` holds a closure
    // cell) goes through the cell FIRST — the cell snapshots captures at
    // creation, the one capture semantics. The beta-reduction fast path below
    // only serves lambdas that never materialized as a value.
    if let Some(v) = crate::genfn::try_indirect(cg, name, arguments, named)? {
        return Ok(v);
    }
    // A let-bound lambda with no materialized cell is inlined at its call site.
    if let Some((params, body, position)) = cg.lambda_def(name).cloned() {
        return apply_lambda(cg, &params, &body, position, arguments);
    }
    match name {
        "print" => {
            let arg = first_arg(arguments, named)
                .ok_or_else(|| CodegenError::invalid("print needs one argument"))?;
            let v = gen_expr(cg, arg)?;
            gen_print(cg, v)
        }
        "toString" => {
            let arg = first_arg(arguments, named)
                .ok_or_else(|| CodegenError::invalid("toString needs one argument"))?;
            let v = gen_expr(cg, arg)?;
            to_string_value(cg, v)
        }
        "abs" => {
            let arg = first_arg(arguments, named)
                .ok_or_else(|| CodegenError::invalid("abs needs one argument"))?;
            gen_abs(cg, arg)
        }
        "intDiv" => {
            let (l, r) = two_int_args(cg, name, arguments, named)?;
            gen_int_division(cg, l, r)
        }
        // [BUILTIN-TOFLOAT] [GPU-CONVERT] Widening int → float: one `sitofp`,
        // round-to-nearest-even, exact for |n| <= 2^53. Total, so no Result.
        "toFloat" => {
            let arg = first_arg(arguments, named)
                .ok_or_else(|| CodegenError::invalid("toFloat needs one argument"))?;
            let v = gen_expr(cg, arg)?;
            let n = crate::conv::as_i64(cg, v)?;
            crate::conv::as_double(cg, n)
        }
        // Compatibility names for the same checked integer operations used by
        // the natural operators.
        "checkedAdd" | "checkedSub" | "checkedMul" => {
            let (l, r) = two_int_args(cg, name, arguments, named)?;
            gen_checked_arith(cg, checked_intrinsic(name), l, r)
        }
        // Runtime builtins take precedence over a same-named user function: the
        // names below are reserved. Each dispatcher returns `None` when the name
        // is not its builtin, so the chain falls through to a user call.
        _ => {
            for dispatch in BUILTIN_DISPATCH {
                if let Some(v) = dispatch(cg, name, arguments, named)? {
                    return Ok(v);
                }
            }
            // A generic user function is specialised by inlining its body with
            // the concrete argument types at this call site.
            if let Some(v) = crate::genfn::try_inline(cg, name, arguments, named, &[])? {
                return Ok(v);
            }
            gen_user_call(cg, name, arguments, named)
        }
    }
}

/// Call through an evaluated function value: lower the callee expression to a
/// closure handle, coerce the arguments to the signature's parameter types,
/// and call through the cell.
fn call_fn_value(
    cg: &mut Codegen,
    callee: &Expr,
    sig: &FnSig,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    let handle = gen_expr(cg, callee)?;
    let exprs = arg_exprs(arguments, named);
    crate::closure::cell_call_exprs(cg, &handle.operand, sig, &exprs)
}

/// Beta-reduce a lambda at its application site: bind each parameter to its
/// argument and lower the body in a fresh scope. The returned value keeps its
/// complete inferred representation, including a Result wrapper.
fn apply_lambda(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    position: Option<osprey_ast::Position>,
    arguments: &[Expr],
) -> Result<Value> {
    let mut values = Vec::with_capacity(arguments.len());
    for a in arguments {
        values.push(gen_expr(cg, a)?);
    }
    apply_lambda_values(
        cg,
        parameters,
        body,
        values,
        inline_sig(cg, position).as_ref(),
        position,
    )
}

/// The signature to fit an INLINED lambda application to, or `None` when the
/// lambda is still generic.
///
/// Inference records ONE type per lambda position, so fitting a generic lambda
/// to it used whichever instantiation happened to be recorded: `let idl = |x|
/// => x` applied to an `int` and then to a `string` coerced the string into the
/// int slot, and the second call printed a pointer as a number. A generic
/// lambda is specialised by its ARGUMENTS at each call site instead, exactly as
/// a generic function is ([`crate::genfn`], [TYPE-GENERICS-FN]).
pub(crate) fn inline_sig(cg: &Codegen, position: Option<osprey_ast::Position>) -> Option<FnSig> {
    cg.prog
        .lambda_type(position)
        .filter(|t| crate::types::fn_value_concrete(t))
        .and_then(|t| Codegen::fn_value_sig(&cg.prog, t))
}

/// [`apply_lambda`] over already-evaluated argument values — shared with the
/// iterator builtins, which produce loop elements as values.
pub(crate) fn apply_lambda_values(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    values: Vec<Value>,
    sig: Option<&FnSig>,
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    reduce_lambda(cg, parameters, body, values, sig, &[], position)
}

/// [`apply_lambda_values`] with the groups of a curried application spine that
/// this lambda does not itself consume ([`crate::curry`]). While groups remain
/// the value produced is the spine's result rather than this lambda's return,
/// so this lambda's return adaptation is skipped until the last group.
pub(crate) fn reduce_lambda(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    values: Vec<Value>,
    sig: Option<&FnSig>,
    rest: &[crate::curry::ArgGroup<'_>],
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    cg.push_scope();
    // `fn_ptr_locals` is per-FUNCTION, not per-scope ([`Codegen::begin_function`]),
    // so a function-typed lambda parameter registered below must be unwound by
    // hand — exactly as an inlined call does ([`crate::genfn`]).
    let saved_fn_ptrs = cg.fn_ptr_locals.clone();
    let lowered = (|| {
        bind_lambda_params(cg, parameters, values, sig, position)?;
        let value = crate::curry::apply_groups(cg, body, rest)?;
        if rest.is_empty() {
            fit_lambda_return(cg, value, sig)
        } else {
            Ok(value)
        }
    })();
    cg.fn_ptr_locals = saved_fn_ptrs;
    cg.pop_scope();
    lowered
}

/// Bind a lambda's parameters to its argument values, coercing each to the
/// parameter type its signature declares.
fn bind_lambda_params(
    cg: &mut Codegen,
    parameters: &[Parameter],
    values: Vec<Value>,
    sig: Option<&FnSig>,
    position: Option<osprey_ast::Position>,
) -> Result<()> {
    let declared = cg.prog.lambda_type(position).cloned();
    for (index, (p, v)) in parameters.iter().zip(values).enumerate() {
        bind_lambda_fn_param(cg, &p.name, declared.as_ref(), index);
        let v = match sig.and_then(|s| s.0.get(index)) {
            Some(want) => crate::cast::coerce_semantic_param(cg, v, want)?,
            None => v,
        };
        cg.bind(p.name.clone(), v);
    }
    Ok(())
}

/// Register a function-typed lambda parameter so a call through it lowers to an
/// indirect call, the way a top-level function's own higher-order parameters are
/// registered ([`crate::lower`]).
///
/// Without this the curried ML head `feeding reading body = handle … in body ()`
/// — whose `body` is a LAMBDA parameter, not a function parameter — lowered
/// `body ()` to a direct call on an `@body` symbol nothing defines, and the
/// program failed at the LINKER. The tupled head `feeding (reading, body)` put
/// the same parameter on the function itself and so always worked
/// [FLAVOR-ML-CURRY], [FLAVOR-IR-EQUIV].
fn bind_lambda_fn_param(
    cg: &mut Codegen,
    name: &str,
    declared: Option<&osprey_types::Type>,
    index: usize,
) {
    let Some(osprey_types::Type::Fun { params, .. }) = declared else {
        return;
    };
    if let Some(ty @ osprey_types::Type::Fun { .. }) = params.get(index) {
        cg.bind_fn_local(name, ty.clone());
    }
}

/// Adapt a lambda body's value to the lambda's own inferred signature — the
/// shared tail of beta-reduction ([`apply_lambda_values`]) and kernel
/// extraction ([`crate::gpu_kernel`]), so the two lowerings of one lambda
/// produce the same value by construction [GPU-KERNEL-EXTRACT].
pub(crate) fn fit_lambda_return(
    cg: &mut Codegen,
    value: Value,
    sig: Option<&FnSig>,
) -> Result<Value> {
    // A lambda's return slot is typed by its signature, so a list literal
    // returned through a cell loses the flat tag [`crate::listlit::escaping`].
    let value = crate::listlit::escaping(cg, value);
    let value = match sig {
        Some((_, _, Some(inner), _, _)) if value.result_inner.is_some() => {
            crate::result::repack_to_inner(cg, value, *inner)?
        }
        Some((_, _, Some(inner), _, _)) => crate::result::make_ok(cg, value, *inner)?,
        Some((_, ret, None, _, _)) => crate::cast::coerce_to(cg, value, *ret)?,
        None => value,
    };
    Ok(match sig.and_then(|signature| signature.3.clone()) {
        Some(fiber) => fiber.restore(value),
        None => value,
    })
}

/// A call to a user-defined or runtime function. Parameter types come from
/// inference (so a string/float/bool parameter is passed in its real LLVM
/// type), as does the return type.
fn gen_user_call(
    cg: &mut Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    let args = ordered_args(cg, name, arguments, named)?;
    call_with_values(cg, name, args)
}

/// Lower a builtin that was passed as a first-class callback — `forEach(xs,
/// print)`, `gpuMap(toFloat)`. Each arm has a value form needing no argument
/// expressions, so it lowers once per element. `None` means `name` has no such
/// form. Implements [BUILTIN-ITER-CALLBACK].
fn call_builtin_with_values(cg: &mut Codegen, name: &str, args: &[Value]) -> Option<Result<Value>> {
    let arg = || args.first().cloned().unwrap_or_else(Value::unit);
    Some(match name {
        "print" => gen_print(cg, arg()),
        "toString" => to_string_value(cg, arg()),
        // [BUILTIN-TOFLOAT] [GPU-CONVERT] the canonical float-pipeline seed
        // `gpuIota(n) |> gpuMap(toFloat)` lowers through this arm.
        "toFloat" => as_i64(cg, arg()).and_then(|n| crate::conv::as_double(cg, n)),
        "abs" => gen_unary_propagating(cg, arg(), gen_abs_value),
        _ => return None,
    })
}

/// Call `name` with already-evaluated argument values — the shared tail of
/// `gen_user_call` and the iterator callbacks. Coerces each argument to the
/// inferred parameter type, declares unknown (runtime) callees, and tags a
/// `Result`-returning callee's value.
pub(crate) fn call_with_values(cg: &mut Codegen, name: &str, args: Vec<Value>) -> Result<Value> {
    // An intrinsic builtin has no emitted `@name` symbol, so one reaching this
    // path as a first-class callback lowers to its value form here.
    if let Some(v) = call_builtin_with_values(cg, name, &args) {
        return v;
    }
    // Coerce each argument to the declared parameter type where known.
    // A parameter slot is typed, not tagged: a list literal handed to a real
    // (non-inlined) callee arrives as `List<T>` [`crate::listlit::escaping`].
    let args: Vec<Value> = args
        .into_iter()
        .map(|a| crate::listlit::escaping(cg, a))
        .collect();
    let coerced = match cg.fn_param_abis(name) {
        Some(ptys) if ptys.len() == args.len() => args
            .into_iter()
            .zip(ptys)
            .map(|(a, want)| crate::cast::coerce_param(cg, a, &want))
            .collect::<Result<Vec<_>>>()?,
        _ => args,
    };
    let typed = crate::llty::comma_join(&coerced, Value::typed);
    // A function declared `-> Result<T, E>` hands back a Result block pointer.
    if let Some(inner) = cg.fn_ret_result_inner(name) {
        let rty = format!("{}*", crate::llty::result_struct_ty(inner));
        let reg = emit_user_call(cg, name, &rty, &coerced, &typed);
        let v = Value::result(reg, inner);
        // Callee epilogues transfer +1 on every return [GC-ARC-PERCEUS].
        crate::arc::own(cg, &v);
        return Ok(v);
    }
    let ret = cg.fn_ret_ltype(name).unwrap_or(LType::I64);
    let reg = emit_user_call(cg, name, ret.as_str(), &coerced, &typed);
    let v = Value::new(reg, ret).with_owner(cg.fn_ret_owner(name));
    let v = match cg.fn_ret_fiber_sig(name) {
        Some(fiber) => fiber.restore(v),
        None => v,
    };
    crate::arc::own(cg, &v);
    Ok(v)
}

/// Emit a call to `name` returning LLVM type `rty`. A name with no user
/// definition is a runtime builtin, so synthesize its `declare` (param types
/// from `coerced`) — the IR stays valid and links only if the symbol exists.
fn emit_user_call(
    cg: &mut Codegen,
    name: &str,
    rty: &str,
    coerced: &[Value],
    typed: &str,
) -> String {
    if !cg.fn_params.contains_key(name) {
        let sig = crate::llty::comma_join(coerced, Value::llvm_ty);
        cg.add_extern(format!("declare {rty} @{name}({sig})"));
    }
    cg.emit_reg(format!("call {rty} @{name}({typed})"))
}

fn ordered_args(
    cg: &mut Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Vec<Value>> {
    // The function-value signature of each declared parameter (if it is
    // function-typed), so an inline-lambda argument is lowered to that slot's
    // ABI rather than evaluated as a value. An EXTERN callee crosses the C
    // boundary: its function-typed slots take raw code pointers, not cells.
    let sigs: Vec<Option<FnSig>> = cg
        .prog
        .param_types(name)
        .map(|ts| {
            ts.iter()
                .map(|t| Codegen::fn_value_sig(&cg.prog, t))
                .collect()
        })
        .unwrap_or_default();
    let ffi = !cg.fn_params.contains_key(name) && cg.prog.functions.contains_key(name);
    if !named.is_empty() {
        if let Some(pnames) = cg.fn_params.get(name).cloned() {
            let mut out = Vec::new();
            for (i, pn) in pnames.iter().enumerate() {
                if let Some(na) = named.iter().find(|a| &a.name == pn) {
                    out.push(eval_arg(
                        cg,
                        &na.value,
                        sigs.get(i).and_then(Option::as_ref),
                        ffi,
                    )?);
                }
            }
            if out.len() == named.len() {
                return Ok(out);
            }
        }
        return named.iter().map(|na| gen_expr(cg, &na.value)).collect();
    }
    arguments
        .iter()
        .enumerate()
        .map(|(i, a)| eval_arg(cg, a, sigs.get(i).and_then(Option::as_ref), ffi))
        .collect()
}

/// Lower one call argument. A lambda flowing into a function-typed parameter
/// becomes a closure cell with the slot's ABI — except across the C boundary
/// (`ffi`), where the slot needs a raw code pointer: there a non-capturing
/// lambda lifts env-free and a named function takes its raw address.
/// Everything else goes through `gen_expr` (where a user function name becomes
/// its forwarder cell).
fn eval_arg(cg: &mut Codegen, expr: &Expr, sig: Option<&FnSig>, ffi: bool) -> Result<Value> {
    match (expr, sig) {
        (
            Expr::Lambda {
                parameters, body, ..
            },
            Some(sig),
        ) => {
            if ffi {
                crate::closure::raw_callback_lambda(cg, parameters, body, sig)
            } else {
                crate::closure::emit_closure(cg, parameters, body, sig)
            }
        }
        (Expr::Identifier(n), Some(sig)) if cg.lookup(n).is_none() => {
            // Resolve a call alias (`let g = identity`) to its real target.
            let target = cg.call_aliases.get(n).cloned().unwrap_or_else(|| n.clone());
            // A GENERIC named function flowing into a concrete function-typed
            // slot: specialise it to the slot's ABI — its (params, body) emit
            // exactly like a capture-free lambda. A monomorphic name keeps its
            // once-per-module forwarder cell via `gen_expr`/`named_fn_cell`.
            // Implements [TYPE-GENERICS-FN].
            if let Some((params, body)) = cg.fn_defs.get(&target).cloned() {
                return if ffi {
                    crate::closure::raw_callback_lambda(cg, &params, &body, sig)
                } else {
                    // Keyed by (function, slot ABI): every use at the same ABI
                    // lowers to a byte-identical body, so emit it once and
                    // share the cell. Distinct ABIs still get distinct bodies —
                    // that is what specialising means.
                    let key = crate::closure::specialisation_key(&target, sig);
                    crate::closure::emit_closure_keyed(cg, &params, &body, sig, Some(key))
                };
            }
            if ffi && cg.fn_params.contains_key(&target) {
                return Ok(fn_pointer(cg, &target));
            }
            let v = gen_expr(cg, expr)?;
            Ok(list_arg_as_handle(cg, v, ffi))
        }
        _ => {
            let v = gen_expr(cg, expr)?;
            Ok(list_arg_as_handle(cg, v, ffi))
        }
    }
}

/// Normalise a flat list-literal argument into an `OspreyList` handle before it
/// crosses into a callee (a no-op for every other value).
///
/// A callee cannot tell which list layout it was handed: its parameter is one
/// `i8*` either way, and inside the body the value carries the `List` owner tag
/// regardless of how the caller spelled it. `osprey_list_length` happens to read
/// both layouts — they share a leading `i64` — but `osprey_list_get` and
/// `osprey_list_drop` need the real trie, so a list pattern over a literal
/// argument **segfaulted**:
///
/// ```text
/// fn headOf(xs: List<int>) -> int = match xs { [] => -1  [h, ...t] => h }
/// print("${headOf([7, 8])}")            // SIGSEGV
/// print("${headOf(listAppend(List(), 7))}")   // fine
/// ```
///
/// Rebuilding at the boundary makes a parameter's representation independent of
/// the caller's spelling. Nothing regresses: a `List<T>` parameter cannot be
/// indexed in the first place (`xs[0]` on a parameter is
/// `index of a non-list/map value` for both spellings), and every `list*`
/// builtin, the receiver-directed `length`/`isEmpty`, and `+` all accept a
/// handle. An `extern` callee is skipped — a C signature taking a list is
/// outside this contract, so its argument is left exactly as written.
fn list_arg_as_handle(cg: &mut Codegen, v: Value, ffi: bool) -> Value {
    if ffi {
        return v;
    }
    crate::listlit::to_runtime_list(cg, v)
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
