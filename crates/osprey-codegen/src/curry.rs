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
use osprey_ast::{Expr, NamedArgument};

/// One application group of a spine: `f(a, b)(c)` has groups `[a, b]`, `[c]`.
pub(crate) type ArgGroup<'a> = (&'a [Expr], &'a [NamedArgument]);

/// Lower `function(arguments)` when `function` is itself an application spine
/// headed by a generic user function — `None` when it is anything else, so the
/// ordinary call paths keep precedence.
pub(crate) fn try_spine(
    cg: &mut Codegen,
    function: &Expr,
    arguments: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    let Some((head, mut groups)) = spine(function) else {
        return Ok(None);
    };
    if !cg.fn_defs.contains_key(head) {
        return Ok(None);
    }
    groups.push((arguments, named));
    let Some((first, rest)) = groups.split_first() else {
        return Ok(None);
    };
    crate::genfn::try_inline(cg, head, first.0, first.1, rest)
}

/// Flatten an application spine into its head identifier and argument groups,
/// outermost group last. `None` when the head is not a bare name.
fn spine(expr: &Expr) -> Option<(&str, Vec<ArgGroup<'_>>)> {
    let mut groups = Vec::new();
    let mut node = expr;
    loop {
        match node {
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
                return Some((name.as_str(), groups));
            }
            _ => return None,
        }
    }
}

/// Apply the still-unconsumed groups of a spine to an inlined body, beta-
/// reducing one lambda per group. With no groups left this is just the body.
pub(crate) fn apply_groups(
    cg: &mut Codegen,
    body: &Expr,
    groups: &[ArgGroup<'_>],
) -> Result<Value> {
    let Some((group, rest)) = groups.split_first() else {
        return gen_expr(cg, body);
    };
    let Expr::Lambda {
        parameters,
        body: inner,
        position,
        ..
    } = body
    else {
        return gen_expr(cg, &applied(body, groups));
    };
    // A group that does not fill the lambda's parameter list is a partial
    // application of a flat head, which beta-reduction cannot express; hand the
    // rebuilt call back to the ordinary paths so it reports there.
    if parameters.len() != group.0.len() + group.1.len() {
        return gen_expr(cg, &applied(body, groups));
    }
    let values = group_values(cg, group)?;
    // A still-generic lambda in the spine is specialised by its arguments, not
    // by the one type inference recorded for its position
    // ([`crate::expr::inline_sig`]).
    let sig = crate::expr::inline_sig(cg, *position);
    crate::expr::reduce_lambda(cg, parameters, inner, values, sig.as_ref(), rest, *position)
}

/// Lower one group's arguments, named ones in their parameter's position.
fn group_values(cg: &mut Codegen, group: &ArgGroup<'_>) -> Result<Vec<Value>> {
    let exprs = crate::expr::arg_exprs(group.0, group.1);
    let mut values = Vec::with_capacity(exprs.len());
    for a in exprs {
        values.push(gen_expr(cg, a)?);
    }
    Ok(values)
}

/// Rebuild `head(g1)(g2)…` as an AST so a spine this module cannot reduce is
/// lowered by the ordinary call paths rather than silently dropped.
fn applied(head: &Expr, groups: &[ArgGroup<'_>]) -> Expr {
    groups
        .iter()
        .fold(head.clone(), |function, (arguments, named)| Expr::Call {
            function: Box::new(function),
            arguments: arguments.to_vec(),
            named_arguments: named.to_vec(),
        })
}
