//! Expression operators lowering.
use super::{
    arg_exprs, as_double, as_i1, as_i64, gen_expr, to_string_value, Codegen, CodegenError, Expr,
    LType, NamedArgument, Result, Value,
};

pub(super) fn gen_binary(cg: &mut Codegen, op: &str, left: &Expr, right: &Expr) -> Result<Value> {
    if op == "&&" || op == "||" {
        return gen_short_circuit(cg, op, left, right);
    }

    let l = gen_expr(cg, left)?;
    let r = gen_expr(cg, right)?;
    match op {
        "+" | "-" | "*" | "/" | "%" => gen_arith(cg, op, l, r),
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
    // `/` and `%` dispatch zero divisors to the active Arith policy. Their
    // value arms supply the numeric result without a Result wrapper.
    if op == "/" {
        return crate::arithmetic::division(cg, "/", l, r);
    }
    if op == "%" {
        return crate::arithmetic::remainder(cg, l, r);
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
    crate::arithmetic::binary(cg, op, intrinsic, l, r)
}

/// Explicit Result-producing `checkedAdd`, `checkedSub` and `checkedMul`.
/// `llvm.s{add,sub,mul}.with.overflow.i64` returns the wrapped
/// value paired with an overflow bit; the bit selects `Error`.
pub(super) fn gen_checked_arith(
    cg: &mut Codegen,
    intrinsic: &str,
    l: Value,
    r: Value,
) -> Result<Value> {
    let (wrapped, bad) = emit_overflow_arith(cg, intrinsic, l, r)?;
    gen_guarded(cg, &bad, LType::I64, "0", "integer overflow", |cg| {
        cg.emit_reg(wrapped)
    })
}

/// Emit one LLVM signed-overflow intrinsic and return its wrapped-value
/// extraction instruction plus the overflow flag. Callers decide how the
/// successful value is shaped before joining it with the Error path.
pub(crate) fn emit_overflow_arith(
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
pub(super) fn checked_intrinsic(name: &str) -> &'static str {
    match name {
        "checkedSub" => "ssub",
        "checkedMul" => "smul",
        _ => "sadd",
    }
}

/// The two evaluated plain operands of a binary integer builtin. The checker
/// normally proves this shape; the backend also rejects an unhandled Result
/// instead of extracting its success slot.
pub(super) fn two_int_args(
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
        crate::llty::RESULT_STRUCT,
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

/// [FLOAT-COMPARE] Float inequality includes unordered (NaN) operands so it
/// remains the complement of equality. The other float predicates stay ordered;
/// integer and string comparisons use signed `icmp` codes.
fn cmp_code(op: &str, float: bool) -> &'static str {
    match (op, float) {
        ("==", false) => "eq",
        ("!=", false) => "ne",
        ("<", false) => "slt",
        ("<=", false) => "sle",
        (">", false) => "sgt",
        (_, false) => "sge",
        ("==", true) => "oeq",
        ("!=", true) => "une",
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

pub(super) fn gen_unary(cg: &mut Codegen, op: &str, operand: &Expr) -> Result<Value> {
    let v = gen_expr(cg, operand)?;
    match op {
        "-" => crate::arithmetic::negation(cg, v),
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
