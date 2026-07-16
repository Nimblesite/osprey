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
        Expr::List(elements) => crate::listlit::gen_list(cg, elements),
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
    // A block does NOT open a new scope: locals live in a flat per-function
    // symbol table, so a nested `let` rebinds (and leaks) the name in the
    // enclosing scope. The goldens in `examples/tested` rely on this — e.g.
    // block_statements' inner `let outer` is visible to the outer `outer + inner`.
    for s in statements {
        crate::lower::gen_local_stmt(cg, s)?;
    }
    match value {
        Some(e) => gen_expr(cg, e),
        None => Ok(Value::unit()),
    }
}

fn gen_binary(cg: &mut Codegen, op: &str, left: &Expr, right: &Expr) -> Result<Value> {
    // Logical operators are control flow over booleans; keep them lazy-safe by
    // evaluating both sides (the lowered programs have pure operands).
    if op == "&&" || op == "||" {
        let l = gen_expr(cg, left)?;
        let r = gen_expr(cg, right)?;
        let lb = as_i1(cg, l)?;
        let rb = as_i1(cg, r)?;
        let opc = if op == "&&" { "and" } else { "or" };
        let reg = cg.fresh_reg();
        cg.emit(format!("{reg} = {opc} i1 {}, {}", lb.operand, rb.operand));
        return Ok(Value::new(reg, LType::I1));
    }

    // Operands auto-unwrap a Result to its success payload before arithmetic or
    // comparison — binary operands are value sites.
    let l = gen_expr(cg, left)?;
    let l = crate::result::unwrap(cg, l);
    let r = gen_expr(cg, right)?;
    let r = crate::result::unwrap(cg, r);
    match op {
        "+" | "-" | "*" | "/" | "%" => gen_arith(cg, op, l, r),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => gen_comparison(cg, op, l, r),
        other => Err(CodegenError::unsupported(format!(
            "binary operator `{other}`"
        ))),
    }
}

/// Arithmetic. Float if either operand is a float (the other is promoted),
/// otherwise integer. Division ALWAYS returns float (the Osprey spec); modulo
/// stays integer. The Result<…, `MathError`> wrapper the type system tracks is
/// auto-unwrapped at value sites.
fn gen_arith(cg: &mut Codegen, op: &str, l: Value, r: Value) -> Result<Value> {
    // `+` on list handles is concatenation (`a + b` ≡ `listConcat(a, b)`); on
    // map handles it is a right-biased merge (`a + b` ≡ `mapMerge(a, b)`).
    let is_list = |v: &Value| v.osp_ty.as_deref() == Some(crate::collections::LIST_OWNER);
    let is_map = |v: &Value| v.osp_ty.as_deref() == Some(crate::collections::MAP_OWNER);
    if op == "+" && (is_list(&l) || is_list(&r)) {
        return Ok(crate::collections::concat_handles(cg, &l, &r));
    }
    if op == "+" && (is_map(&l) || is_map(&r)) {
        return Ok(crate::collections::merge_handles(cg, &l, &r));
    }
    // `+` with a string operand is concatenation: osp_strlen/strcpy/strcat
    // into a fresh malloc'd buffer.
    if op == "+" && (l.ty == LType::Str || r.ty == LType::Str) {
        return gen_str_concat(cg, l, r);
    }
    // Numeric arithmetic is typed `Result<…, MathError>`, so each operation
    // builds a Success block: `/` always float, the rest follow operand type.
    // The Success wrapper auto-unwraps at value sites (interpolation,
    // comparison, args), but `toString`/`print` show it as `Success(n)`.
    if op == "/" {
        return gen_division(cg, l, r);
    }
    if l.ty == LType::Double || r.ty == LType::Double {
        let ld = as_double(cg, l)?;
        let rd = as_double(cg, r)?;
        let opc = match op {
            "+" => "fadd",
            "-" => "fsub",
            "*" => "fmul",
            "/" => "fdiv",
            _ => "frem",
        };
        let reg = cg.emit_reg(format!("{opc} double {}, {}", ld.operand, rd.operand));
        return crate::result::make_ok(cg, Value::new(reg, LType::Double), LType::Double);
    }
    let li = as_i64(cg, l)?;
    let ri = as_i64(cg, r)?;
    let opc = match op {
        "+" => "add",
        "-" => "sub",
        "*" => "mul",
        "/" => "sdiv",
        _ => "srem",
    };
    let reg = cg.emit_reg(format!("{opc} i64 {}, {}", li.operand, ri.operand));
    crate::result::make_ok(cg, Value::new(reg, LType::I64), LType::I64)
}

/// The `/` operator — always float, divide-by-zero checked.
fn gen_division(cg: &mut Codegen, l: Value, r: Value) -> Result<Value> {
    let ld = as_double(cg, l)?;
    let rd = as_double(cg, r)?;
    gen_checked_division(
        cg,
        &ld.operand,
        &rd.operand,
        LType::Double,
        "fdiv double",
        "fcmp oeq double",
        "0.0",
    )
}

/// The `intDiv(a, b)` builtin — truncating integer division, divide-by-zero
/// checked. The integer sibling of `/` (which the spec fixes to float).
/// Implements [BUILTIN-INTDIV].
fn gen_int_division(cg: &mut Codegen, l: Value, r: Value) -> Result<Value> {
    let li = as_i64(cg, l)?;
    let ri = as_i64(cg, r)?;
    gen_checked_division(
        cg,
        &li.operand,
        &ri.operand,
        LType::I64,
        "sdiv i64",
        "icmp eq i64",
        "0",
    )
}

/// Shared divide-by-zero skeleton for `/` and `intDiv`: a zero divisor yields
/// `Error` (`Result<_, MathError>` disc 1), else `Success(quotient)`. `div`/`cmp`
/// carry their LLVM type, `zero` is the typed zero literal.
fn gen_checked_division(
    cg: &mut Codegen,
    lop: &str,
    rop: &str,
    inner: LType,
    div: &str,
    cmp: &str,
    zero: &str,
) -> Result<Value> {
    use crate::result::{make_result, NO_MSG};
    let isz = cg.emit_reg(format!("{cmp} {rop}, {zero}"));
    let (zero_bb, nonzero_bb, end) = (cg.fresh_label(), cg.fresh_label(), cg.fresh_label());
    cg.emit(format!(
        "br i1 {isz}, label %{zero_bb}, label %{nonzero_bb}"
    ));

    cg.start_block(&nonzero_bb);
    let q = cg.emit_reg(format!("{div} {lop}, {rop}"));
    let ok = make_result(cg, Value::new(q, inner), inner, "0", NO_MSG)?;
    let okb = cg.snapshot_to(&end);

    cg.start_block(&zero_bb);
    let msg = cg.string_constant("division by zero");
    let err = make_result(cg, Value::new(zero, inner), inner, "1", &msg.operand)?;
    let errb = cg.snapshot_to(&end);

    cg.start_block(&end);
    let reg = cg.emit_reg(format!(
        "phi {0}* [ {1}, %{okb} ], [ {2}, %{errb} ]",
        crate::llty::result_struct_ty(inner),
        ok.operand,
        err.operand
    ));
    Ok(Value::result(reg, inner))
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
    Ok(Value::new(buf, LType::Str))
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

fn gen_comparison(cg: &mut Codegen, op: &str, l: Value, r: Value) -> Result<Value> {
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
        "-" if v.ty == LType::Double => Ok(Value::new(
            cg.emit_reg(format!("fneg double {}", v.operand)),
            LType::Double,
        )),
        "-" => {
            let i = as_i64(cg, v)?;
            Ok(Value::new(
                cg.emit_reg(format!("sub i64 0, {}", i.operand)),
                LType::I64,
            ))
        }
        "!" | "not" => {
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

fn gen_call(
    cg: &mut Codegen,
    function: &Expr,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    // A directly-applied lambda (`x |> fn(y) => …`, `(fn(y) => …)(x)`) is
    // beta-reduced inline.
    if let Expr::Lambda {
        parameters, body, ..
    } = function
    {
        return apply_lambda(cg, parameters, body, arguments);
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
                .and_then(Codegen::fn_value_sig);
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
            .and_then(Codegen::fn_value_sig)
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
    if let Some((params, body)) = cg.lambdas.get(name).cloned() {
        return apply_lambda(cg, &params, &body, arguments);
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
        "intDiv" => {
            let a = arg_exprs(arguments, named);
            let (an, bn) = (
                a.first()
                    .ok_or_else(|| CodegenError::invalid("intDiv needs two arguments"))?,
                a.get(1)
                    .ok_or_else(|| CodegenError::invalid("intDiv needs two arguments"))?,
            );
            let l = gen_expr(cg, an)?;
            let l = crate::result::unwrap(cg, l);
            let r = gen_expr(cg, bn)?;
            let r = crate::result::unwrap(cg, r);
            gen_int_division(cg, l, r)
        }
        // Runtime builtins take precedence over a same-named user function: the
        // names below are reserved. Each dispatcher returns `None` when the name
        // is not its builtin, so the chain falls through to a user call.
        _ => {
            if let Some(v) = crate::testing::gen(cg, name, arguments, named)? {
                return Ok(v);
            }
            if let Some(v) = crate::strings::gen(cg, name, arguments, named)? {
                return Ok(v);
            }
            if let Some(v) = crate::collections::gen(cg, name, arguments, named)? {
                return Ok(v);
            }
            if let Some(v) = crate::iter::gen(cg, name, arguments, named)? {
                return Ok(v);
            }
            if let Some(v) = crate::fiber::gen_builtin(cg, name, arguments)? {
                return Ok(v);
            }
            if let Some(v) = crate::extern_call::gen(cg, name, arguments, named)? {
                return Ok(v);
            }
            // A generic user function is specialised by inlining its body with
            // the concrete argument types at this call site.
            if let Some(v) = crate::genfn::try_inline(cg, name, arguments, named)? {
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
/// argument, lower the body in a fresh scope, then unwrap a `Result` return.
/// A lambda's inferred return type is its body's success payload, so applying
/// `fn(x) => x * 2` yields a plain `int` — the body's `Result<…, MathError>`
/// is unwrapped at the non-Result return boundary.
fn apply_lambda(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    arguments: &[Expr],
) -> Result<Value> {
    let mut values = Vec::with_capacity(arguments.len());
    for a in arguments {
        values.push(gen_expr(cg, a)?);
    }
    apply_lambda_values(cg, parameters, body, values)
}

/// [`apply_lambda`] over already-evaluated argument values — shared with the
/// iterator builtins, which produce loop elements as values.
pub(crate) fn apply_lambda_values(
    cg: &mut Codegen,
    parameters: &[Parameter],
    body: &Expr,
    values: Vec<Value>,
) -> Result<Value> {
    cg.push_scope();
    for (p, v) in parameters.iter().zip(values) {
        cg.bind(p.name.clone(), v);
    }
    let v = gen_expr(cg, body);
    cg.pop_scope();
    Ok(crate::result::unwrap(cg, v?))
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

/// Call `name` with already-evaluated argument values — the shared tail of
/// `gen_user_call` and the iterator callbacks. Coerces each argument to the
/// inferred parameter type, declares unknown (runtime) callees, and tags a
/// `Result`-returning callee's value.
pub(crate) fn call_with_values(cg: &mut Codegen, name: &str, args: Vec<Value>) -> Result<Value> {
    // `print` as a first-class callback maps to the print intrinsic.
    if name == "print" {
        let v = args.into_iter().next().unwrap_or_else(Value::unit);
        return gen_print(cg, v);
    }
    // Coerce each argument to the declared parameter type where known.
    let coerced = match cg.fn_param_ltypes(name) {
        Some(ptys) if ptys.len() == args.len() => args
            .into_iter()
            .zip(ptys)
            .map(|(a, want)| crate::cast::coerce_to(cg, a, want))
            .collect::<Result<Vec<_>>>()?,
        _ => args,
    };
    let typed = crate::llty::comma_join(&coerced, Value::typed);
    // A function declared `-> Result<T, E>` hands back a Result block pointer.
    if let Some(inner) = cg.fn_ret_result_inner(name) {
        let rty = format!("{}*", crate::llty::result_struct_ty(inner));
        let reg = emit_user_call(cg, name, &rty, &coerced, &typed);
        return Ok(Value::result(reg, inner));
    }
    let ret = cg.fn_ret_ltype(name).unwrap_or(LType::I64);
    let reg = emit_user_call(cg, name, ret.as_str(), &coerced, &typed);
    Ok(Value::new(reg, ret).with_owner(cg.fn_ret_owner(name)))
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

pub(crate) fn ordered_args(
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
        .map(|ts| ts.iter().map(Codegen::fn_value_sig).collect())
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
                    crate::closure::emit_closure(cg, &params, &body, sig)
                };
            }
            if ffi && cg.fn_params.contains_key(&target) {
                return Ok(fn_pointer(cg, &target));
            }
            gen_expr(cg, expr)
        }
        _ => gen_expr(cg, expr),
    }
}

fn gen_interpolation(cg: &mut Codegen, parts: &[InterpolatedPart]) -> Result<Value> {
    let mut fmt = String::new();
    let mut args: Vec<String> = Vec::new();
    for part in parts {
        match part {
            InterpolatedPart::Text(t) => fmt.push_str(&t.replace('%', "%%")),
            InterpolatedPart::Expr(e) => {
                // `${expr}` unwraps a Result to its payload before formatting
                // (an interpolation hole is a value site), so `${21 * 2}`
                // prints `42`, not `Success(42)`.
                let v = gen_expr(cg, e)?;
                let v = crate::result::unwrap(cg, v);
                let s = to_string_value(cg, v)?;
                fmt.push_str("%s");
                args.push(format!("i8* {}", s.operand));
            }
        }
    }
    let fmtv = cg.string_constant(&fmt);
    cg.add_extern("declare i32 @sprintf(i8*, i8*, ...)");
    let buf = cg.heap_alloc("4096");
    let tmp = cg.fresh_reg();
    let extra = if args.is_empty() {
        String::new()
    } else {
        format!(", {}", args.join(", "))
    };
    cg.emit(format!(
        "{tmp} = call i32 (i8*, i8*, ...) @sprintf(i8* {buf}, i8* {}{extra})",
        fmtv.operand
    ));
    Ok(Value::new(buf, LType::Str))
}

fn first_arg<'a>(arguments: &'a [Expr], named: &'a [NamedArgument]) -> Option<&'a Expr> {
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

pub(crate) fn describe(expr: &Expr) -> String {
    let kind = match expr {
        Expr::List(_) => "list literal",
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
