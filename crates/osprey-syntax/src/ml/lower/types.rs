//! ML types lowering.
use super::{
    DocScope, EffectOperation, EffectRef, ExternParameter, MlEffectOp, MlEffectRef, MlExternParam,
    MlType, MlTypeField, MlTypeParam, MlVariance, MlVariant, TypeExpr, TypeField, TypeParam,
    TypeVariant, Variance, UNIT_PAYLOAD,
};

/// Lower one `extern` parameter to a canonical [`ExternParameter`], threading its
/// declared type through the shared [`type_expr`] path so it is byte-identical to
/// the Default flavor's extern parameter ([FLAVOR-ML-EXTERN]). A type with no
/// canonical [`TypeExpr`] form (a tuple) falls back to its rendered surface name.
pub(in crate::ml) fn lower_extern_param(param: MlExternParam) -> ExternParameter {
    let ty = type_expr(&param.ty).unwrap_or_else(|| TypeExpr::named(render_type(&param.ty)));
    ExternParameter {
        name: param.name,
        ty,
    }
}

/// Lower one CST variant to a canonical [`TypeVariant`], rendering each field's
/// type to the same surface string the Default flavor stores ([FLAVOR-ML-TYPE]).
pub(in crate::ml) fn lower_variant(variant: MlVariant) -> TypeVariant {
    TypeVariant {
        name: variant.name,
        fields: variant.fields.into_iter().map(lower_type_field).collect(),
    }
}

/// Lower one `field : type` line, with `constraint: None` (ML has no `where`
/// clause on type fields yet) — byte-identical to the Default field shape.
pub(in crate::ml) fn lower_type_field(field: MlTypeField) -> TypeField {
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
pub(in crate::ml) fn render_type(ty: &MlType) -> String {
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
pub(in crate::ml) fn render_arrow_arg(ty: &MlType) -> String {
    match ty {
        MlType::Tuple(parts) => render_tuple(parts),
        other => format!("({})", render_type(other)),
    }
}

/// Render a tuple type as `(a, b, …)`.
pub(in crate::ml) fn render_tuple(parts: &[MlType]) -> String {
    let rendered = parts.iter().map(render_type).collect::<Vec<_>>().join(", ");
    format!("({rendered})")
}

/// Lower one CST type parameter to the canonical variance-carrying
/// [`TypeParam`] — byte-identical to the Default flavor's lowering.
/// Implements [TYPE-VARIANCE-DECL].
pub(in crate::ml) fn lower_type_param(p: MlTypeParam) -> TypeParam {
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
pub(in crate::ml) fn lower_effect_ref(r: MlEffectRef) -> EffectRef {
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

/// Lower one `op : P => R` effect operation line to the canonical
/// [`EffectOperation`], rendering the payload/result into the `fn(P) -> R`
/// surface string the Default flavor emits ([FLAVOR-ML-EFFECT]). `parameters`
/// and `return_type` stay empty/blank, matching the Default-flavor shape.
pub(in crate::ml) fn lower_effect_op(op: MlEffectOp) -> EffectOperation {
    EffectOperation {
        ty: format!(
            "fn({}) -> {}",
            render_op_payload(&op.payload),
            render_type(&op.result)
        ),
        name: op.name,
        mode: op.mode,
        declared_multiplicity: op.multiplicity,
        replayable: op.replayable,
        parameters: Vec::new(),
        return_type: String::new(),
        doc: op
            .doc
            .map(|text| crate::docparse::parse_doc(&text, DocScope::Outer)),
        position: Some(op.pos),
    }
}

/// Render a multi-argument effect-operation payload `(P1, P2, …)` as the bare
/// comma-separated `P1, P2, …` the Default flavor's `fn(P1, P2, …) -> R` op
/// signature carries — NOT the parenthesised tuple form — so the canonical
/// operation `ty` (and the per-argument types inference recovers from it) is
/// byte-identical across flavors ([FLAVOR-ML-EFFECT]). A bare `Unit` payload is
/// the zero-argument boundary and renders empty (`fn() -> R`); a single
/// (non-tuple, non-Unit) payload renders normally.
pub(in crate::ml) fn render_op_payload(ty: &MlType) -> String {
    match ty {
        MlType::Name(name) if name == UNIT_PAYLOAD => String::new(),
        MlType::Tuple(parts) => parts.iter().map(render_type).collect::<Vec<_>>().join(", "),
        other => render_type(other),
    }
}

/// Flatten the top-level arrow spine of a type: `a -> b -> c` ⇒ `[a, b, c]`,
/// `(a, b) -> c` ⇒ `[(a,b), c]`, a non-arrow ⇒ a single-element list.
pub(in crate::ml) fn arrow_spine(ty: &MlType) -> Vec<MlType> {
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
pub(in crate::ml) fn arrow_of(slice: &[MlType]) -> Option<TypeExpr> {
    match slice {
        [] => None,
        [single] => type_expr(single),
        [first, rest @ ..] => Some(TypeExpr {
            name: "fn".to_owned(),
            generic_params: Vec::new(),
            is_array: false,
            array_element: None,
            is_function: true,
            parameter_types: arrow_parameter_types(first)?,
            return_type: Some(Box::new(arrow_of(rest)?)),
            position: None,
        }),
    }
}

/// Convert an ML type to a canonical [`TypeExpr`]. A tuple type has no canonical
/// `TypeExpr` form, so it (and anything containing one) yields `None` — leaving
/// that position to inference rather than annotating it wrongly.
pub(in crate::ml) fn type_expr(ty: &MlType) -> Option<TypeExpr> {
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
            parameter_types: arrow_parameter_types(from)?,
            return_type: Some(Box::new(type_expr(to)?)),
            position: None,
        }),
        MlType::Tuple(_) => None,
    }
}

/// ML's bare `Unit` arrow domain is the zero-argument function boundary, just
/// as `()` is the zero-argument binding/call marker. A tuple domain is the FLAT
/// multi-parameter function type: `(int, int) -> int` is the Default flavor's
/// `fn(int, int) -> int` ([FLAVOR-ML-CURRY]). Dropping that arm rejected the
/// valid HOF signature `applyTwo : ((int, int) -> int) -> int -> int -> int` as
/// "a tuple type has no value form" — a signature SLOT holding a flat function
/// type is not a tuple value, and only a BINDING's outermost domain is graded
/// by [`signature_mismatch`]/[`expand_tuple_head`]. Other domains contribute
/// one canonical parameter.
pub(in crate::ml) fn arrow_parameter_types(domain: &MlType) -> Option<Vec<TypeExpr>> {
    match domain {
        MlType::Name(name) if name == UNIT_PAYLOAD => Some(Vec::new()),
        MlType::Tuple(parts) => parts.iter().map(type_expr).collect(),
        other => Some(vec![type_expr(other)?]),
    }
}
