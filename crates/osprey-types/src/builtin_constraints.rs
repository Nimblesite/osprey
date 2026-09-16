//! Runtime-representation constraints for built-ins whose surface signature
//! cannot be expressed as one Hindley-Milner function type. The environment
//! keeps `any` for call inference; named call sites are checked against these
//! predicates before code generation sees an incompatible handle.
//! Implements [BUILTIN-PRINT], [BUILTIN-TOSTRING],
//! [BUILTIN-COLLECTION-LENGTH], and [BUILTIN-COLLECTION-ISEMPTY].

use crate::ty::{names, Type};

/// [FLOAT-OPERANDS] Cannot collide with a source identifier or built-in name.
pub(crate) const NUMERIC_OPERAND_PREFIX: &str = "numeric ";

/// Arithmetic obligation keys retain their origin across scheme instantiation,
/// including obligations transported inside deferred method relations.
pub(crate) fn located_name(name: &str, position: Option<osprey_ast::Position>) -> String {
    position.map_or_else(
        || name.to_owned(),
        |p| format!("{name} @{}:{}", p.line, p.column),
    )
}

pub(crate) fn source_position(name: &str) -> Option<osprey_ast::Position> {
    let (_, location) = name.rsplit_once(" @")?;
    let (line, column) = location.split_once(':')?;
    Some(osprey_ast::Position {
        line: line.parse().ok()?,
        column: column.parse().ok()?,
    })
}

pub(crate) fn operation_name(name: &str) -> &str {
    name.split_once(" @")
        .map_or(name, |(operation, _)| operation)
}

pub(crate) fn is_numeric_scalar(ty: &Type) -> bool {
    ty.is_named(names::INT) || ty.is_named(names::FLOAT)
}

const SIZED_DISPLAY: &str = "string | List<T> | Map<string, V>";
const PRINTABLE_DISPLAY: &str =
    "int | float | bool | string | Unit | any | Result<printable, printable>";
const GPU_SOURCE_DISPLAY: &str = "List<int> | List<float> | List<bool> \
     | Iterator<int> | Iterator<float> | Iterator<bool>";
const GPU_BUFFER_DISPLAY: &str = "GpuBuffer<int> | GpuBuffer<float> | GpuBuffer<bool>";

/// Human-facing parameter type for a constrained `any` scheme.
pub(crate) fn display_param_type(name: &str, index: usize) -> Option<&'static str> {
    if index != 0 {
        return None;
    }
    match name {
        "length" | "isEmpty" => Some(SIZED_DISPLAY),
        "print" | "toString" => Some(PRINTABLE_DISPLAY),
        "toGpu" => Some(GPU_SOURCE_DISPLAY),
        name if is_gpu_buffer_builtin(name) => Some(GPU_BUFFER_DISPLAY),
        _ => None,
    }
}

/// Validate the receiver/value of a representation-sensitive built-in.
pub(crate) fn invalid_use(name: &str, ty: &Type) -> Option<String> {
    // [FLOAT-OPERANDS] Schemes carry this obligation into each instantiation,
    // preserving numeric polymorphism while refusing concrete nonnumeric uses.
    if let Some(op) = operation_name(name).strip_prefix(NUMERIC_OPERAND_PREFIX) {
        return (!is_numeric_scalar(ty) && !matches!(ty, Type::Var(_)))
            .then(|| format!("operator `{op}` requires int or float; got {ty}"));
    }
    match name {
        "interpolation" if matches!(ty, Type::Fun { .. }) && crate::ty::has_type_var(ty) => Some(
            "a closure value with a still-generic type cannot be interpolated; apply it or give it a concrete function type".to_owned()
        ),
        "interpolation" if !is_interpolatable(ty) && !matches!(ty, Type::Fun { .. }) => {
            Some(format!("cannot convert value for interpolation: {ty}"))
        }
        "length" | "isEmpty" if !is_sized(ty) => Some(format!(
            "`{name}` supports only string, List<T>, or Map<string, V>; got {ty}"
        )),
        "print" if !is_printable(ty) => Some(format!("cannot convert value for printing: {ty}")),
        "toString" if !is_printable(ty) => Some(format!("cannot convert value to string: {ty}")),
        // GPU buffer elements are scalars [GPU-BUFFER-ELEM]
        // (docs/specs/0034-GPUComputation.md).
        "toGpu" if !is_gpu_source(ty) => Some(format!(
            "`toGpu` supports only {GPU_SOURCE_DISPLAY}; got {ty}"
        )),
        name if is_gpu_buffer_builtin(name) && !is_gpu_buffer(ty) => Some(format!(
            "`{name}` supports only {GPU_BUFFER_DISPLAY}; got {ty}"
        )),
        _ => None,
    }
}

/// The GPU built-ins whose first argument is a buffer [GPU-BUFFER-ELEM].
pub(crate) fn is_gpu_buffer_builtin(name: &str) -> bool {
    matches!(
        name,
        "fromGpu"
            | "gpuMap"
            | "gpuFold"
            | "gpuLength"
            | "gpuZipWith"
            | "gpuGet"
            | "gpuScan"
            | "gpuFilter"
    )
}

/// A scalar a `GpuBuffer` may hold [GPU-BUFFER-ELEM]. Unresolved variables are
/// deferred exactly as `is_sized` defers them.
fn is_gpu_scalar(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } => {
            args.is_empty() && matches!(name.as_str(), names::INT | names::FLOAT | names::BOOL)
        }
        _ => false,
    }
}

/// A `container<scalar>` (or still-unresolved) receiver for a GPU builtin.
fn is_gpu_container(ty: &Type, container: &str) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } if name == container => {
            matches!(args.as_slice(), [elem] if is_gpu_scalar(elem))
        }
        _ => false,
    }
}

/// A `toGpu` source: a scalar `List`, or a scalar `Iterator` pipeline that
/// fuses straight into the buffer with no list in between [GPU-BUFFER-FUSE].
fn is_gpu_source(ty: &Type) -> bool {
    is_gpu_container(ty, names::LIST) || is_gpu_container(ty, names::ITERATOR)
}

fn is_gpu_buffer(ty: &Type) -> bool {
    is_gpu_container(ty, names::GPU_BUFFER)
}

/// Unresolved variables are deferred by the checker and remain accepted when
/// still polymorphic. Osprey currently has no type-class constraint to carry
/// this obligation through generalization.
fn is_sized(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } if name == names::STRING => args.is_empty(),
        Type::Con { name, args } if name == names::LIST => args.len() == 1,
        Type::Con { name, args } if name == names::MAP => {
            matches!(args.as_slice(), [key, _] if key.is_named(names::STRING))
        }
        _ => false,
    }
}

/// Values supported by `runtime::to_string_value`. `any` is retained as the
/// explicit erased-compatibility escape hatch; concrete aggregate and runtime
/// handles are rejected. Result errors use the runtime's stored string message,
/// while the success payload is formatted recursively.
fn is_printable(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args }
            if args.is_empty()
                && matches!(
                    name.as_str(),
                    names::INT
                        | names::FLOAT
                        | names::STRING
                        | names::BOOL
                        | names::UNIT
                        | names::ANY
                ) =>
        {
            true
        }
        Type::Con { name, args } if name == names::RESULT => {
            matches!(args.as_slice(), [ok, err] if is_printable(ok) && is_printable_error(err))
        }
        _ => false,
    }
}

/// What interpolation can render. That is everything `print` renders, plus a
/// fiber handle, which interpolates as its id — `tests/regressions/fiber`
/// prints `ids 1 2 3 4` from four of them. `print` has never taken a fiber on
/// its own and still does not.
fn is_interpolatable(ty: &Type) -> bool {
    is_printable(ty) || ty.is_named(names::FIBER)
}

fn is_printable_error(ty: &Type) -> bool {
    is_printable(ty)
        || matches!(
            ty,
            Type::Con { name, args }
                if args.is_empty() && matches!(name.as_str(), names::ERROR | names::MATH_ERROR)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A fiber renders as its id — `print("ids ${f1} ${f2}")` in
    /// `tests/regressions/fiber/fiber_showcase.test.osp` prints `ids 1 2`. The
    /// aggregate guard must not take that away. `print` has never accepted a
    /// fiber handle on its own, and this pins that difference as deliberate.
    #[test]
    fn interpolation_renders_a_fiber_where_print_does_not() {
        let fiber = Type::con(names::FIBER, vec![Type::int()]);
        assert_eq!(invalid_use("interpolation", &fiber), None);
        assert!(invalid_use("print", &fiber).is_some());
        assert!(invalid_use("interpolation", &Type::list(Type::int())).is_some());
        assert!(invalid_use("interpolation", &Type::string()).is_none());
    }

    #[test]
    fn size_constraint_accepts_only_runtime_size_receivers() {
        // [BUILTIN-COLLECTION-LENGTH] [BUILTIN-COLLECTION-ISEMPTY]
        assert!(is_sized(&Type::string()));
        assert!(is_sized(&Type::list(Type::int())));
        assert!(is_sized(&Type::map(Type::string(), Type::bool())));
        assert!(!is_sized(&Type::int()));
        assert!(!is_sized(&Type::Record {
            name: "R".into(),
            fields: BTreeMap::new(),
        }));
    }

    #[test]
    fn gpu_constraints_accept_scalar_containers_and_reject_the_rest() {
        // [GPU-BUFFER-ELEM] (docs/specs/0034-GPUComputation.md)
        assert!(invalid_use("toGpu", &Type::list(Type::int())).is_none());
        assert!(invalid_use("toGpu", &Type::list(Type::string())).is_some());
        assert!(invalid_use("toGpu", &Type::int()).is_some());
        assert!(invalid_use("gpuMap", &Type::gpu_buffer(Type::float())).is_none());
        assert!(invalid_use("gpuLength", &Type::gpu_buffer(Type::string())).is_some());
        // Still-unresolved receivers are deferred, exactly as `is_sized` defers.
        assert!(invalid_use("fromGpu", &Type::Var(9)).is_none());
        assert!(invalid_use("gpuFold", &Type::gpu_buffer(Type::Var(9))).is_none());
        assert_eq!(display_param_type("toGpu", 0), Some(GPU_SOURCE_DISPLAY));
        assert_eq!(display_param_type("gpuFold", 0), Some(GPU_BUFFER_DISPLAY));
        assert_eq!(display_param_type("gpuFold", 1), None);
    }

    #[test]
    fn printable_constraint_checks_result_payloads_recursively() {
        // [BUILTIN-PRINT] [BUILTIN-TOSTRING]
        assert!(is_printable(&Type::result(
            Type::int(),
            Type::prim(names::ERROR),
        )));
        assert!(!is_printable(&Type::result(
            Type::list(Type::int()),
            Type::prim(names::ERROR),
        )));
        assert!(!is_printable(&Type::fun(vec![Type::int()], Type::int())));
    }
}
