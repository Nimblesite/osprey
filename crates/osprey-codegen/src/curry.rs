//! Curried application spines. Implements [FLAVOR-ML-CURRY].
//!
//! `applyCurried f a b = f a b` lowers to a ONE-parameter function whose body
//! is a chain of lambdas, so the call `applyCurried g 3 4` arrives here as
//! `Call(Call(Call(applyCurried, [g]), [3]), [4])`. A generic definition exists
//! only as an inlined specialisation ([`crate::genfn`]), and its intermediate
//! lambdas cannot be materialised as closure values while their types are still
//! generic — the whole spine must therefore be inlined and beta-reduced as one
//! unit, one lambda per argument group.

use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::llty::Value;
use osprey_ast::{Expr, NamedArgument, Position};

/// One application group of a spine: `f(a, b)(c)` has groups `[a, b]`, `[c]`.
pub(crate) type ArgGroup<'a> = (&'a [Expr], &'a [NamedArgument]);

struct Spine<'a> {
    head: &'a str,
    application: Option<Position>,
    groups: Vec<ArgGroup<'a>>,
}

/// Lower `function(arguments)` when `function` is itself an application spine
/// headed by a generic user function — `None` when it is anything else, so the
/// ordinary call paths keep precedence.
pub(crate) fn try_spine(
    cg: &mut Codegen,
    function: &Expr,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    let Some(Spine {
        head,
        application,
        mut groups,
    }) = spine(function)
    else {
        return Ok(None);
    };
    if !cg.fn_defs.contains_key(head) {
        return Ok(None);
    }
    groups.push((arguments, named));
    let Some((first, rest)) = groups.split_first() else {
        return Ok(None);
    };
    crate::expr::with_application(cg, application, |cg| {
        crate::genfn::try_inline(cg, head, first.0, first.1, rest)
    })
}

/// Flatten an application spine into its head identifier and argument groups,
/// outermost group last. `None` when the head is not a bare name.
fn spine(expr: &Expr) -> Option<Spine<'_>> {
    let mut groups = Vec::new();
    let mut node = expr;
    let mut application = None;
    loop {
        match node {
            Expr::TypeApply {
                function, position, ..
            } => {
                application = *position;
                node = function;
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                groups.push((arguments.as_slice(), named_arguments.as_slice()));
                node = function;
            }
            Expr::Identifier(name) => {
                groups.reverse();
                return Some(Spine {
                    head: name,
                    application,
                    groups,
                });
            }
            _ => return None,
        }
    }
}

/// Remaining applications retain their caller independently of callee bindings.
#[derive(Default)]
pub(crate) struct Groups<'a> {
    remaining: &'a [ArgGroup<'a>],
    caller: Option<crate::builder::FileScopeState>,
}

impl<'a> Groups<'a> {
    pub(crate) fn file_scoped(cg: &mut Codegen, remaining: &'a [ArgGroup<'a>]) -> Self {
        Self {
            remaining,
            caller: Some(crate::builder::FileScopeState::enter(cg)),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    pub(crate) fn restore(self, cg: &mut Codegen) {
        if let Some(caller) = self.caller {
            caller.restore(cg);
        }
    }

    fn in_caller<T>(&mut self, cg: &mut Codegen, emit: impl FnOnce(&mut Codegen) -> T) -> T {
        cg.with_caller_types(|cg| match &mut self.caller {
            Some(caller) => caller.with(cg, emit),
            None => emit(cg),
        })
    }
}

/// Beta-reduce each lambda in declaration scope, evaluating each group in caller scope.
/// Implements [TYPE-GENERICS-FN] [FLAVOR-ML-CURRY].
pub(crate) fn apply_groups(
    cg: &mut Codegen,
    body: &Expr,
    groups: &mut Groups<'_>,
) -> Result<Value> {
    if groups.is_empty() {
        return gen_expr(cg, body);
    }
    match body {
        Expr::Block {
            statements,
            value,
            position,
        } => {
            crate::expr::gen_block_with(cg, statements, value.as_deref(), *position, |cg, tail| {
                apply_groups(cg, tail, groups)
            })
        }
        Expr::Lambda {
            parameters,
            body: inner,
            position,
            ..
        } if groups
            .remaining
            .first()
            .is_some_and(|group| parameters.len() == group.0.len() + group.1.len()) =>
        {
            apply_lambda_group(cg, parameters, inner, *position, groups)
        }
        _ => apply_value_groups(cg, body, groups),
    }
}

fn apply_lambda_group(
    cg: &mut Codegen,
    parameters: &[osprey_ast::Parameter],
    body: &Expr,
    position: Option<Position>,
    groups: &mut Groups<'_>,
) -> Result<Value> {
    let Some((group, rest)) = groups.remaining.split_first() else {
        return gen_expr(cg, body);
    };
    let values = groups.in_caller(cg, |cg| group_values(cg, group))?;
    groups.remaining = rest;
    let sig = crate::expr::inline_sig(cg, position);
    crate::expr::reduce_lambda(cg, parameters, body, values, sig.as_ref(), groups, position)
}

fn apply_value_groups(cg: &mut Codegen, body: &Expr, groups: &mut Groups<'_>) -> Result<Value> {
    let mut value = gen_expr(cg, body)?;
    let mut ty = value.inferred_type.clone().or_else(|| cg.callee_fn_type(body));
    while let Some((group, rest)) = groups.remaining.split_first() {
        (value, ty) = groups.in_caller(cg, |cg| apply_value_group(cg, &value, ty, group))?;
        groups.remaining = rest;
    }
    Ok(value)
}

fn apply_value_group(
    cg: &mut Codegen,
    value: &Value,
    ty: Option<osprey_types::Type>,
    group: &ArgGroup<'_>,
) -> Result<(Value, Option<osprey_types::Type>)> {
    let signature = ty
        .as_ref()
        .and_then(|ty| Codegen::fn_value_sig(&cg.prog, ty))
        .ok_or_else(|| {
            crate::error::CodegenError::invalid("curried value has no function signature")
        })?;
    let value = crate::closure::cell_call_exprs(
        cg,
        &value.operand,
        &signature,
        &crate::expr::arg_exprs(group.0, group.1),
    )?;
    let returned_type = ty.and_then(|ty| match ty {
        osprey_types::Type::Fun { ret, .. } => Some(*ret),
        _ => None,
    });
    Ok((value, returned_type))
}

fn group_values(cg: &mut Codegen, group: &ArgGroup<'_>) -> Result<Vec<Value>> {
    crate::expr::arg_exprs(group.0, group.1)
        .into_iter()
        .map(|a| gen_expr(cg, a))
        .collect()
}
