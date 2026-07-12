//! The ML **lowerer**: CST ([`super::cst`]) → canonical [`osprey_ast::Program`].
//! This is the *only* place ML surface syntax is normalised into the shared
//! core, and it is where the boundary law is enforced — the output is canonical
//! AST that no later phase can distinguish from Default-flavor output
//! ([FLAVOR-BOUNDARY], [FLAVOR-LOWER-CONTRACT], docs/specs/0023).
//!
//! What this module canonicalises (and the parser deliberately does not):
//! - **Curry-by-default** ([FLAVOR-ML-CURRY]): ML *curries by default*. A
//!   multi-parameter binding `f a b = …` lowers to a one-parameter
//!   [`Stmt::Function`] whose body is a one-parameter [`Expr::Lambda`] chain,
//!   whitespace application `f a b` to nested single-argument calls
//!   `Call(Call(f, [a]), [b])`, and a lambda `\a b => …` to the same curried
//!   [`Expr::Lambda`] chain — each byte-identical to the Default flavor's
//!   *explicit-curry* `fn f(a) = fn(b) => …`, `f(a)(b)`, and `fn(a) => fn(b) =>
//!   …`. Partial application therefore just works: `f a` is the inner saturated
//!   call returning a function value. The IR-equivalence guarantee
//!   ([FLAVOR-IR-EQUIV]) holds against the Default *explicit-curry* twin; a
//!   saturated curried call may be folded back to a multi-argument call by the
//!   backend, but the lowered AST is always the curried form.
//! - **Pipes**: `x |> f` desugars to a call, exactly as the Default lowerer does.
//! - **Records / blocks / interpolation**: surface nodes map to
//!   [`Expr::TypeConstructor`], [`Expr::Block`], and [`Expr::InterpolatedStr`].

use super::cst::{
    MlArm, MlEffectOp, MlEffectRef, MlExpr, MlExternParam, MlField, MlHandleArm, MlItem, MlParam,
    MlPattern, MlType, MlTypeField, MlTypeParam, MlVariance, MlVariant,
};
use crate::strings::{lower_interpolation, unquote};
use osprey_ast::{
    DocComment, DocScope, EffectOperation, EffectRef, Expr, ExternParameter, FieldAssignment,
    HandlerArm, MapEntry, MatchArm, Parameter, Pattern, Position, Program, Stmt, TypeExpr,
    TypeField, TypeParam, TypeVariant, Variance,
};
use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// Names BOUND (as a function or value) in the program currently being
    /// lowered. A whitespace-application spine whose head is one of these is a
    /// user definition, kept CURRIED — nested one-argument calls — so partial
    /// application works ([FLAVOR-ML-CURRY]). Any other head (a multi-argument
    /// builtin or `extern`, which cannot be partially applied) has its SATURATED
    /// spine folded to ONE flat multi-argument call — the saturated-call
    /// optimisation the spec assigns to the backend, done here while the surface
    /// spine is still visible.
    static BOUND_NAMES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Lower a parsed ML CST into the canonical program. Collects every bound name
/// first so [`lower_application`] can tell a curried user call from a saturated
/// builtin/`extern` call.
pub(crate) fn lower(items: Vec<MlItem>) -> Program {
    BOUND_NAMES.with(|s| {
        let mut set = s.borrow_mut();
        set.clear();
        collect_bound_names(&items, &mut set);
    });
    Program {
        statements: lower_items(items),
    }
}

/// Record every name a `name … = …` binding introduces (functions and values),
/// recursing into nested blocks so block-local definitions are seen too.
fn collect_bound_names(items: &[MlItem], out: &mut HashSet<String>) {
    for item in items {
        match item {
            MlItem::Binding { name, body, .. } => {
                let _ = out.insert(name.clone());
                collect_names_in_expr(body, out);
            }
            MlItem::Assign { value, .. } | MlItem::Expr { value, .. } => {
                collect_names_in_expr(value, out);
            }
            _ => {}
        }
    }
}

/// Walk an expression for nested binding names (a block's items, and the bodies
/// of lambdas/matches/handlers that may themselves contain blocks).
fn collect_names_in_expr(expr: &MlExpr, out: &mut HashSet<String>) {
    match expr {
        MlExpr::Block { items, value } => {
            collect_bound_names(items, out);
            if let Some(v) = value {
                collect_names_in_expr(v, out);
            }
        }
        MlExpr::Lambda { body, .. }
        | MlExpr::Spawn(body)
        | MlExpr::Await(body)
        | MlExpr::Recv(body)
        | MlExpr::Paren(body) => collect_names_in_expr(body, out),
        MlExpr::App { func, arg } => {
            collect_names_in_expr(func, out);
            collect_names_in_expr(arg, out);
        }
        MlExpr::AppMulti { func, args } => {
            collect_names_in_expr(func, out);
            for a in args {
                collect_names_in_expr(a, out);
            }
        }
        MlExpr::UnitApp { func } => collect_names_in_expr(func, out),
        MlExpr::Binary { left, right, .. } => {
            collect_names_in_expr(left, out);
            collect_names_in_expr(right, out);
        }
        MlExpr::Unary { operand, .. } => collect_names_in_expr(operand, out),
        MlExpr::Index { target, index } => {
            collect_names_in_expr(target, out);
            collect_names_in_expr(index, out);
        }
        MlExpr::Field { target, .. } => collect_names_in_expr(target, out),
        MlExpr::Send { channel, value } => {
            collect_names_in_expr(channel, out);
            collect_names_in_expr(value, out);
        }
        MlExpr::Match { scrutinee, arms } => {
            collect_names_in_expr(scrutinee, out);
            for a in arms {
                collect_names_in_expr(&a.body, out);
            }
        }
        MlExpr::Handle { arms, body, .. } => {
            for a in arms {
                collect_names_in_expr(&a.body, out);
            }
            collect_names_in_expr(body, out);
        }
        MlExpr::Select(arms) => {
            for a in arms {
                collect_names_in_expr(&a.body, out);
            }
        }
        MlExpr::List(items) => {
            for i in items {
                collect_names_in_expr(i, out);
            }
        }
        MlExpr::Map(entries) => {
            for (k, v) in entries {
                collect_names_in_expr(k, out);
                collect_names_in_expr(v, out);
            }
        }
        MlExpr::Record { fields, .. } => {
            for f in fields {
                collect_names_in_expr(&f.value, out);
            }
        }
        MlExpr::Perform { args, .. } => {
            for a in args {
                collect_names_in_expr(a, out);
            }
        }
        MlExpr::Yield(Some(v)) | MlExpr::Resume(Some(v)) => collect_names_in_expr(v, out),
        _ => {}
    }
}

/// Lower a run of items, pairing each type signature with the binding of the
/// same name that immediately follows it. An orphaned signature (no matching
/// binding next) is dropped. Used at top level and inside layout blocks so
/// local signed functions work too.
fn lower_items(items: Vec<MlItem>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut pending: Option<MlSig> = None;
    // The most recent `(** … *)` doc comment, attached to the next declaration
    // ([DOC-SIGIL-ML]) — the same pairing pattern as a signature.
    let mut pending_doc: Option<DocComment> = None;
    for item in items {
        match item {
            MlItem::Doc(text) => {
                pending_doc = Some(crate::docparse::parse_doc(&text, DocScope::Outer));
            }
            MlItem::Signature {
                name,
                type_params,
                ty,
                effects,
            } => {
                pending = Some(MlSig {
                    name,
                    type_params,
                    ty,
                    effects,
                });
            }
            MlItem::Binding {
                mutable,
                name,
                params,
                uncurried,
                body,
                pos,
            } => {
                // Pair the binding with its preceding signature's type params,
                // type and effect row (all `None`/empty when unsigned), passed
                // as one `sig` argument so the lowerer stays within the
                // parameter budget.
                let sig = pending.take().filter(|s| s.name == name);
                let stmt = lower_binding(mutable, name, params, uncurried, body, pos, sig);
                out.push(attach_doc(stmt, pending_doc.take()));
            }
            MlItem::Assign { name, value, pos } => {
                pending = None;
                pending_doc = None;
                out.push(Stmt::Assignment {
                    name,
                    value: lower_expr(value),
                    position: Some(pos),
                });
            }
            MlItem::Type {
                name,
                type_params,
                variants,
                pos,
            } => {
                pending = None;
                out.push(Stmt::Type {
                    name,
                    type_params: type_params.into_iter().map(lower_type_param).collect(),
                    variants: variants.into_iter().map(lower_variant).collect(),
                    validation_func: None,
                    doc: pending_doc.take(),
                    position: Some(pos),
                });
            }
            MlItem::Extern {
                name,
                params,
                return_type,
                pos,
            } => {
                pending = None;
                out.push(Stmt::Extern {
                    name,
                    parameters: params.into_iter().map(lower_extern_param).collect(),
                    return_type: return_type.as_ref().and_then(type_expr),
                    doc: pending_doc.take(),
                    position: Some(pos),
                });
            }
            MlItem::Effect {
                name,
                type_params,
                operations,
                pos,
            } => {
                pending = None;
                out.push(Stmt::Effect {
                    name,
                    type_params: type_params.into_iter().map(lower_type_param).collect(),
                    operations: operations.into_iter().map(lower_effect_op).collect(),
                    doc: pending_doc.take(),
                    position: Some(pos),
                });
            }
            MlItem::Expr { value, pos } => {
                pending = None;
                pending_doc = None;
                out.push(Stmt::Expr {
                    value: lower_expr(value),
                    position: Some(pos),
                });
            }
        }
    }
    out
}

/// Attach a pending doc comment to the `Function`/`Let` a binding lowered to.
/// A binding lowers to exactly one of those two, so this sets whichever it is —
/// matching by mutable reference to the `doc` field only, no struct rebuild.
fn attach_doc(mut stmt: Stmt, doc: Option<DocComment>) -> Stmt {
    if let Stmt::Function { doc: slot, .. } | Stmt::Let { doc: slot, .. } = &mut stmt {
        *slot = doc;
    }
    stmt
}

/// Lower one `extern` parameter to a canonical [`ExternParameter`], threading its
/// declared type through the shared [`type_expr`] path so it is byte-identical to
/// the Default flavor's extern parameter ([FLAVOR-ML-EXTERN]). A type with no
/// canonical [`TypeExpr`] form (a tuple) falls back to its rendered surface name.
fn lower_extern_param(param: MlExternParam) -> ExternParameter {
    let ty = type_expr(&param.ty).unwrap_or_else(|| TypeExpr::named(render_type(&param.ty)));
    ExternParameter {
        name: param.name,
        ty,
    }
}

/// Lower one CST variant to a canonical [`TypeVariant`], rendering each field's
/// type to the same surface string the Default flavor stores ([FLAVOR-ML-TYPE]).
fn lower_variant(variant: MlVariant) -> TypeVariant {
    TypeVariant {
        name: variant.name,
        fields: variant.fields.into_iter().map(lower_type_field).collect(),
    }
}

/// Lower one `field : type` line, with `constraint: None` (ML has no `where`
/// clause on type fields yet) — byte-identical to the Default field shape.
fn lower_type_field(field: MlTypeField) -> TypeField {
    TypeField {
        name: field.name,
        ty: render_type(&field.ty),
        constraint: None,
    }
}

/// Render an [`MlType`] to the surface type string the Default flavor stores in
/// [`TypeField::ty`]: a bare name as itself, an application as `Head<a, b>`, and
/// a function type as `(arg) -> ret` — the parenthesised-argument spelling the
/// type checker's `convert.rs` accepts (`int -> bool` is rejected). The argument
/// side is always parenthesised (`(int)`, or a tuple's own `(a, b)`); the result
/// side is rendered bare so a curried tail reads `(int) -> (int) -> int`.
fn render_type(ty: &MlType) -> String {
    match ty {
        MlType::Name(name) => name.clone(),
        MlType::App { head, args } => {
            let rendered = args.iter().map(render_type).collect::<Vec<_>>().join(", ");
            format!("{head}<{rendered}>")
        }
        MlType::Arrow { from, to } => {
            format!("{} -> {}", render_arrow_arg(from), render_type(to))
        }
        MlType::Tuple(parts) => render_tuple(parts),
    }
}

/// Render an arrow's argument side, always parenthesised: a tuple keeps its own
/// `(a, b)` form; any other single type is wrapped as `(type)`.
fn render_arrow_arg(ty: &MlType) -> String {
    match ty {
        MlType::Tuple(parts) => render_tuple(parts),
        other => format!("({})", render_type(other)),
    }
}

/// Render a tuple type as `(a, b, …)`.
fn render_tuple(parts: &[MlType]) -> String {
    let rendered = parts.iter().map(render_type).collect::<Vec<_>>().join(", ");
    format!("({rendered})")
}

/// A binding with no parameters is a `let` (its signature becomes the binding's
/// type); one with parameters is a function. The `uncurried` flag selects the
/// surface form: `f (x, y) = …` (uncurried) builds one FLAT multi-parameter
/// `Function` (twinning Default `fn f(x, y)`); `f x y = …` (curried) builds a
/// one-parameter `Function` returning a `Lambda` chain ([FLAVOR-ML-CURRY]). The
/// unit marker `()` yields a zero-parameter function, matching `fn f() = …`.
fn lower_binding(
    mutable: bool,
    name: String,
    params: Vec<MlParam>,
    uncurried: bool,
    body: MlExpr,
    pos: Position,
    sig: Option<MlSig>,
) -> Stmt {
    let body = lower_expr(body);
    // Split the paired signature into its type params, declared type and
    // effect row.
    let (type_params, ty, effects) = match sig {
        Some(s) => (s.type_params, Some(s.ty), s.effects),
        None => (Vec::new(), None, Vec::new()),
    };
    let ty = ty.as_ref();
    // An empty surface parameter list is a value binding; a non-empty one (even
    // the lone unit marker `()`) is a function. `()` binds no canonical
    // parameter, so `f () = e` is a zero-parameter function like `fn f() = e`.
    // A value binding has no effect row, so the signature's effects are dropped.
    if params.is_empty() {
        return Stmt::Let {
            name,
            mutable,
            ty: ty.and_then(type_expr),
            value: body,
            doc: None,
            position: Some(pos),
        };
    }
    let (parameters, body, return_type) = if uncurried {
        build_function_flat(params, body, ty)
    } else {
        build_function(params, body, ty, pos)
    };
    Stmt::Function {
        name,
        type_params: type_params.into_iter().map(lower_type_param).collect(),
        parameters,
        return_type,
        effects: effects.into_iter().map(lower_effect_ref).collect(),
        body,
        doc: None,
        position: Some(pos),
    }
}

/// A parsed signature awaiting its binding: `name<T, U> : ty ! effects`.
struct MlSig {
    name: String,
    type_params: Vec<MlTypeParam>,
    ty: MlType,
    effects: Vec<MlEffectRef>,
}

/// Lower one CST type parameter to the canonical variance-carrying
/// [`TypeParam`] — byte-identical to the Default flavor's lowering.
/// Implements [TYPE-VARIANCE-DECL].
fn lower_type_param(p: MlTypeParam) -> TypeParam {
    TypeParam {
        name: p.name,
        variance: match p.variance {
            MlVariance::Invariant => Variance::Invariant,
            MlVariance::Covariant => Variance::Covariant,
            MlVariance::Contravariant => Variance::Contravariant,
        },
    }
}

/// Lower one effect-row reference to the canonical [`EffectRef`], threading
/// its type arguments through the shared [`type_expr`] path so they are
/// byte-identical to the Default flavor's. Implements [EFFECTS-GENERIC-ROWS].
fn lower_effect_ref(r: MlEffectRef) -> EffectRef {
    EffectRef {
        name: r.name,
        type_args: r
            .args
            .iter()
            .map(|a| type_expr(a).unwrap_or_else(|| TypeExpr::named(render_type(a))))
            .collect(),
        position: Some(r.pos),
    }
}

/// Build a FLAT multi-parameter function (`f (x, y) = …`, the uncurried form):
/// every surface parameter becomes a real canonical parameter, typed positionally
/// from the signature spine, and the return type is the spine tail left after
/// them — byte-identical to the Default `fn f(x, y) -> r` ([FLAVOR-ML-CURRY]).
fn build_function_flat(
    params: Vec<MlParam>,
    body: Expr,
    sig: Option<&MlType>,
) -> (Vec<Parameter>, Expr, Option<TypeExpr>) {
    let spine = expand_tuple_head(sig.map(arrow_spine).unwrap_or_default(), params.len());
    let consumed = params.len();
    let parameters = params
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| match p {
            MlParam::Named(name) => Some(Parameter {
                name,
                ty: spine.get(i).and_then(type_expr),
            }),
            MlParam::Typed(name, ty) => Some(Parameter {
                name,
                ty: type_expr(&ty),
            }),
            MlParam::Unit => None,
        })
        .collect();
    (
        parameters,
        body,
        arrow_of(spine.get(consumed..).unwrap_or(&[])),
    )
}

/// An uncurried binding may be signed with the tuple spelling
/// `(T, U) -> R`: the tuple's parts type the parameters one-to-one and the
/// rest of the spine is the return — matching the Default flavor's
/// `fn f(a: T, b: U) -> R` exactly ([FLAVOR-ML-GENERICS]). A curried spine
/// (`T -> U -> R`) keeps typing parameters positionally.
fn expand_tuple_head(spine: Vec<MlType>, param_count: usize) -> Vec<MlType> {
    match spine.split_first() {
        Some((MlType::Tuple(parts), rest)) if parts.len() == param_count => {
            parts.iter().cloned().chain(rest.iter().cloned()).collect()
        }
        _ => spine,
    }
}

/// Lower one `op : P => R` effect operation line to the canonical
/// [`EffectOperation`], rendering the payload/result into the `fn(P) -> R`
/// surface string the Default flavor emits ([FLAVOR-ML-EFFECT]). `parameters`
/// and `return_type` stay empty/blank, matching the Default-flavor shape.
fn lower_effect_op(op: MlEffectOp) -> EffectOperation {
    EffectOperation {
        ty: format!(
            "fn({}) -> {}",
            render_op_payload(&op.payload),
            render_type(&op.result)
        ),
        name: op.name,
        parameters: Vec::new(),
        return_type: String::new(),
    }
}

/// The payload name that denotes a zero-argument effect operation: `Unit => R`
/// in ML mirrors the Default flavor's `fn() -> R` (no argument), so it must
/// render to an EMPTY payload — not `fn(Unit) -> R`, which the codegen would
/// count as one argument and emit a differently-typed handler thunk.
const UNIT_PAYLOAD: &str = "Unit";

/// Render a multi-argument effect-operation payload `(P1, P2, …)` as the bare
/// comma-separated `P1, P2, …` the Default flavor's `fn(P1, P2, …) -> R` op
/// signature carries — NOT the parenthesised tuple form — so the canonical
/// operation `ty` (and the per-argument types inference recovers from it) is
/// byte-identical across flavors ([FLAVOR-ML-EFFECT]). A bare `Unit` payload is
/// the zero-argument boundary and renders empty (`fn() -> R`); a single
/// (non-tuple, non-Unit) payload renders normally.
fn render_op_payload(ty: &MlType) -> String {
    match ty {
        MlType::Name(name) if name == UNIT_PAYLOAD => String::new(),
        MlType::Tuple(parts) => parts.iter().map(render_type).collect::<Vec<_>>().join(", "),
        other => render_type(other),
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
fn build_function(
    params: Vec<MlParam>,
    body: Expr,
    sig: Option<&MlType>,
    pos: Position,
) -> (Vec<Parameter>, Expr, Option<TypeExpr>) {
    let spine = sig.map(arrow_spine).unwrap_or_default();
    let mut rest = params.into_iter();
    let first = rest.next();
    let parameters = match first {
        Some(MlParam::Named(name)) => vec![Parameter {
            name,
            ty: spine.first().and_then(type_expr),
        }],
        Some(MlParam::Typed(name, ty)) => vec![Parameter {
            name,
            ty: type_expr(&ty),
        }],
        // `()` (unit marker) or no parameter binds nothing.
        _ => Vec::new(),
    };
    let tail_spine = spine.get(1..).unwrap_or(&[]);
    let body = curry_params(rest.collect(), body, tail_spine, pos);
    (parameters, body, arrow_of(tail_spine))
}

/// Fold a surface parameter list into a right-nested chain of one-parameter
/// lambdas over `body` (curry-by-default, [FLAVOR-ML-CURRY]): `[x, y]` over `b`
/// becomes `Lambda{[x], Lambda{[y], b}}`. `spine` supplies each parameter's type
/// positionally; the lambda for the i-th parameter returns the function-typed
/// tail `arrow_of(spine[i+1..])`. An empty parameter list returns `body`
/// unchanged (the curried tail of a single-parameter function is just its body).
fn curry_params(params: Vec<MlParam>, body: Expr, spine: &[MlType], pos: Position) -> Expr {
    let mut acc = body;
    for (i, param) in params.into_iter().enumerate().rev() {
        let parameters = match param {
            MlParam::Named(name) => vec![Parameter {
                name,
                ty: spine.get(i).and_then(type_expr),
            }],
            MlParam::Typed(name, ty) => vec![Parameter {
                name,
                ty: type_expr(&ty),
            }],
            MlParam::Unit => Vec::new(),
        };
        acc = Expr::Lambda {
            parameters,
            return_type: arrow_of(spine.get(i + 1..).unwrap_or(&[])),
            body: Box::new(acc),
            position: Some(pos),
        };
    }
    acc
}

/// Lower a lambda head over an already-lowered `body`. A unit-only or empty head
/// `\() => body` / `\=> body` is a single zero-parameter lambda (nothing to
/// curry); otherwise the parameters curry into nested one-parameter lambdas
/// ([FLAVOR-ML-CURRY]), byte-identical to the Default `fn(x) => fn(y) => body`.
/// Convert a surface parameter list to canonical parameters for a FLAT lambda
/// (the uncurried `\(x, y) =>` head): named/typed params become real parameters,
/// the unit marker `()` contributes none.
fn flat_params(params: Vec<MlParam>) -> Vec<Parameter> {
    params
        .into_iter()
        .filter_map(|p| match p {
            MlParam::Named(name) => Some(Parameter { name, ty: None }),
            MlParam::Typed(name, ty) => Some(Parameter {
                name,
                ty: type_expr(&ty),
            }),
            MlParam::Unit => None,
        })
        .collect()
}

fn lower_lambda(params: Vec<MlParam>, body: Expr, pos: Position) -> Expr {
    if params.iter().all(|p| matches!(p, MlParam::Unit)) {
        return Expr::Lambda {
            parameters: Vec::new(),
            return_type: None,
            body: Box::new(body),
            position: Some(pos),
        };
    }
    curry_params(params, body, &[], pos)
}

/// Flatten the top-level arrow spine of a type: `a -> b -> c` ⇒ `[a, b, c]`,
/// `(a, b) -> c` ⇒ `[(a,b), c]`, a non-arrow ⇒ a single-element list.
fn arrow_spine(ty: &MlType) -> Vec<MlType> {
    match ty {
        MlType::Arrow { from, to } => {
            let mut spine = vec![(**from).clone()];
            spine.extend(arrow_spine(to));
            spine
        }
        other => vec![other.clone()],
    }
}

/// Rebuild a right-associative function type from an arrow-spine slice: `[]` ⇒
/// no type, `[t]` ⇒ `t`, `[a, b, …]` ⇒ `a -> (b -> …)`.
fn arrow_of(slice: &[MlType]) -> Option<TypeExpr> {
    match slice {
        [] => None,
        [single] => type_expr(single),
        [first, rest @ ..] => Some(TypeExpr {
            name: "fn".to_owned(),
            generic_params: Vec::new(),
            is_array: false,
            array_element: None,
            is_function: true,
            parameter_types: vec![type_expr(first)?],
            return_type: Some(Box::new(arrow_of(rest)?)),
            position: None,
        }),
    }
}

/// Convert an ML type to a canonical [`TypeExpr`]. A tuple type has no canonical
/// `TypeExpr` form, so it (and anything containing one) yields `None` — leaving
/// that position to inference rather than annotating it wrongly.
fn type_expr(ty: &MlType) -> Option<TypeExpr> {
    match ty {
        MlType::Name(name) => Some(TypeExpr::named(name.clone())),
        MlType::App { head, args } => {
            let generic_params = args.iter().map(type_expr).collect::<Option<Vec<_>>>()?;
            Some(TypeExpr {
                generic_params,
                ..TypeExpr::named(head.clone())
            })
        }
        MlType::Arrow { from, to } => Some(TypeExpr {
            name: "fn".to_owned(),
            generic_params: Vec::new(),
            is_array: false,
            array_element: None,
            is_function: true,
            parameter_types: vec![type_expr(from)?],
            return_type: Some(Box::new(type_expr(to)?)),
            position: None,
        }),
        MlType::Tuple(_) => None,
    }
}

/// Lower one CST expression to a canonical [`Expr`].
fn lower_expr(expr: MlExpr) -> Expr {
    match expr {
        MlExpr::Int(n) => Expr::Integer(n),
        MlExpr::Float(f) => Expr::Float(f),
        MlExpr::Bool(b) => Expr::Bool(b),
        MlExpr::Str(raw) => lower_string(&raw),
        MlExpr::Ident(name) => Expr::Identifier(name),
        MlExpr::Paren(inner) => lower_expr(*inner),
        MlExpr::Unary { op, operand } => Expr::Unary {
            op,
            operand: Box::new(lower_expr(*operand)),
        },
        MlExpr::Binary { op, left, right } => lower_binary(&op, *left, *right),
        MlExpr::App { func, arg } => lower_application(*func, *arg),
        // `func (a, b, …)` — the uncurried saturated call lowers to one flat
        // multi-argument `Call`, byte-identical to the Default `func(a, b, …)`
        // ([FLAVOR-ML-CALL]).
        MlExpr::AppMulti { func, args } => call(
            lower_expr(*func),
            args.into_iter().map(lower_expr).collect(),
        ),
        MlExpr::UnitApp { func } => call(lower_expr(*func), Vec::new()),
        MlExpr::List(items) => Expr::List(items.into_iter().map(lower_expr).collect()),
        MlExpr::Map(entries) => Expr::Map(entries.into_iter().map(lower_map_entry).collect()),
        MlExpr::Index { target, index } => Expr::Index {
            target: Box::new(lower_expr(*target)),
            index: Box::new(lower_expr(*index)),
        },
        MlExpr::Field { target, name } => Expr::FieldAccess {
            target: Box::new(lower_expr(*target)),
            field: name,
        },
        // A multi-parameter lambda `\x y => body` curries into nested
        // one-parameter lambdas ([FLAVOR-ML-CURRY]); an empty/unit head stays a
        // single zero-parameter lambda.
        // `\(x, y) => body` (uncurried) is one flat multi-parameter lambda;
        // `\x y => body` (curried) nests one-parameter lambdas ([FLAVOR-ML-CURRY]).
        MlExpr::Lambda {
            params,
            uncurried,
            body,
            pos,
        } => {
            let body = lower_expr(*body);
            if uncurried {
                Expr::Lambda {
                    parameters: flat_params(params),
                    return_type: None,
                    body: Box::new(body),
                    position: Some(pos),
                }
            } else {
                lower_lambda(params, body, pos)
            }
        }
        MlExpr::Match { scrutinee, arms } => Expr::Match {
            value: Box::new(lower_expr(*scrutinee)),
            arms: arms.into_iter().map(lower_arm).collect(),
        },
        MlExpr::Record {
            name,
            type_args,
            fields,
        } => Expr::TypeConstructor {
            name,
            // Thread explicit construction-site type arguments through the
            // shared `type_expr` path — byte-identical to the Default
            // flavor's `Ctor<t> { … }`. Implements [FLAVOR-ML-GENERICS].
            type_args: type_args
                .iter()
                .map(|a| type_expr(a).unwrap_or_else(|| TypeExpr::named(render_type(a))))
                .collect(),
            fields: fields.into_iter().map(lower_field).collect(),
        },
        MlExpr::Block { items, value } => lower_block(items, value),
        MlExpr::Spawn(body) => Expr::Spawn(Box::new(lower_expr(*body))),
        MlExpr::Perform {
            effect,
            operation,
            args,
            pos,
        } => Expr::Perform {
            effect,
            operation,
            arguments: args.into_iter().map(lower_expr).collect(),
            named_arguments: Vec::new(),
            position: Some(pos),
        },
        MlExpr::Handle {
            effect,
            arms,
            body,
            pos,
        } => Expr::Handler {
            effect,
            arms: arms.into_iter().map(lower_handle_arm).collect(),
            body: Box::new(lower_expr(*body)),
            position: Some(pos),
        },
        MlExpr::Resume(value) => Expr::Resume(value.map(|e| Box::new(lower_expr(*e)))),
        MlExpr::Await(inner) => Expr::Await(Box::new(lower_expr(*inner))),
        MlExpr::Yield(value) => Expr::Yield(value.map(|e| Box::new(lower_expr(*e)))),
        MlExpr::Send { channel, value } => Expr::Send {
            channel: Box::new(lower_expr(*channel)),
            value: Box::new(lower_expr(*value)),
        },
        MlExpr::Recv(inner) => Expr::Recv(Box::new(lower_expr(*inner))),
        MlExpr::Select(arms) => Expr::Select {
            arms: arms.into_iter().map(lower_arm).collect(),
        },
    }
}

/// Lower one `op param* => body` handle arm to the canonical [`HandlerArm`] —
/// byte-identical to the Default handler arm ([FLAVOR-ML-EFFECT]).
fn lower_handle_arm(arm: MlHandleArm) -> HandlerArm {
    HandlerArm {
        operation: arm.operation,
        params: arm.params,
        body: lower_expr(arm.body),
    }
}

/// `|>` desugars to a call (the pipe is invisible downstream); every other
/// operator is a canonical [`Expr::Binary`].
fn lower_binary(op: &str, left: MlExpr, right: MlExpr) -> Expr {
    let left = lower_expr(left);
    let right = lower_expr(right);
    if op == "|>" {
        return pipe_into(left, right);
    }
    Expr::Binary {
        op: op.to_owned(),
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// A block lowers to [`Expr::Block`]; a block that is a single trailing value
/// with no statements unwraps to that value, so it is structurally identical to
/// the Default inline body.
fn lower_block(items: Vec<MlItem>, value: Option<Box<MlExpr>>) -> Expr {
    let statements = lower_items(items);
    let value = value.map(|v| Box::new(lower_expr(*v)));
    match (statements.is_empty(), value) {
        (true, Some(value)) => *value,
        (_, value) => Expr::Block { statements, value },
    }
}

fn lower_arm(arm: MlArm) -> MatchArm {
    MatchArm {
        pattern: lower_pattern(arm.pattern),
        body: lower_expr(arm.body),
    }
}

fn lower_pattern(pattern: MlPattern) -> Pattern {
    match pattern {
        MlPattern::Wildcard => Pattern::Wildcard,
        MlPattern::Int(n) => Pattern::Literal(Box::new(Expr::Integer(n))),
        MlPattern::Str(raw) => Pattern::Literal(Box::new(lower_string(&raw))),
        MlPattern::Bool(b) => Pattern::Literal(Box::new(Expr::Bool(b))),
        MlPattern::Bind(name) => Pattern::Binding(name),
        MlPattern::Ctor { name, fields } => Pattern::Constructor {
            name,
            fields,
            sub_patterns: Vec::new(),
        },
        MlPattern::List { elements, rest } => Pattern::List {
            elements: elements.into_iter().map(lower_pattern).collect(),
            rest,
        },
    }
}

fn lower_field(field: MlField) -> FieldAssignment {
    FieldAssignment {
        name: field.name,
        value: lower_expr(field.value),
    }
}

/// Lower one `key => value` map entry to a canonical [`MapEntry`] — byte-identical
/// to the Default `{ key: value }` entry ([FLAVOR-ML-MAP]).
fn lower_map_entry((key, value): (MlExpr, MlExpr)) -> MapEntry {
    MapEntry {
        key: lower_expr(key),
        value: lower_expr(value),
    }
}

/// `x |> f a` → `f x a`: prepend the piped value as the first argument of the
/// right-hand call, or wrap a bare callee in a one-argument call.
fn pipe_into(left: Expr, right: Expr) -> Expr {
    match right {
        Expr::Call {
            function,
            mut arguments,
            named_arguments,
        } => {
            arguments.insert(0, left);
            Expr::Call {
                function,
                arguments,
                named_arguments,
            }
        }
        callee => call(callee, vec![left]),
    }
}

/// Build a single positional [`Expr::Call`] node.
fn call(function: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(function),
        arguments,
        named_arguments: Vec::new(),
    }
}

/// Lower a whitespace-application spine ([FLAVOR-ML-CURRY]). The spine
/// `((head a) b) c` is collected into `(head, [a, b, c])`, then:
/// - if `head` is a user binding (or a non-identifier callee like a closure
///   value), the spine stays CURRIED — nested one-argument calls
///   `Call(Call(Call(head,[a]),[b]),[c])` — so partial application works and the
///   form is byte-identical to the Default explicit-curry `head(a)(b)(c)`;
/// - otherwise `head` is a multi-argument builtin or `extern` that cannot be
///   partially applied, so the SATURATED spine folds to ONE flat call
///   `Call(head, [a, b, c])` — the saturated-call optimisation the spec assigns
///   to the backend, applied here while the surface spine is still visible.
fn lower_application(func: MlExpr, arg: MlExpr) -> Expr {
    let mut args = vec![arg];
    let mut head = func;
    while let MlExpr::App { func, arg } = head {
        args.push(*arg);
        head = *func;
    }
    args.reverse();
    let curried = match &head {
        MlExpr::Ident(name) => BOUND_NAMES.with(|s| s.borrow().contains(name)),
        _ => true,
    };
    if curried {
        args.into_iter()
            .fold(lower_expr(head), |acc, a| call(acc, vec![lower_expr(a)]))
    } else {
        call(lower_expr(head), args.into_iter().map(lower_expr).collect())
    }
}

/// Lower a raw string token to a plain or interpolated string expression,
/// reusing the Default frontend's escape/`${…}` handling with an ML fragment
/// parser ([FLAVOR-FRONTEND]).
fn lower_string(raw: &str) -> Expr {
    if raw.contains("${") {
        Expr::InterpolatedStr(lower_interpolation(raw, parse_fragment))
    } else {
        Expr::Str(unquote(raw))
    }
}

/// Parse a `${…}` fragment as an ML expression (`${toString id}` is ML
/// application), threading the flavor through interpolation re-entry.
fn parse_fragment(frag: &str) -> Expr {
    let (items, _) = super::parser::parse(&format!("__frag__ = {frag}\n"));
    match items.into_iter().next() {
        Some(MlItem::Binding { body, .. }) => lower_expr(body),
        _ => Expr::Identifier(frag.trim().to_owned()),
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a panic"
)]
mod tests {
    use super::super::parse_ml;
    use osprey_ast::{Expr, InterpolatedPart, Pattern, Stmt, Variance};

    fn stmts(src: &str) -> Vec<Stmt> {
        let parsed = parse_ml(src);
        assert!(parsed.errors.is_empty(), "ml errors: {:?}", parsed.errors);
        parsed.program.statements
    }

    fn one(src: &str) -> Stmt {
        let mut s = stmts(src);
        assert_eq!(s.len(), 1, "expected exactly one statement: {s:?}");
        // `remove(0)` is panic-free given the length assertion above and avoids
        // the forbidden `unwrap()` ([USER-MANDATE-NO-PANIC-IN-TESTS]).
        s.remove(0)
    }

    #[test]
    fn value_binding_lowers_to_let() {
        let s = one("answer = 42\n");
        assert!(matches!(s, Stmt::Let { .. }), "expected let, got {s:?}");
        if let Stmt::Let {
            name,
            mutable,
            value,
            ..
        } = s
        {
            assert_eq!(name, "answer");
            assert!(!mutable);
            assert_eq!(value, Expr::Integer(42));
        }
    }

    #[test]
    fn mut_and_assignment_lower_distinctly() {
        let s = stmts("mut requests = 0\nrequests := requests + 1\n");
        assert!(matches!(s[0], Stmt::Let { mutable: true, .. }));
        assert!(matches!(s[1], Stmt::Assignment { ref name, .. } if name == "requests"));
    }

    #[test]
    fn multi_param_function_is_curried_nested_lambda() {
        // `add x y = x + y` curries by default ([FLAVOR-ML-CURRY]): a ONE-parameter
        // `Stmt::Function` over `x` whose body is a one-parameter `Expr::Lambda`
        // over `y` — byte-identical to the Default *explicit-curry*
        // `fn add(x) = fn(y) => x + y`, deliberately NOT the multi-parameter
        // `fn add(x, y)`.
        let s = one("add x y = x + y\n");
        assert!(
            matches!(s, Stmt::Function { .. }),
            "expected function, got {s:?}"
        );
        if let Stmt::Function {
            name,
            parameters,
            body,
            ..
        } = s
        {
            assert_eq!(name, "add");
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "x");
            assert!(
                matches!(&body, Expr::Lambda { parameters, .. }
                    if parameters.len() == 1 && parameters[0].name == "y"),
                "expected curried lambda over y, got {body:?}"
            );
            if let Expr::Lambda { body: inner, .. } = body {
                assert!(matches!(*inner, Expr::Binary { ref op, .. } if op == "+"));
            }
        }
    }

    #[test]
    fn single_param_function_has_no_extra_lambda() {
        let s = one("inc x = x + 1\n");
        assert!(
            matches!(s, Stmt::Function { .. }),
            "expected function, got {s:?}"
        );
        if let Stmt::Function {
            parameters, body, ..
        } = s
        {
            assert_eq!(parameters.len(), 1);
            assert!(matches!(body, Expr::Binary { .. }));
        }
    }

    #[test]
    fn unit_function_has_zero_parameters() {
        // `f () = body` is a zero-parameter function, like the Default `fn f()`.
        let s = one("greet () = 1\n");
        assert!(
            matches!(s, Stmt::Function { .. }),
            "expected function, got {s:?}"
        );
        if let Stmt::Function { parameters, .. } = s {
            assert!(parameters.is_empty());
        }
    }

    #[test]
    fn whitespace_application_is_curried_nested_call() {
        // A user-defined `add` curries by default ([FLAVOR-ML-CURRY]): the
        // whitespace call `add 1 2` is nested one-argument calls
        // `Call(Call(add, [1]), [2])` — byte-identical to the Default explicit-curry
        // `add(1)(2)`, NOT a flat `add(1, 2)`. (An UNBOUND head is treated as a
        // multi-argument builtin and folds to a flat saturated call instead.)
        let value = stmts("add a b = a + b\nr = add 1 2\n")
            .into_iter()
            .find_map(|st| match st {
                Stmt::Let { name, value, .. } if name == "r" => Some(value),
                _ => None,
            });
        match value {
            Some(Expr::Call {
                function,
                arguments,
                ..
            }) => {
                // The outer call applies the inner `(add 1)` to the single argument `2`.
                assert_eq!(arguments, vec![Expr::Integer(2)]);
                assert!(
                    matches!(&*function, Expr::Call { arguments, .. }
                        if arguments == &vec![Expr::Integer(1)]),
                    "expected inner call add(1), got {function:?}"
                );
                if let Expr::Call {
                    function: inner, ..
                } = *function
                {
                    assert_eq!(*inner, Expr::Identifier("add".to_owned()));
                }
            }
            other => panic!("expected nested curried call for r, got {other:?}"),
        }
    }

    #[test]
    fn application_binds_tighter_than_operators() {
        // `add 1 2 == 3` ⇒ (add 1 2) == 3.
        let s = one("r = add 1 2 == 3\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Binary { .. },
                    ..
                }
            ),
            "expected comparison, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Binary { op, left, right },
            ..
        } = s
        {
            assert_eq!(op, "==");
            assert!(matches!(*left, Expr::Call { .. }));
            assert_eq!(*right, Expr::Integer(3));
        }
    }

    #[test]
    fn unit_application_is_zero_arg_call() {
        let s = one("r = make ()\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Call { .. },
                    ..
                }
            ),
            "expected zero-arg call, got {s:?}"
        );
        if let Stmt::Let {
            value:
                Expr::Call {
                    function,
                    arguments,
                    ..
                },
            ..
        } = s
        {
            assert!(arguments.is_empty());
            assert_eq!(*function, Expr::Identifier("make".to_owned()));
        }
    }

    #[test]
    fn match_lowers_constructor_and_wildcard_arms() {
        let s = one("r =\n    match x\n        Success value => value\n        _ => 0\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Match { .. },
                    ..
                }
            ),
            "expected match, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = s
        {
            assert_eq!(arms.len(), 2);
            let p0 = &arms[0].pattern;
            assert!(
                matches!(p0, Pattern::Constructor { .. }),
                "expected constructor pattern, got {p0:?}"
            );
            if let Pattern::Constructor { name, fields, .. } = p0 {
                assert_eq!(name, "Success");
                assert_eq!(fields, &vec!["value".to_owned()]);
            }
            assert!(matches!(arms[1].pattern, Pattern::Wildcard));
        }
    }

    #[test]
    fn list_patterns_lower_to_canonical_list_pattern() {
        // `[]`, `[x]`, `[a, b]`, `[head, ...tail]`, `[_, b, ...rest]` lower to the
        // SAME `Pattern::List { elements, rest }` the Default flavor emits
        // ([FLAVOR-ML-MATCH], [TYPE-LIST-PATTERNS]).
        let src = "r =\n    match xs\n        [] => 0\n        [head, ...tail] => 1\n        [_, b, ...rest] => 2\n";
        let s = one(src);
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Match { .. },
                    ..
                }
            ),
            "expected match, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } = s
        {
            assert!(
                matches!(&arms[0].pattern, Pattern::List { elements, rest } if elements.is_empty() && rest.is_none())
            );
            let p1 = &arms[1].pattern;
            assert!(
                matches!(p1, Pattern::List { .. }),
                "expected list pattern, got {p1:?}"
            );
            if let Pattern::List { elements, rest } = p1 {
                assert_eq!(elements, &vec![Pattern::Binding("head".to_owned())]);
                assert_eq!(rest, &Some("tail".to_owned()));
            }
            let p2 = &arms[2].pattern;
            assert!(
                matches!(p2, Pattern::List { .. }),
                "expected list pattern, got {p2:?}"
            );
            if let Pattern::List { elements, rest } = p2 {
                assert_eq!(elements.len(), 2);
                assert!(matches!(elements[0], Pattern::Wildcard));
                assert_eq!(elements[1], Pattern::Binding("b".to_owned()));
                assert_eq!(rest, &Some("rest".to_owned()));
            }
        }
    }

    #[test]
    fn lambda_is_curried_and_pipe_desugars_to_call() {
        // `\x y => x + y` curries by default ([FLAVOR-ML-CURRY]): a one-parameter
        // `Expr::Lambda` over `x` whose body is a one-parameter lambda over `y` —
        // byte-identical to the Default explicit-curry `fn(x) => fn(y) => x + y`,
        // not a single two-parameter lambda.
        let s = one("f = \\x y => x + y\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Lambda { .. },
                    ..
                }
            ),
            "expected lambda, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Lambda {
                parameters, body, ..
            },
            ..
        } = s
        {
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "x");
            assert!(
                matches!(&*body, Expr::Lambda { parameters, .. }
                    if parameters.len() == 1 && parameters[0].name == "y"),
                "expected curried inner lambda over y, got {body:?}"
            );
            if let Expr::Lambda { body: inner, .. } = *body {
                assert!(matches!(*inner, Expr::Binary { ref op, .. } if op == "+"));
            }
        }
        // `x |> f` becomes `f(x)` — no Pipe node survives, matching Default.
        let piped = one("r = x |> f\n");
        assert!(
            matches!(
                piped,
                Stmt::Let {
                    value: Expr::Call { .. },
                    ..
                }
            ),
            "expected piped call, got {piped:?}"
        );
        if let Stmt::Let {
            value:
                Expr::Call {
                    function,
                    arguments,
                    ..
                },
            ..
        } = piped
        {
            assert_eq!(*function, Expr::Identifier("f".to_owned()));
            assert_eq!(arguments, vec![Expr::Identifier("x".to_owned())]);
        }
    }

    #[test]
    fn record_block_lowers_to_type_constructor() {
        let src = "p =\n    Point\n        x = 1\n        y = 2\n";
        let s = one(src);
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::TypeConstructor { .. },
                    ..
                }
            ),
            "expected type constructor, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::TypeConstructor { name, fields, .. },
            ..
        } = s
        {
            assert_eq!(name, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "x");
        }
    }

    #[test]
    fn inline_record_lowers_to_type_constructor() {
        // `Ok(value = "x")` in expression position is an inline record literal —
        // it lowers to the SAME `Expr::TypeConstructor` the layout form and the
        // Default `Ok { value: "x" }` produce ([FLAVOR-ML-RECORD]).
        let s = one("r = Ok(value = \"x\")\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::TypeConstructor { .. },
                    ..
                }
            ),
            "expected type constructor, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::TypeConstructor { name, fields, .. },
            ..
        } = s
        {
            assert_eq!(name, "Ok");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "value");
            assert_eq!(fields[0].value, Expr::Str("x".to_owned()));
        }
    }

    #[test]
    fn lowercase_inline_record_lowers_to_update_type_constructor() {
        // `receiver(field = v)` with a LOWERCASE head is a non-destructive record
        // update; it lowers to the SAME `Expr::TypeConstructor { name: receiver }`
        // the Default `receiver { field: v }` produces ([FLAVOR-ML-RECORD]).
        let s = one("p2 = point1(x = 30)\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::TypeConstructor { .. },
                    ..
                }
            ),
            "expected update type constructor, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::TypeConstructor { name, fields, .. },
            ..
        } = s
        {
            assert_eq!(name, "point1");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "x");
            assert_eq!(fields[0].value, Expr::Integer(30));
        }
    }

    #[test]
    fn map_literal_lowers_to_canonical_map() {
        // `["a" => 1, "b" => 2]` lowers to the SAME `Expr::Map` the Default
        // `{ "a": 1, "b": 2 }` produces ([FLAVOR-ML-MAP]).
        let s = one("m = [\"a\" => 1, \"b\" => 2]\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Map(_),
                    ..
                }
            ),
            "expected map, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Map(entries),
            ..
        } = s
        {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].key, Expr::Str("a".to_owned()));
            assert_eq!(entries[0].value, Expr::Integer(1));
            assert_eq!(entries[1].key, Expr::Str("b".to_owned()));
        }
        // `[=>]` is the explicit empty-map form.
        assert!(matches!(
            one("m = [=>]\n"),
            Stmt::Let { value: Expr::Map(ref e), .. } if e.is_empty()
        ));
    }

    #[test]
    fn generic_type_annotation_lowers_to_generic_params() {
        // `empty : List<string>` flows the angle-bracketed generic argument into
        // `TypeExpr.generic_params`, byte-identical to the Default annotation.
        let s = stmts("empty : List<string>\nempty = []\n");
        let first = s.first();
        assert!(
            matches!(first, Some(Stmt::Let { ty: Some(_), .. })),
            "expected typed let, got {first:?}"
        );
        if let Some(Stmt::Let { ty: Some(ty), .. }) = first {
            assert_eq!(ty.name, "List");
            assert_eq!(ty.generic_params.len(), 1);
            assert_eq!(ty.generic_params[0].name, "string");
        }
    }

    #[test]
    fn fn_typed_field_renders_with_parenthesised_arg() {
        // A function-typed record field renders as `(int) -> bool` — the spelling
        // the type checker accepts — not the bare `int -> bool` ([FLAVOR-ML-TYPE]).
        let s = one("type Checker =\n    check : (int) -> bool\n");
        assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
        if let Stmt::Type { variants, .. } = s {
            assert_eq!(variants[0].fields[0].ty, "(int) -> bool");
        }
    }

    #[test]
    fn spawn_inline_expr_lowers_to_spawn() {
        // `spawn f x` lowers to `Expr::Spawn` wrapping the call, byte-identical
        // to the Default `spawn f(x)` ([FLAVOR-ML-SPAWN]).
        let s = one("r = spawn task 1\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Spawn(_),
                    ..
                }
            ),
            "expected spawn, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Spawn(inner),
            ..
        } = s
        {
            assert!(
                matches!(*inner, Expr::Call { .. }),
                "spawn body should be the call, got {inner:?}"
            );
        }
    }

    #[test]
    fn spawn_block_lowers_to_spawn_block() {
        // `spawn` + an indented block lowers to `Expr::Spawn` wrapping the block.
        let s = one("r = spawn\n    x = 1\n    task x\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Spawn(_),
                    ..
                }
            ),
            "expected spawn, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Spawn(inner),
            ..
        } = s
        {
            assert!(
                matches!(*inner, Expr::Block { .. }),
                "spawn block body should be a Block, got {inner:?}"
            );
        }
    }

    #[test]
    fn interpolation_parses_fragment_as_ml_application() {
        // `${toString id}` is ML whitespace application inside the fragment.
        let s = one("r = \"n=${toString id}\"\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::InterpolatedStr(_),
                    ..
                }
            ),
            "expected interpolated string, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::InterpolatedStr(parts),
            ..
        } = s
        {
            assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "n="));
            assert!(matches!(
                parts[1],
                InterpolatedPart::Expr(Expr::Call { .. })
            ));
        }
    }

    #[test]
    fn block_body_with_statements_keeps_block_with_trailing_value() {
        let src = "f x =\n    y = x + 1\n    y + 2\n";
        let s = one(src);
        assert!(
            matches!(
                s,
                Stmt::Function {
                    body: Expr::Block { .. },
                    ..
                }
            ),
            "expected block body, got {s:?}"
        );
        if let Stmt::Function {
            body: Expr::Block { statements, value },
            ..
        } = s
        {
            assert_eq!(statements.len(), 1);
            assert!(value.is_some());
        }
    }

    #[test]
    fn name_binding_is_an_immutable_let_at_top_level_and_in_a_block() {
        // The cross-flavor guarantee `name = expr` must satisfy: it lowers to the
        // SAME node a Default `let name = expr` does — `Stmt::Let { mutable:
        // false }` — both at the top level and inside a layout block. This is the
        // structural precondition for byte-identical IR with the Default twin
        // ([FLAVOR-CURRY], [FLAVOR-IR-EQUIV]); only `mut`+`:=` produces an
        // `Assignment`, never a bare `=`.
        assert!(matches!(
            one("answer = 41 + 1\n"),
            Stmt::Let { mutable: false, .. }
        ));
        // Same binding, this time the first statement of a function block.
        let s = one("main () =\n    answer = 41 + 1\n    answer\n");
        assert!(
            matches!(
                s,
                Stmt::Function {
                    body: Expr::Block { .. },
                    ..
                }
            ),
            "expected function with block body, got {s:?}"
        );
        if let Stmt::Function {
            body: Expr::Block { statements, .. },
            ..
        } = s
        {
            assert!(
                matches!(statements.first(), Some(Stmt::Let { mutable: false, name, .. }) if name == "answer"),
                "block-local `name = expr` must be an immutable Let, got {statements:?}"
            );
        }
    }

    #[test]
    fn union_type_lowers_to_canonical_type_stmt() {
        // The ML layout union must lower to the SAME `Stmt::Type` the Default
        // `type Outcome = Ok { value: string } | Err { message: string }` emits:
        // two payload-carrying variants with `validation_func: None` and each
        // field `constraint: None` ([FLAVOR-ML-TYPE], [FLAVOR-IR-EQUIV]).
        let src =
            "type Outcome =\n    Ok\n        value : string\n    Err\n        message : string\n";
        let s = one(src);
        assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
        if let Stmt::Type {
            name,
            type_params,
            variants,
            validation_func,
            ..
        } = s
        {
            assert_eq!(name, "Outcome");
            assert!(type_params.is_empty());
            assert!(validation_func.is_none());
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].name, "Ok");
            assert_eq!(variants[0].fields.len(), 1);
            assert_eq!(variants[0].fields[0].name, "value");
            assert_eq!(variants[0].fields[0].ty, "string");
            assert!(variants[0].fields[0].constraint.is_none());
            assert_eq!(variants[1].name, "Err");
            assert_eq!(variants[1].fields[0].name, "message");
        }
    }

    #[test]
    fn enum_type_lowers_to_fieldless_variants() {
        let s = one("type Status =\n    Active\n    Inactive\n");
        assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
        if let Stmt::Type { variants, .. } = s {
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].name, "Active");
            assert!(variants[0].fields.is_empty());
            assert_eq!(variants[1].name, "Inactive");
            assert!(variants[1].fields.is_empty());
        }
    }

    #[test]
    fn record_type_lowers_to_single_variant_named_after_type() {
        // A lowercase first field marks the record form; its lone variant takes
        // the type's own name, exactly as Default's `type Point = { x, y }` does.
        let s = one("type Point =\n    x : int\n    y : int\n");
        assert!(matches!(s, Stmt::Type { .. }), "expected type, got {s:?}");
        if let Stmt::Type { name, variants, .. } = s {
            assert_eq!(name, "Point");
            assert_eq!(variants.len(), 1);
            assert_eq!(variants[0].name, "Point");
            assert_eq!(variants[0].fields.len(), 2);
            assert_eq!(variants[0].fields[0].name, "x");
            assert_eq!(variants[0].fields[0].ty, "int");
        }
    }

    #[test]
    fn extern_lowers_to_canonical_extern_stmt() {
        // `extern name (p : T) (q : U) -> R` lowers to the SAME `Stmt::Extern`
        // the Default `extern fn name(p: T, q: U) -> R` emits — typed parameters
        // in order plus a return type ([FLAVOR-ML-EXTERN], [FLAVOR-IR-EQUIV]).
        let s = one("extern sqlite3_open (filename : string) (ppDb : Ptr) -> int\n");
        assert!(
            matches!(s, Stmt::Extern { .. }),
            "expected extern, got {s:?}"
        );
        if let Stmt::Extern {
            name,
            parameters,
            return_type,
            ..
        } = s
        {
            assert_eq!(name, "sqlite3_open");
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0].name, "filename");
            assert_eq!(parameters[0].ty.name, "string");
            assert_eq!(parameters[1].name, "ppDb");
            assert_eq!(parameters[1].ty.name, "Ptr");
            assert_eq!(return_type.map(|t| t.name), Some("int".to_owned()));
        }
    }

    #[test]
    fn reserved_handler_word_reports_a_clear_error() {
        // `handler`/`do` are not yet in the shared core, so the parser still
        // reports a precise "not yet supported" diagnostic for them.
        let parsed = parse_ml("handler Db\n    add : string => int\n");
        assert!(parsed
            .errors
            .iter()
            .any(|e| e.message.contains("not yet supported")));
    }

    #[test]
    fn effect_decl_lowers_to_effect_stmt() {
        // `effect Trace` + `mark : string => Unit` lowers to the SAME
        // `Stmt::Effect` the Default `effect Trace { mark: fn(string) -> Unit }`
        // emits — one operation rendered as `fn(string) -> Unit`, with empty
        // parameters and a blank return type ([FLAVOR-ML-EFFECT], [FLAVOR-IR-EQUIV]).
        let s = one("effect Trace\n    mark : string => Unit\n");
        assert!(
            matches!(s, Stmt::Effect { .. }),
            "expected effect, got {s:?}"
        );
        if let Stmt::Effect {
            name, operations, ..
        } = s
        {
            assert_eq!(name, "Trace");
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0].name, "mark");
            assert_eq!(operations[0].ty, "fn(string) -> Unit");
            assert!(operations[0].parameters.is_empty());
            assert_eq!(operations[0].return_type, "");
        }
    }

    #[test]
    fn multi_arg_effect_op_renders_flat_payload() {
        // A multi-argument op `exec : (Ptr, string) => int` must lower to the
        // FLAT `fn(Ptr, string) -> int` the Default flavor emits — NOT the
        // parenthesised `fn((Ptr, string)) -> int` — so inference recovers each
        // argument type and the IR is byte-identical ([FLAVOR-ML-EFFECT],
        // [FLAVOR-IR-EQUIV]).
        let s = one("effect Database\n    exec : (Ptr, string) => int\n");
        if let Stmt::Effect { operations, .. } = s {
            assert_eq!(operations[0].ty, "fn(Ptr, string) -> int");
        } else {
            panic!("expected effect, got {s:?}");
        }
    }

    #[test]
    fn signature_effect_row_threads_into_function() {
        // `traced : Unit -> int ! Trace` puts `Trace` in the function's effect row,
        // byte-identical to the Default `fn traced() -> int !Trace`.
        let s = one("traced : Unit -> int ! Trace\ntraced () =\n    perform Trace.mark \"one\"\n");
        assert!(
            matches!(s, Stmt::Function { .. }),
            "expected function, got {s:?}"
        );
        if let Stmt::Function { effects, .. } = s {
            let names: Vec<&str> = effects.iter().map(|e| e.name.as_str()).collect();
            assert_eq!(names, vec!["Trace"]);
            assert!(effects[0].type_args.is_empty());
        }
    }

    #[test]
    fn generic_signature_and_effect_row_args_thread_into_function() {
        // `tick<T> : Unit -> int ! State<int>` — the signature's type-param
        // binder and the row's type arguments both land on the canonical
        // `Stmt::Function`, byte-identical to the Default
        // `fn tick<T>() -> int !State<int>`. Implements [FLAVOR-ML-GENERICS],
        // [EFFECTS-GENERIC-ROWS].
        let s = one("tick<T> : Unit -> int ! State<int>\ntick () =\n    perform State.get ()\n");
        if let Stmt::Function {
            type_params,
            effects,
            ..
        } = s
        {
            assert_eq!(type_params.len(), 1);
            assert_eq!(type_params[0].name, "T");
            assert_eq!(effects.len(), 1);
            assert_eq!(effects[0].name, "State");
            assert_eq!(effects[0].type_args.len(), 1);
            assert_eq!(effects[0].type_args[0].name, "int");
        } else {
            panic!("expected function, got {s:?}");
        }
    }

    #[test]
    fn variance_markers_lower_onto_type_and_effect_params() {
        // `type Source out T =` / `type Sink in T =` — variance markers lower
        // to the canonical `TypeParam` variance the Default `type Source<out T>`
        // carries. Implements [TYPE-VARIANCE-DECL].
        let s = one("type Source out T =\n    produce : T\n");
        if let Stmt::Type { type_params, .. } = s {
            assert_eq!(type_params.len(), 1);
            assert_eq!(type_params[0].name, "T");
            assert_eq!(type_params[0].variance, Variance::Covariant);
        } else {
            panic!("expected type, got {s:?}");
        }
        let s = one("type Sink in T =\n    accept : T -> Unit\n");
        if let Stmt::Type { type_params, .. } = s {
            assert_eq!(type_params[0].variance, Variance::Contravariant);
        } else {
            panic!("expected type, got {s:?}");
        }
        // `effect State T` — a generic effect declaration.
        // Implements [EFFECTS-GENERIC-DECL].
        let s = one("effect State T\n    get : Unit => T\n    set : T => Unit\n");
        if let Stmt::Effect {
            type_params,
            operations,
            ..
        } = s
        {
            assert_eq!(type_params.len(), 1);
            assert_eq!(type_params[0].name, "T");
            assert_eq!(operations.len(), 2);
            assert_eq!(operations[0].ty, "fn() -> T");
            assert_eq!(operations[1].ty, "fn(T) -> Unit");
        } else {
            panic!("expected effect, got {s:?}");
        }
    }

    #[test]
    fn perform_lowers_to_perform_expr() {
        // `perform Trace.mark "one"` lowers to the SAME `Expr::Perform` the Default
        // `perform Trace.mark("one")` emits ([FLAVOR-ML-EFFECT]).
        let s = one("r = perform Trace.mark \"one\"\n");
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Perform { .. },
                    ..
                }
            ),
            "expected perform, got {s:?}"
        );
        if let Stmt::Let {
            value:
                Expr::Perform {
                    effect,
                    operation,
                    arguments,
                    named_arguments,
                    ..
                },
            ..
        } = s
        {
            assert_eq!(effect, "Trace");
            assert_eq!(operation, "mark");
            assert_eq!(arguments, vec![Expr::Str("one".to_owned())]);
            assert!(named_arguments.is_empty());
        }
    }

    #[test]
    fn handle_lowers_to_handler_expr() {
        // `handle Trace` + a `mark label => …` arm + `in traced ()` lowers to the
        // SAME `Expr::Handler` the Default `handle Trace mark label => … in traced()`
        // emits ([FLAVOR-ML-EFFECT]).
        let src =
            "r =\n    handle Trace\n        mark label =>\n            resume\n    in traced ()\n";
        let s = one(src);
        assert!(
            matches!(
                s,
                Stmt::Let {
                    value: Expr::Handler { .. },
                    ..
                }
            ),
            "expected handler, got {s:?}"
        );
        if let Stmt::Let {
            value: Expr::Handler {
                effect, arms, body, ..
            },
            ..
        } = s
        {
            assert_eq!(effect, "Trace");
            assert_eq!(arms.len(), 1);
            assert_eq!(arms[0].operation, "mark");
            assert_eq!(arms[0].params, vec!["label".to_owned()]);
            assert!(
                matches!(*body, Expr::Call { .. }),
                "handle body should be the call, got {body:?}"
            );
        }
    }

    #[test]
    fn resume_lowers_with_and_without_argument() {
        // `resume` (no arg) → `Resume(None)`; `resume seed` → `Resume(Some(seed))`,
        // byte-identical to the Default `resume()` / `resume(seed)` ([FLAVOR-ML-EFFECT]).
        let bare = one("r = resume\n");
        assert!(
            matches!(
                bare,
                Stmt::Let {
                    value: Expr::Resume(None),
                    ..
                }
            ),
            "expected bare resume, got {bare:?}"
        );
        let valued = one("r = resume seed\n");
        assert!(
            matches!(
                valued,
                Stmt::Let {
                    value: Expr::Resume(Some(_)),
                    ..
                }
            ),
            "expected valued resume, got {valued:?}"
        );
        if let Stmt::Let {
            value: Expr::Resume(Some(inner)),
            ..
        } = valued
        {
            assert_eq!(*inner, Expr::Identifier("seed".to_owned()));
        }
    }
}
