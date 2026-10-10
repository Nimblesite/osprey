//! Effect row value shapes.
use super::{
    merge_optional_value, shift_summary_levels, shift_value_levels, BTreeSet, Callable,
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
            projection.push(next);
            Callable::Parameter {
                level,
                index,
                projection,
            }
        }
        Some(Callable::Unknown) => Callable::Unknown,
        Some(Callable::Known(_)) | None => return,
    };
    merge_optional_value(projected, Value::from_callable(callable));
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
