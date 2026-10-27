//! Effect row value shapes.
use super::{
    merge_optional_value, shift_summary_levels, shift_value_levels, BTreeMap, BTreeSet, Callable,
    KnownCallable, Projection, Requirement, Summary, Value,
};

pub(super) fn project_field(mut value: Value, field: &str) -> Option<Value> {
    let mut projected = value.fields.remove(field);
    project_callable(
        value.callable,
        Projection::Field(field.to_string()),
        &mut projected,
    );
    projected
}

pub(super) fn project_element(mut value: Value) -> Option<Value> {
    let mut projected = value.element.take().map(|value| *value);
    project_callable(value.callable, Projection::Element, &mut projected);
    projected
}

pub(super) fn project_success_value(mut value: Value) -> Option<Value> {
    let mut projected = value.result_payload.take().map(|value| *value);
    if let Some(field) = value.fields.remove("value") {
        merge_optional_value(&mut projected, field);
    }
    project_callable(value.callable, Projection::SuccessValue, &mut projected);
    projected
}

pub(super) fn project_fiber_value(mut value: Value) -> Option<Value> {
    let mut projected = value.fiber_payload.take().map(|value| *value);
    project_callable(value.callable, Projection::FiberValue, &mut projected);
    projected
}

/// Resolve [`Projection::Deep`]: the finite summary of a recursive aggregate.
/// Every callable stored anywhere below this value — fields, elements, result
/// and fiber payloads — merges into one flat value, so each stored closure's
/// effect provenance is counted no matter how deep the aggregate nests, while
/// the projection path itself stays finite. Implements [EFFECTS-PROVENANCE].
pub(super) fn project_deep(value: &Value) -> Value {
    let mut merged = Value::default();
    collect_deep(value, &mut merged);
    merged
}

fn collect_deep(value: &Value, merged: &mut Value) {
    let mut leaf = value.clone();
    leaf.fields = BTreeMap::new();
    leaf.element = None;
    leaf.result_payload = None;
    leaf.fiber_payload = None;
    merged.union(leaf);
    let nested = value
        .fields
        .values()
        .chain(value.element.as_deref())
        .chain(value.result_payload.as_deref())
        .chain(value.fiber_payload.as_deref());
    for nested in nested {
        collect_deep(nested, merged);
    }
}

/// Cut the one feedback a single fixed-point pass cannot complete on its own:
/// `fold`'s accumulator carries its callback's return, so a contribution that
/// still holds symbolic parameter USES would be consumed and re-grown every
/// pass and the recompute would oscillate between the unrolled levels forever.
/// Replace every merge-shaped callable — no named parameters, yet uses — with
/// `Unknown`: fail-closed and idempotent, so the recursion through values
/// reaches the finite fixed point [EFFECTS-PROVENANCE] requires. A real
/// closure always names its parameters, so only joined alternatives flatten.
pub(super) fn stabilize_merged_callables(value: &Value) -> Value {
    let mut stabilized = value.clone();
    if let Some(Callable::Known(known)) = &value.callable {
        if known.parameters.is_empty() && !known.summary.parameter_uses.is_empty() {
            stabilized.callable = Some(Callable::Unknown);
        }
    }
    stabilized.fields = value
        .fields
        .iter()
        .map(|(name, nested)| (name.clone(), stabilize_merged_callables(nested)))
        .collect();
    stabilized.element = stabilize_boxed(value.element.as_deref());
    stabilized.result_payload = stabilize_boxed(value.result_payload.as_deref());
    stabilized.fiber_payload = stabilize_boxed(value.fiber_payload.as_deref());
    stabilized
}

fn stabilize_boxed(nested: Option<&Value>) -> Option<Box<Value>> {
    nested.map(|nested| Box::new(stabilize_merged_callables(nested)))
}

pub(super) fn project_callable(
    callable: Option<Callable>,
    next: Projection,
    projected: &mut Option<Value>,
) {
    let callable = match callable {
        Some(Callable::Parameter {
            level,
            index,
            mut projection,
        }) => {
            projection = extend_projection(projection, next);
            Callable::Parameter {
                level,
                index,
                projection,
            }
        }
        // A value's own unknown callable-ness is not stored at its fields or
        // elements: those keep exactly the provenance they merge, and a call
        // site whose projection finds no callable fails closed on the static
        // function type. Widening stays closed through the slots themselves —
        // a terminal value holds `unknown_callable` in every one. An
        // invocation-shaped step must still propagate the unknown callable.
        // Implements [EFFECTS-PROVENANCE].
        Some(Callable::Unknown) if next.is_structural() => return,
        Some(Callable::Unknown) => Callable::Unknown,
        Some(Callable::Known(_)) | None => return,
    };
    merge_optional_value(projected, Value::from_callable(callable));
}

/// Extend a projection path by one step while keeping it finite: a structural
/// step after a `Deep` summary is subsumed by it, and a path that repeats one
/// structural cycle is cut back to the first cycle plus `Deep`, so a recursion
/// through an aggregate cannot grow the path one cycle per fixed-point
/// iteration. Implements [EFFECTS-PROVENANCE].
fn extend_projection(mut projection: Vec<Projection>, next: Projection) -> Vec<Projection> {
    if subsumed_by_deep(&projection, &next) {
        return projection;
    }
    projection.push(next);
    compress_projection(projection)
}

fn subsumed_by_deep(projection: &[Projection], next: &Projection) -> bool {
    projection.last() == Some(&Projection::Deep)
        && (next.is_structural() || matches!(next, Projection::Deep))
}

/// The end of the first structural cycle that occurs twice in a row, so that
/// every later step is structural too. `None` means no cycle: the path is
/// already finite and stays verbatim.
fn structural_cycle(projection: &[Projection]) -> Option<usize> {
    (0..projection.len())
        .filter(|&start| {
            projection
                .get(start..)
                .is_some_and(|steps| steps.iter().all(Projection::is_structural))
        })
        .flat_map(|start| (1..=(projection.len() - start) / 2).map(move |len| (start, len)))
        .find(|&(start, len)| {
            projection.get(start..start + len) == projection.get(start + len..start + 2 * len)
        })
        .map(|(start, len)| start + len)
}

/// Cut a repeated structural cycle back to its first occurrence plus one
/// [`Projection::Deep`] step: every callable the repeated part and the
/// structural tail could reach is stored below that first occurrence, which is
/// exactly what `Deep` collects. A call-shaped step in that range keeps the
/// path verbatim — summarising across an invocation is not sound.
fn compress_projection(projection: Vec<Projection>) -> Vec<Projection> {
    match structural_cycle(&projection) {
        Some(cut) => projection
            .into_iter()
            .take(cut)
            .chain([Projection::Deep])
            .collect(),
        None => projection,
    }
}

pub(super) fn method_thunk(mut summary: Summary, mut returned: Option<Value>) -> Value {
    shift_summary_levels(&mut summary, 1);
    if let Some(returned) = &mut returned {
        shift_value_levels(returned, 1, 0);
    }
    Value::from_callable(Callable::Known(Box::new(KnownCallable {
        parameters: Vec::new(),
        summary,
        returned: returned.map(Box::new),
    })))
}

/// Built-in signatures can prove a primitive result has no fields without
/// inventing callable provenance for an otherwise opaque result.
pub(super) fn builtin_return_shape(name: &str) -> Option<Value> {
    // Effect summaries revisit calls during fixed-point inference and warning
    // comparisons. Their immutable builtin signatures need one shared table.
    let crate::ty::Type::Fun { ret, .. } = &builtin_environment().get(name)?.ty else {
        return None;
    };
    value_shape(ret)
}

pub(super) fn builtin_environment() -> &'static crate::env::TypeEnv {
    static BUILTINS: std::sync::LazyLock<crate::env::TypeEnv> =
        std::sync::LazyLock::new(crate::builtins::base_env);
    &BUILTINS
}

pub(super) fn arithmetic_builtin_operations(
    name: &str,
    ty: Option<&crate::ty::Type>,
) -> &'static [&'static str] {
    match name {
        "abs" if !crate::arithmetic::absolute_overflow_possible(ty) => &[],
        "abs" => &["overflow"],
        "intDiv" => &["overflow", "remainderByZero"],
        _ => &[],
    }
}

pub(super) fn builtin_callable_value(name: &str, ty: Option<&crate::ty::Type>) -> Option<Value> {
    let env = builtin_environment();
    if !env.is_runtime_builtin(name)
        && !matches!(
            name,
            "abs"
                | "intDiv"
                | "toFloat"
                | "sqrt"
                | "exp"
                | "ln"
                | "tanh"
                | "sin"
                | "cos"
                | "tan"
                | "asin"
                | "acos"
                | "atan"
                | "sinh"
                | "cosh"
                | "asinh"
                | "acosh"
                | "atanh"
                | "log2"
                | "log10"
                | "log1p"
                | "expm1"
                | "erf"
                | "erfc"
                | "floor"
                | "ceil"
                | "trunc"
                | "round"
                | "pow"
                | "atan2"
                | "hypot"
                | "fmod"
                | "copysign"
                | "float32"
                | "floatBits"
                | "floatFromBits"
                | "float32Bits"
                | "float32FromBits"
                | "floatToInt"
                | "bitAnd"
                | "bitOr"
                | "bitXor"
                | "bitNot"
                | "shiftLeft"
                | "shiftRight"
                | "shiftRightLogical"
                | "toString"
                | "checkedAdd"
                | "checkedSub"
                | "checkedMul"
                | "wrapAdd"
                | "wrapSub"
                | "wrapMul"
                | "satAdd"
                | "satSub"
                | "satMul"
        )
    {
        return None;
    }
    let crate::ty::Type::Fun { params, ret } = &env.get(name)?.ty else {
        return None;
    };
    Some(Value::from_callable(Callable::Known(Box::new(
        KnownCallable {
            parameters: (0..params.len())
                .map(|index| format!("arg{index}"))
                .collect(),
            summary: Summary {
                runtime_builtins: env
                    .is_runtime_builtin(name)
                    .then(|| name.to_owned())
                    .into_iter()
                    .collect(),
                required: arithmetic_builtin_operations(name, ty)
                    .iter()
                    .map(|op| Requirement::new(osprey_ast::ARITH_EFFECT, op, Vec::new()))
                    .collect(),
                ..Summary::default()
            },
            returned: value_shape(ret).map(Box::new),
        },
    ))))
}

pub(super) fn value_shape(ty: &crate::ty::Type) -> Option<Value> {
    use crate::ty::{names, Type};
    match ty {
        Type::Con { name, args } if name == names::RESULT => Some(Value {
            result_payload: args.first().and_then(value_shape).map(Box::new),
            ..Value::default()
        }),
        Type::Con { name, .. }
            if matches!(
                name.as_str(),
                names::INT
                    | names::FLOAT
                    | names::STRING
                    | names::BOOL
                    | names::UNIT
                    | names::LIST
                    | names::MAP
                    | names::ITERATOR
                    | names::FIBER
                    | names::CHANNEL
                    | names::PTR
                    | names::GPU_BUFFER
            ) =>
        {
            Some(Value {
                field_names: Some(BTreeSet::new()),
                ..Value::default()
            })
        }
        _ => None,
    }
}
