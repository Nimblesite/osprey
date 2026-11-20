//! Runtime-representation constraints for built-ins whose surface signature
//! cannot be expressed as one Hindley-Milner function type. The environment
//! keeps `any` for call inference; named call sites are checked against these
//! predicates before code generation sees an incompatible handle.
//! Implements [BUILTIN-PRINT], [BUILTIN-TOSTRING],
//! [BUILTIN-COLLECTION-LENGTH], and [BUILTIN-COLLECTION-ISEMPTY].

use crate::ty::{names, Type};

impl crate::check::Checker {
    pub(crate) fn apply_bound_fn(
        &mut self,
        name: Option<&str>,
        function: &Type,
        arguments: Vec<Type>,
        env: &crate::env::TypeEnv,
    ) -> Type {
        self.apply_named_fn(
            name.filter(|name| env.is_builtin(name)),
            function,
            arguments,
        )
    }
}

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
    crate::numeric::Numeric::of_type(ty).is_some()
}

/// A known outer constructor cannot become numeric by resolving its arguments.
/// Keep its obligation at the definition even when a Result error stays open.
pub(crate) fn fixed_numeric_operand(name: &str, ty: &Type) -> bool {
    name.starts_with(NUMERIC_OPERAND_PREFIX) && !matches!(ty, Type::Var(_))
}

/// [TYPE-NUMERIC-SIZED] The integer-only form of [`NUMERIC_OPERAND_PREFIX`],
/// for a builtin open over the integer types.
pub(crate) const INTEGER_OPERAND_PREFIX: &str = "integer ";

/// [BUILTIN-MATH] The float-only form of [`NUMERIC_OPERAND_PREFIX`].
pub(crate) const FLOAT_OPERAND_PREFIX: &str = "float ";
pub(crate) const MATRIX_OPERAND_PREFIX: &str = "matrix ";

const SIZED_DISPLAY: &str = "string | List<T> | Map<string, V>";
const SORTABLE_DISPLAY: &str = "List<int> | List<string>";
fn scalar_names() -> impl Iterator<Item = &'static str> {
    crate::numeric::Numeric::ALL
        .into_iter()
        .map(crate::numeric::Numeric::name)
        .chain(std::iter::once(names::BOOL))
}

fn container_display(containers: &[&str]) -> String {
    containers
        .iter()
        .flat_map(|container| scalar_names().map(move |element| format!("{container}<{element}>")))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Human-facing parameter type for a constrained `any` scheme.
pub(crate) fn display_param_type(name: &str, index: usize) -> Option<String> {
    if index != 0 {
        return None;
    }
    match name {
        "length" | "isEmpty" => Some(SIZED_DISPLAY.to_owned()),
        "listSort" => Some(SORTABLE_DISPLAY.to_owned()),
        "print" | "toString" => Some(
            scalar_names()
                .chain([
                    names::STRING,
                    names::UNIT,
                    names::ANY,
                    "List<printable>",
                    "Result<printable, printable>",
                    "record",
                    "union",
                ])
                .collect::<Vec<_>>()
                .join(" | "),
        ),
        "toGpu" => Some(container_display(&["List", "Iterator"])),
        "gpuMatMul" => Some("GpuBuffer<float> | GpuBuffer<float32>".to_owned()),
        name if is_gpu_buffer_builtin(name) => Some(container_display(&["GpuBuffer"])),
        _ => None,
    }
}

/// Validate the receiver/value of a representation-sensitive built-in.
pub(crate) fn invalid_use(name: &str, ty: &Type, named: Lookup<'_>) -> Option<String> {
    // [FLOAT-OPERANDS] Schemes carry this obligation into each instantiation,
    // preserving numeric polymorphism while refusing concrete nonnumeric uses.
    if let Some(op) = name.strip_prefix(NUMERIC_OPERAND_PREFIX) {
        let sized_float =
            crate::numeric::Numeric::of_type(ty).is_some_and(|n| n.is_sized() && n.is_float());
        let arithmetic = matches!(op, "+" | "-" | "*" | "/" | "%");
        let valid = is_numeric_scalar(ty) && !(arithmetic && sized_float);
        return (!valid && !matches!(ty, Type::Var(_)))
            .then(|| format!("operator `{op}` requires int or float; got {ty}"));
    }
    if let Some(op) = name.strip_prefix(FLOAT_OPERAND_PREFIX) {
        let float =
            crate::numeric::Numeric::of_type(ty).is_some_and(crate::numeric::Numeric::is_float);
        return (!float && !matches!(ty, Type::Var(_)))
            .then(|| format!("`{op}` requires a float; got {ty}"));
    }
    if let Some(op) = name.strip_prefix(INTEGER_OPERAND_PREFIX) {
        let integer =
            crate::numeric::Numeric::of_type(ty).is_some_and(crate::numeric::Numeric::is_integer);
        return (!integer && !matches!(ty, Type::Var(_)))
            .then(|| format!("`{op}` requires an integer; got {ty}"));
    }
    let name = name.strip_prefix(MATRIX_OPERAND_PREFIX).unwrap_or(name);
    match name {
        "interpolation" if matches!(ty, Type::Fun { .. }) && crate::ty::has_type_var(ty) => Some(
            "a closure value with a still-generic type cannot be interpolated; apply it or give it a concrete function type".to_owned()
        ),
        "interpolation" if !is_interpolatable(ty, named) => {
            Some(format!("cannot convert value for interpolation: {ty}"))
        }
        "length" | "isEmpty" if !is_sized(ty) => Some(format!(
            "`{name}` supports only string, List<T>, or Map<string, V>; got {ty}"
        )),
        "listSort" if !is_sortable_list(ty) => Some(format!(
            "`listSort` supports only List<int> or List<string>; got {ty}"
        )),
        "print" if !is_printable(ty, named, &mut Vec::new()) => Some(format!("cannot convert value for printing: {ty}")),
        "toString" if !is_printable(ty, named, &mut Vec::new()) => Some(format!("cannot convert value to string: {ty}")),
        // GPU buffer elements are scalars [GPU-BUFFER-ELEM]
        // (docs/specs/runtime/0034-GPUComputation.md).
        "toGpu" if !is_gpu_source(ty) => Some(format!(
            "`toGpu` supports only {}; got {ty}", container_display(&["List", "Iterator"])
        )),
        name if is_gpu_buffer_builtin(name) && !is_gpu_buffer(ty) => Some(format!(
            "`{name}` supports only {}; got {ty}", container_display(&["GpuBuffer"])
        )),
        "gpuMatMul" if !is_float_matrix(ty) => Some(format!(
            "`gpuMatMul` supports only GpuBuffer<float> | GpuBuffer<float32>; got {ty}"
        )),
        _ => None,
    }
}

// [GPU-MATMUL]: retain a generic obligation until its actual element is known.
fn is_float_matrix(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } if name == names::GPU_BUFFER => {
            matches!(args.as_slice(), [elem] if matches!(elem, Type::Var(_)) ||
            crate::numeric::Numeric::of_type(elem).is_some_and(|n| n == crate::numeric::Numeric::FLOAT || n == crate::numeric::Numeric::FLOAT32))
        }
        _ => false,
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
#[must_use]
pub fn is_gpu_scalar(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } => {
            args.is_empty()
                && (name == names::BOOL || crate::numeric::Numeric::of_name(name).is_some())
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

/// A `List<T>` whose elements the list runtime can order without a callback
/// [BUILTIN-LIST-SORT]. Unresolved variables are deferred exactly as
/// `is_sized` defers them.
fn is_sortable_list(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } if name == names::LIST => {
            matches!(args.as_slice(), [elem] if is_sortable_elem(elem))
        }
        _ => false,
    }
}

/// An element with a natural ascending order: `int` compares by value and
/// `string` by byte content [BUILTIN-LIST-SORT].
fn is_sortable_elem(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { name, args } => {
            args.is_empty()
                && (name == names::STRING
                    || crate::numeric::Numeric::of_name(name)
                        .is_some_and(crate::numeric::Numeric::is_integer))
        }
        _ => false,
    }
}

/// Values supported by `runtime::to_string_value`. `any` is retained as the
/// explicit erased-compatibility escape hatch; concrete aggregate and runtime
/// handles are rejected. Both sides of a Result are formatted recursively.
/// [BUILTIN-TOSTRING]: every value has a rendering except a function and the
/// runtime handles in [`UNPRINTABLE`], alone or inside a record, union, list
/// or Result.
fn is_printable(ty: &Type, named: Lookup<'_>, seen: &mut Vec<String>) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Fun { .. } => false,
        Type::Record { fields, .. } => fields.values().all(|t| is_printable(t, named, seen)),
        Type::Union { variants, .. } => variants.iter().all(|t| is_printable(t, named, seen)),
        Type::Con { name, args } => {
            !UNPRINTABLE.contains(&name.as_str())
                && args.iter().all(|t| is_printable(t, named, seen))
                && is_printable_named(name, args, named, seen)
        }
    }
}

/// A declared type is printable when every field of every variant is; a
/// type met again inside itself adds nothing new [BUILTIN-TOSTRING].
fn is_printable_named(
    name: &str,
    args: &[Type],
    named: Lookup<'_>,
    seen: &mut Vec<String>,
) -> bool {
    if seen.iter().any(|s| s == name) {
        return true;
    }
    match named(name, args) {
        Named::Sealed => false,
        Named::Plain => true,
        Named::Fields(fields) => {
            seen.push(name.to_owned());
            fields.iter().all(|t| is_printable(t, named, seen))
        }
    }
}

/// What a type name is at the use site.
pub(crate) enum Named {
    /// An alias sealed there: no structure, so nothing can render it
    /// [MODULES-OPAQUE-TYPES].
    Sealed,
    /// A declared record or union: the field types of every variant.
    Fields(Vec<Type>),
    /// A built-in or otherwise structureless name.
    Plain,
}

/// Resolve a type name, with its arguments, at the use site.
pub(crate) type Lookup<'a> = &'a dyn Fn(&str, &[Type]) -> Named;

const UNPRINTABLE: [&str; 7] = [
    names::MAP,
    names::ITERATOR,
    names::FIBER,
    names::CHANNEL,
    names::PTR,
    names::GPU_BUFFER,
    names::CAPABILITY,
];

/// What interpolation can render. That is everything `print` renders, plus a
/// fiber handle, which interpolates as its id — `src/osprey/tests/regressions/fiber`
/// prints `ids 1 2 3 4` from four of them. `print` has never taken a fiber on
/// its own and still does not.
fn is_interpolatable(ty: &Type, named: Lookup<'_>) -> bool {
    is_printable(ty, named, &mut Vec::new()) || ty.is_named(names::FIBER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn invalid_use(name: &str, ty: &Type) -> Option<String> {
        super::invalid_use(name, ty, &|_, _| Named::Plain)
    }

    fn is_printable(ty: &Type) -> bool {
        super::is_printable(ty, &|_, _| Named::Plain, &mut Vec::new())
    }

    #[test]
    fn numeric_signatures_name_every_accepted_width() {
        // [TYPE-NUMERIC-SIZED] Hover and generated docs must not omit accepted types.
        for numeric in crate::numeric::Numeric::ALL {
            for name in ["print", "toString", "toGpu", "fromGpu"] {
                let signature = crate::builtin_signature(name).expect("builtin signature");
                assert!(
                    signature
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .any(|word| word == numeric.name()),
                    "{name}: {signature}"
                );
                let docs = crate::builtin_doc_view(name).expect("builtin documentation");
                let parameter = docs.params.first().expect("first parameter");
                assert!(
                    parameter
                        .ty
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .any(|word| word == numeric.name()),
                    "{name}: {}",
                    parameter.ty
                );
            }
        }
    }

    /// A fiber renders as its id — `print("ids ${f1} ${f2}")` in
    /// `src/osprey/tests/regressions/fiber/fiber_showcase.test.osp` prints `ids 1 2`. The
    /// aggregate guard must not take that away. `print` has never accepted a
    /// fiber handle on its own, and this pins that difference as deliberate.
    #[test]
    fn interpolation_renders_a_fiber_where_print_does_not() {
        let fiber = Type::con(names::FIBER, vec![Type::int()]);
        assert_eq!(invalid_use("interpolation", &fiber), None);
        assert!(invalid_use("print", &fiber).is_some());
        let map = Type::con(names::MAP, vec![Type::string(), Type::int()]);
        assert!(invalid_use("interpolation", &Type::list(map)).is_some());
        assert!(invalid_use("interpolation", &Type::list(Type::int())).is_none());
        assert!(invalid_use("interpolation", &Type::string()).is_none());
    }

    #[test]
    fn sort_constraint_accepts_only_int_and_string_lists() {
        // [BUILTIN-LIST-SORT] (docs/specs/runtime/0012-Built-InFunctions.md)
        assert_eq!(invalid_use("listSort", &Type::list(Type::int())), None);
        assert_eq!(invalid_use("listSort", &Type::list(Type::string())), None);
        assert_eq!(invalid_use("listSort", &Type::Var(9)), None);
        assert!(invalid_use("listSort", &Type::list(Type::float())).is_some());
        assert!(invalid_use("listSort", &Type::list(Type::bool())).is_some());
        assert!(invalid_use("listSort", &Type::list(Type::list(Type::int()))).is_some());
        assert!(invalid_use("listSort", &Type::int()).is_some());
        assert_eq!(
            display_param_type("listSort", 0),
            Some(SORTABLE_DISPLAY.to_owned())
        );
        assert_eq!(display_param_type("listSort", 1), None);
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
        // [GPU-BUFFER-ELEM] (docs/specs/runtime/0034-GPUComputation.md)
        assert!(invalid_use("toGpu", &Type::list(Type::int())).is_none());
        assert!(invalid_use("toGpu", &Type::list(Type::string())).is_some());
        assert!(invalid_use("toGpu", &Type::int()).is_some());
        assert!(invalid_use("gpuMap", &Type::gpu_buffer(Type::float())).is_none());
        assert!(invalid_use("gpuLength", &Type::gpu_buffer(Type::string())).is_some());
        // Still-unresolved receivers are deferred, exactly as `is_sized` defers.
        assert!(invalid_use("fromGpu", &Type::Var(9)).is_none());
        assert!(invalid_use("gpuFold", &Type::gpu_buffer(Type::Var(9))).is_none());
        let row = Type::Record {
            name: String::new(),
            fields: BTreeMap::from([("x".to_owned(), Type::int())]),
        };
        assert!(invalid_use("gpuLength", &Type::gpu_buffer(row.clone())).is_some());
        assert!(invalid_use("gpuLength", &Type::gpu_buffer(Type::list(Type::int()))).is_some());
        let matmul = format!("{MATRIX_OPERAND_PREFIX}gpuMatMul");
        assert!(invalid_use(&matmul, &Type::Var(3)).is_none());
        assert!(invalid_use(&matmul, &row).is_some());
        assert!(invalid_use(&matmul, &Type::gpu_buffer(Type::float())).is_none());
        assert!(invalid_use(&matmul, &Type::gpu_buffer(Type::int())).is_some());
        assert_eq!(
            display_param_type("toGpu", 0),
            Some(container_display(&["List", "Iterator"]))
        );
        assert_eq!(
            display_param_type("gpuFold", 0),
            Some(container_display(&["GpuBuffer"]))
        );
        assert_eq!(display_param_type("gpuFold", 1), None);
    }

    #[test]
    fn printable_constraint_checks_result_payloads_recursively() {
        // [BUILTIN-PRINT] [BUILTIN-TOSTRING]
        assert!(is_printable(&Type::result(Type::int(), Type::string(),)));
        assert!(is_printable(&Type::result(
            Type::list(Type::int()),
            Type::string(),
        )));
        assert!(is_printable(&Type::result(Type::int(), Type::bool())));
        assert!(is_printable(&Type::con("ParseError", vec![])));
        assert!(is_printable(&Type::list(Type::con("MathError", vec![]))));
        let callback = Type::fun(vec![Type::int()], Type::int());
        assert!(!is_printable(&Type::result(Type::int(), callback.clone())));
        assert!(!is_printable(&Type::list(callback)));
        assert!(!is_printable(&Type::con(
            names::MAP,
            vec![Type::string(), Type::int()]
        )));
        assert!(!is_printable(&Type::con(names::FIBER, vec![Type::int()])));
        assert!(!is_printable(&Type::fun(vec![Type::int()], Type::int())));
        let sealed = Type::list(Type::con("M::Token", vec![]));
        assert!(is_printable(&sealed));
        let token = |name: &str, _: &[Type]| {
            if name == "M::Token" {
                Named::Sealed
            } else {
                Named::Plain
            }
        };
        assert!(!super::is_printable(&sealed, &token, &mut Vec::new()));
    }
}
