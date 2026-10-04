//! Expression calls lowering.
use super::{
    apply_bound_lambda, apply_lambda, arg_exprs, as_double, as_i64, binary_integer_builtin,
    eval_arg, first_arg, gen_expr, gen_print, gen_user_call, is_binary_integer_builtin,
    to_string_value, two_int_args, Codegen, CodegenError, Expr, FnSig, NamedArgument, Position,
    Result, Value,
};

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

pub(crate) fn unapplied(mut expression: &Expr) -> &Expr {
    while let Expr::TypeApply { function, .. } = expression {
        expression = function;
    }
    expression
}

pub(crate) fn with_application<R>(
    cg: &mut Codegen,
    position: Option<Position>,
    generate: impl FnOnce(&mut Codegen) -> R,
) -> R {
    let Some(bindings) = position
        .and_then(|p| cg.prog.applications.get(&(p.line, p.column)))
        .cloned()
    else {
        return generate(cg);
    };
    let specialized = cg.prog.specialized(&bindings);
    let original = std::mem::replace(&mut cg.prog, specialized);
    let previous = cg.application_caller.replace(original.clone());
    let result = generate(cg);
    cg.prog = original;
    cg.application_caller = previous;
    result
}

/// Deferred generic dotted calls select against the instantiated receiver.
/// Binding its value before selecting also guarantees exactly one evaluation.
pub(super) fn gen_method_call(
    cg: &mut Codegen,
    target: &Expr,
    method: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    let receiver = gen_expr(cg, target)?;
    let source = osprey_ast::symbol::demangle(method).unwrap_or_else(|| method.to_owned());
    let field = source.rsplit("::").next().unwrap_or(&source);
    let has_field = receiver
        .inferred_type
        .as_ref()
        .and_then(|ty| cg.prog.field_type(ty, field))
        .is_some()
        || receiver
            .osp_ty
            .as_ref()
            .and_then(|owner| cg.prog.ctors.get(owner))
            .is_some_and(|ctor| ctor.fields.iter().any(|(name, _)| name == field));
    let temporary = osprey_ast::generated_name("method_receiver", 0);
    if !has_field
        && !cg.fn_params.contains_key(method)
        && !cg.prog.functions.contains_key(method)
        && !cg.call_aliases.contains_key(method)
        && !cg.module_globals.contains_key(method)
        && cg.lambda_def(method).is_none()
        && cg.lookup(method).is_none()
        && osprey_types::builtin_signature(method).is_none()
    {
        return Err(CodegenError::unsupported("method call"));
    }
    let target = Expr::Identifier(temporary.clone());
    let mut arguments = arguments.to_vec();
    let function = if has_field {
        Expr::FieldAccess {
            target: Box::new(target),
            field: field.to_owned(),
        }
    } else {
        arguments.insert(0, target);
        Expr::Identifier(method.to_owned())
    };
    cg.push_scope();
    cg.bind(temporary, receiver);
    let result = gen_call(cg, &function, &arguments, named);
    cg.pop_scope();
    result
}

pub(super) fn gen_call(
    cg: &mut Codegen,
    function: &Expr,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    if let Expr::TypeApply {
        function, position, ..
    } = function
    {
        return with_application(cg, *position, |cg| gen_call(cg, function, arguments, named));
    }
    // A directly-applied lambda (`x |> fn(y) => …`, `(fn(y) => …)(x)`) is
    // beta-reduced inline.
    if let Expr::Lambda {
        parameters,
        body,
        position,
        ..
    } = function
    {
        let slots = arg_exprs(arguments, named);
        return apply_lambda(cg, parameters, body, *position, &slots);
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
    if let Some(sig) = call_result_sig(cg, function) {
        return call_fn_value(cg, function, Some(&sig), arguments, named);
    }
    let Expr::Identifier(ident) = function else {
        // A higher-order callee that is an arbitrary expression — a chained
        // application (`add3(1)(2)(3)`) or a function held in a record field.
        // Recover its signature from the type table and dispatch through the
        // closure cell; fail loudly only when the callee is not a function value.
        let sig = cg
            .callee_fn_type(function)
            .as_ref()
            .and_then(|t| Codegen::fn_value_sig(&cg.prog, t));
        return call_fn_value(cg, function, sig.as_ref(), arguments, named);
    };
    // A function-valued parameter (bound while inlining a generic function)
    // redirects to its real callee, so `f(x)` becomes `toString(x)` / `addOne(x)`.
    // The source call still targets a value: preserve its written slots before
    // the known declaration's named-argument path can reorder them.
    // Implements [CALL-ARGUMENTS].
    let aliased_arguments = alias_slot_arguments(cg, ident, arguments, named);
    let (arguments, named) = aliased_arguments
        .as_deref()
        .map_or((arguments, named), |arguments| (arguments, &[]));
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
        let slots = arg_exprs(arguments, named);
        return apply_bound_lambda(cg, name, &params, &body, position, &slots);
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
            let value = gen_expr(cg, arg)?;
            crate::arithmetic::absolute(cg, value)
        }
        // [BUILTIN-TOFLOAT] [GPU-CONVERT] Widening int → float: one `sitofp`,
        // round-to-nearest-even, exact for |n| <= 2^53. Total, so no Result.
        "toFloat" => {
            let arg = first_arg(arguments, named)
                .ok_or_else(|| CodegenError::invalid("toFloat needs one argument"))?;
            let v = gen_expr(cg, arg)?;
            let n = as_i64(cg, v)?;
            as_double(cg, n)
        }
        name if is_binary_integer_builtin(name) => {
            let (left, right) = two_int_args(cg, name, arguments, named)?;
            binary_integer_builtin(cg, name, left, right)
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

/// Normalize a value call before an inlining alias exposes a declaration's
/// formal parameter names. The implicit UFCS receiver stays before its values.
fn alias_slot_arguments(
    cg: &Codegen,
    name: &str,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Option<Vec<Expr>> {
    (!named.is_empty() && cg.call_aliases.contains_key(name)).then(|| {
        arguments
            .iter()
            .chain(named.iter().map(|argument| &argument.value))
            .cloned()
            .collect()
    })
}

fn call_result_sig(cg: &Codegen, function: &Expr) -> Option<FnSig> {
    let Expr::Call {
        function: inner, ..
    } = function
    else {
        return None;
    };
    let Expr::Identifier(name) = &**inner else {
        return None;
    };
    cg.call_result_fn_type(name)
        .as_ref()
        .and_then(|ty| Codegen::fn_value_sig(&cg.prog, ty))
}

/// Call through an evaluated function value: lower the callee expression to a
/// closure handle, coerce the arguments to the signature's parameter types,
/// and call through the cell.
fn call_fn_value(
    cg: &mut Codegen,
    callee: &Expr,
    sig: Option<&FnSig>,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Value> {
    let handle = gen_expr(cg, callee)?;
    // Generic constructor layouts retain erased field templates. The loaded
    // closure carries its instantiated type, including float returns and the
    // function types of callback parameters.
    let semantic = handle.inferred_type.as_ref();
    let actual = semantic.and_then(|ty| Codegen::fn_value_sig(&cg.prog, ty));
    let sig = actual
        .as_ref()
        .or(sig)
        .ok_or_else(|| CodegenError::unsupported("indirect / higher-order call"))?;
    let slots: Vec<_> = match semantic {
        Some(osprey_types::Type::Fun { params, .. }) => params
            .iter()
            .map(|ty| Codegen::fn_value_sig(&cg.prog, ty))
            .collect(),
        _ => Vec::new(),
    };
    let exprs = arg_exprs(arguments, named);
    let values = exprs
        .iter()
        .enumerate()
        .map(|(i, expr)| eval_arg(cg, expr, slots.get(i).and_then(Option::as_ref), false))
        .collect::<Result<Vec<_>>>()?;
    let mut result = crate::closure::cell_call_values(cg, &handle.operand, sig, values)?;
    if let Some(osprey_types::Type::Fun { ret, .. }) = semantic {
        result.inferred_type = Some((**ret).clone());
    }
    Ok(result)
}
