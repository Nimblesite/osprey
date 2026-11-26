//! Which types have a rendering: `print`, `toString`, interpolation and the
//! assertion operands all ask the same question [BUILTIN-TOSTRING].

use crate::ty::{names, Type};

/// Values supported by `runtime::to_string_value`. `any` is retained as the
/// explicit erased-compatibility escape hatch; concrete aggregate and runtime
/// handles are rejected. Both sides of a Result are formatted recursively.
/// [BUILTIN-TOSTRING]: every value has a rendering except a function and the
/// runtime handles in [`UNPRINTABLE`], alone or inside a record, union, list
/// or Result.
pub(super) fn is_printable(ty: &Type, named: Lookup<'_>, seen: &mut Vec<String>) -> bool {
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
/// fiber handle, which interpolates as its spawn number —
/// `src/osprey/tests/regressions/fiber` prints `ids 1 2 3 4` from four of them. `print` has never taken a fiber on
/// its own and still does not.
pub(super) fn is_interpolatable(ty: &Type, named: Lookup<'_>) -> bool {
    is_printable(ty, named, &mut Vec::new()) || ty.is_named(names::FIBER)
}
