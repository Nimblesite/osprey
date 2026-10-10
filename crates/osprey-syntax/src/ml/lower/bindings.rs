//! ML bindings lowering.
use super::{
    arrow_of, arrow_spine, in_scope, lower_effect_ref, lower_error, lower_expr,
    lower_function_body, lower_type_param, params_scope, render_type, type_expr, Expr, MlExpr,
    MlParam, MlSig, MlType, Parameter, Position, Stmt, TypeExpr, TUPLE_UNIMPLEMENTED, WILDCARD,
};

/// A binding with no parameters is a `let` ([FLAVOR-ML-BIND]; its signature
/// becomes the binding's type); one with parameters is a function. The
/// `uncurried` flag selects the
/// surface form: `f (x, y) = …` (uncurried) builds one FLAT multi-parameter
/// `Function` (twinning Default `fn f(x, y)`); `f x y = …` (curried) builds a
/// one-parameter `Function` returning a `Lambda` chain ([FLAVOR-ML-CURRY]). The
/// unit marker `()` yields a zero-parameter function, matching `fn f() = …`.
pub(in crate::ml) fn lower_binding(
    mutable: bool,
    name: String,
    params: Vec<MlParam>,
    uncurried: bool,
    body: MlExpr,
    pos: Position,
    sig: Option<MlSig>,
) -> Stmt {
    if params.is_empty() {
        crate::ml::binding_ranges::variable(&name, pos);
    }
    let owner = if uncurried || params.len() <= 1 {
        pos
    } else {
        curry_position(pos, params.len().saturating_sub(2))
    };
    let lower_body = if params.is_empty() {
        lower_expr
    } else {
        lower_function_body
    };
    let body = crate::ml::binding_ranges::with_owner(owner, || {
        in_scope(params_scope(&params), move || lower_body(body))
    });
    // Split the paired signature into its type params, declared type and
    // effect row.
    let (type_params, ty, effects, effect_tail, effect_row_present, signature_position) = match sig
    {
        Some(s) => (
            s.type_params,
            Some(s.ty),
            s.effects,
            s.effect_tail,
            s.effect_row_present,
            Some(s.position),
        ),
        None => (Vec::new(), None, Vec::new(), None, false, None),
    };
    let ty = ty.as_ref();
    if let Some(message) = ty.and_then(|t| signature_mismatch(&name, &params, uncurried, t)) {
        lower_error(message, pos);
    }
    // An empty surface parameter list is a value binding; a non-empty one (even
    // the lone unit marker `()`) is a function. `()` binds no canonical
    // parameter, so `f () = e` is a zero-parameter function like `fn f() = e`.
    // A value binding has no effect row, so the signature's effects are dropped.
    if params.is_empty() {
        return Stmt::Let {
            name,
            mutable,
            ty: signature_annotation(ty.and_then(type_expr), signature_position),
            value: body,
            doc: None,
            position: Some(pos),
        };
    }
    let (parameters, body, return_type) = if uncurried {
        build_function_flat(params, body, ty, pos, signature_position)
    } else {
        build_function(params, body, ty, pos, signature_position)
    };
    Stmt::Function {
        name,
        type_params: type_params.into_iter().map(lower_type_param).collect(),
        parameters,
        return_type,
        effects: effects.into_iter().map(lower_effect_ref).collect(),
        effect_tail,
        effect_row_present,
        body,
        doc: None,
        position: Some(pos),
    }
}

/// A header and an inline parameter annotation are independent constraints.
/// Check the inline type immediately inside that parameter's binder, before a
/// later curry parameter can shadow its name.
pub(in crate::ml) fn inline_param_constraint(
    param: Option<&MlParam>,
    index: usize,
    pos: Position,
    signed: bool,
    body: Expr,
) -> Expr {
    let (true, Some(MlParam::Typed(name, ty, annotation_pos))) = (signed, param) else {
        return body;
    };
    let name = parameter_name(name.name.clone(), index);
    Expr::Block {
        position: None,
        statements: vec![Stmt::Let {
            value: Expr::Identifier(name.clone()),
            name,
            mutable: false,
            ty: signature_annotation(type_expr(ty), Some(*annotation_pos)),
            doc: None,
            position: Some(pos),
        }],
        value: Some(Box::new(body)),
    }
}

/// A paired signature the lowering cannot honour, as a diagnostic — or `None`
/// when every written type survives into the canonical AST. Guards the silent
/// drops in [`build_function_flat`]/[`build_function`]: a tuple domain whose
/// arity misses the binding, a spine too short to type every parameter, and any
/// spine slot [`type_expr`] cannot represent (a bare tuple type) all previously
/// vanished without a trace, leaving the binding checked as if unannotated
/// ([FLAVOR-ML-FN], [FLAVOR-ML-CURRY]; tuple values/types are unimplemented,
/// [TYPE-TUPLE]).
pub(in crate::ml) fn signature_mismatch(
    name: &str,
    params: &[MlParam],
    uncurried: bool,
    ty: &MlType,
) -> Option<String> {
    let spine = arrow_spine(ty);
    if params.is_empty() {
        return type_expr(ty)
            .is_none()
            .then(|| format!("let `{name}` signature: a tuple type {TUPLE_UNIMPLEMENTED}"));
    }
    if let Some(MlType::Tuple(parts)) = spine.first() {
        if !uncurried {
            return Some(format!(
                "function `{name}`: a parenthesised signature domain declares flat parameters; write `{name} (…) = …` to match"
            ));
        }
        if parts.len() != params.len() {
            return Some(format!(
                "function `{name}` signature declares {} parameters but the binding takes {}",
                parts.len(),
                params.len()
            ));
        }
    }
    let spine = expand_tuple_head(spine, params.len());
    if spine.len() <= params.len() {
        return Some(format!(
            "function `{name}` signature provides {} parameter types but the binding takes {}",
            spine.len().saturating_sub(1),
            params.len()
        ));
    }
    spine
        .iter()
        .find(|slot| type_expr(slot).is_none())
        .map(|slot| {
            format!(
                "function `{name}` signature: the tuple type {} {TUPLE_UNIMPLEMENTED}",
                render_type(slot)
            )
        })
}

/// Build a FLAT multi-parameter function (`f (x, y) = …`, the uncurried form):
/// every surface parameter becomes a real canonical parameter, typed positionally
/// from the signature spine, and the return type is the spine tail left after
/// them — byte-identical to the Default `fn f(x, y) -> r` ([FLAVOR-ML-CURRY]).
pub(in crate::ml) fn build_function_flat(
    params: Vec<MlParam>,
    body: Expr,
    sig: Option<&MlType>,
    pos: Position,
    signature_position: Option<Position>,
) -> (Vec<Parameter>, Expr, Option<TypeExpr>) {
    let spine = expand_tuple_head(sig.map(arrow_spine).unwrap_or_default(), params.len());
    let consumed = params.len();
    let body = params.iter().enumerate().rev().fold(body, |body, (i, p)| {
        inline_param_constraint(Some(p), i, curry_position(pos, i), sig.is_some(), body)
    });
    let parameters = params
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| lower_param(p, i, spine.get(i), signature_position, pos))
        .collect();
    (
        parameters,
        body,
        signature_annotation(
            arrow_of(spine.get(consumed..).unwrap_or(&[])),
            signature_position,
        ),
    )
}

/// An uncurried binding may be signed with the tuple spelling
/// `(T, U) -> R`: the tuple's parts type the parameters one-to-one and the
/// rest of the spine is the return — matching the Default flavor's
/// `fn f(a: T, b: U) -> R` exactly ([FLAVOR-ML-GENERICS]). A curried spine
/// (`T -> U -> R`) keeps typing parameters positionally.
pub(in crate::ml) fn expand_tuple_head(spine: Vec<MlType>, param_count: usize) -> Vec<MlType> {
    match spine.split_first() {
        Some((MlType::Tuple(parts), rest)) if parts.len() == param_count => {
            parts.iter().cloned().chain(rest.iter().cloned()).collect()
        }
        _ => spine,
    }
}

/// Build a **curried** function ([FLAVOR-ML-CURRY]): ML curries by default, so a
/// multi-parameter binding `f x y = body` lowers to a ONE-parameter
/// [`Stmt::Function`] whose body is a curried chain of one-parameter
/// [`Expr::Lambda`]s — byte-identical to the Default *explicit-curry*
/// `fn f(x) = fn(y) => body`, NOT the multi-parameter `fn f(x, y)`. The first
/// surface parameter stays on the function; every further parameter becomes a
/// nested lambda. Types thread positionally from the signature spine: the first
/// parameter takes `spine[0]`, the curried tail takes `spine[1..]`, and the
/// function's return type is the (function-typed) tail `arrow_of(spine[1..])`.
/// The unit marker `()` binds no parameter (so `f () = e : Unit -> int` is a
/// zero-parameter function returning `int`).
pub(in crate::ml) fn build_function(
    params: Vec<MlParam>,
    body: Expr,
    sig: Option<&MlType>,
    pos: Position,
    signature_position: Option<Position>,
) -> (Vec<Parameter>, Expr, Option<TypeExpr>) {
    let spine = sig.map(arrow_spine).unwrap_or_default();
    let mut rest = params.into_iter();
    let first = rest.next();
    // `()` (unit marker) or no parameter binds nothing.
    let parameters = first
        .clone()
        .and_then(|p| lower_param(p, 0, spine.first(), signature_position, pos))
        .into_iter()
        .collect();
    let tail_spine = spine.get(1..).unwrap_or(&[]);
    let body = curry_params(rest.collect(), body, tail_spine, pos, signature_position);
    let body = inline_param_constraint(
        first.as_ref(),
        0,
        curry_position(pos, 0),
        sig.is_some(),
        body,
    );
    (
        parameters,
        body,
        signature_annotation(arrow_of(tail_spine), signature_position),
    )
}

/// Fold a surface parameter list into a right-nested chain of one-parameter
/// lambdas over `body` (curry-by-default, [FLAVOR-ML-CURRY]): `[x, y]` over `b`
/// becomes `Lambda{[x], Lambda{[y], b}}`. `spine` supplies each parameter's type
/// positionally; the lambda for the i-th parameter returns the function-typed
/// tail `arrow_of(spine[i+1..])`. An empty parameter list returns `body`
/// unchanged (the curried tail of a single-parameter function is just its body).
pub(in crate::ml) fn curry_params(
    params: Vec<MlParam>,
    body: Expr,
    spine: &[MlType],
    pos: Position,
    signature_position: Option<Position>,
) -> Expr {
    let mut acc = body;
    for (i, param) in params.into_iter().enumerate().rev() {
        let body = inline_param_constraint(
            Some(&param),
            i,
            curry_position(pos, i.saturating_add(1)),
            signature_position.is_some(),
            acc,
        );
        acc = Expr::Lambda {
            parameters: lower_param(
                param,
                i,
                spine.get(i),
                signature_position,
                curry_position(pos, i),
            )
            .into_iter()
            .collect(),
            return_type: signature_annotation(
                arrow_of(spine.get(i + 1..).unwrap_or(&[])),
                signature_position,
            ),
            body: Box::new(body),
            position: Some(curry_position(pos, i)),
        };
    }
    acc
}

/// Every lowered fragment of one written signature keeps the same source
/// identity, so diagnostics erase that annotation as one unit.
pub(in crate::ml) fn signature_annotation(
    ty: Option<TypeExpr>,
    position: Option<Position>,
) -> Option<TypeExpr> {
    ty.map(|mut ty| {
        ty.position = position;
        ty
    })
}

/// One surface parameter as a canonical [`Parameter`], or `None` for the unit
/// marker `()`, which binds nothing. `_` takes the generated ignored-parameter
/// name for its slot ([PARAM-WILDCARD]) so repeated `_`s in one head cannot
/// collide and none of them is referenceable from the body. This is the single
/// definition of the mapping; every head form routes through it.
pub(in crate::ml) fn lower_param(
    param: MlParam,
    index: usize,
    inferred: Option<&MlType>,
    signature_position: Option<Position>,
    owner: Position,
) -> Option<Parameter> {
    let inline_constraint = signature_position.is_some() && matches!(&param, MlParam::Typed(..));
    let (name, ty) = match param {
        MlParam::Named(name) => (
            crate::ml::binding_ranges::parameter(name, owner),
            signature_annotation(inferred.and_then(type_expr), signature_position),
        ),
        MlParam::Typed(name, ty, annotation_pos) => (
            crate::ml::binding_ranges::parameter(name, owner),
            inferred.map_or_else(
                || signature_annotation(type_expr(&ty), Some(annotation_pos)),
                |written| signature_annotation(type_expr(written), signature_position),
            ),
        ),
        MlParam::Unit => return None,
        // [`crate::ml::clauses::merge`] rewrites every clause set before lowering,
        // so a surviving head pattern is one the merge already diagnosed;
        // bind it to an unreferenceable name and let that error stand.
        MlParam::Pattern(_) => (osprey_ast::clause_param_name(index), None),
    };
    let name = parameter_name(name, index);
    Some(Parameter {
        name,
        ty,
        inline_constraint,
    })
}

pub(in crate::ml) fn parameter_name(name: String, index: usize) -> String {
    if name == WILDCARD {
        osprey_ast::wildcard_param_name(index)
    } else {
        name
    }
}

/// Give every synthesized curry lambda its own `ProgramTypes::lambdas` key;
/// sharing the binding position lets an outer parameter ABI replace an inner.
pub(in crate::ml) fn curry_position(pos: Position, index: usize) -> Position {
    let offset = u32::try_from(index).unwrap_or(u32::MAX);
    Position {
        line: pos.line,
        column: pos.column.saturating_add(offset).saturating_add(1),
    }
}

/// Lower a lambda head over an already-lowered `body`. A unit-only or empty head
/// `\() => body` / `\=> body` is a single zero-parameter lambda (nothing to
/// curry); otherwise the parameters curry into nested one-parameter lambdas
/// ([FLAVOR-ML-CURRY]), byte-identical to the Default `fn(x) => fn(y) => body`.
/// Convert a surface parameter list to canonical parameters for a FLAT lambda
/// (the uncurried `\(x, y) =>` head): named/typed params become real parameters,
/// the unit marker `()` contributes none.
pub(in crate::ml) fn flat_params(params: Vec<MlParam>, pos: Position) -> Vec<Parameter> {
    params
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| lower_param(p, i, None, None, pos))
        .collect()
}

pub(in crate::ml) fn lower_lambda(params: Vec<MlParam>, body: Expr, pos: Position) -> Expr {
    if params.iter().all(|p| matches!(p, MlParam::Unit)) {
        return Expr::Lambda {
            parameters: Vec::new(),
            return_type: None,
            body: Box::new(body),
            position: Some(pos),
        };
    }
    curry_params(params, body, &[], pos, None)
}
