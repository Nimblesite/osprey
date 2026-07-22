//! Bridges Hindley-Milner inference ([`osprey_types`]) to the backend's LLVM
//! type lattice. Inference runs to completion before emission starts and lands
//! in a finished table ([`ProgramTypes`]); this module just maps an inferred
//! [`Type`] to the [`LType`] the value travels as, so emission never threads
//! inference state. Unresolved/polymorphic variables degrade to `i64`, matching
//! the C runtime's uniform machine-word representation for generic values.

use crate::llty::LType;
use osprey_types::{names, Type};

/// Map an inferred type to the LLVM type a runtime value of it travels as.
pub fn ltype_of(ty: &Type) -> LType {
    match ty {
        Type::Con { name, args } => ltype_of_con(name, args),
        // A function reference is a code pointer; values never hold one directly
        // in the lowered programs (calls are direct), so treat as a handle.
        // Records and unions are runtime handles too.
        Type::Fun { .. } | Type::Record { .. } | Type::Union { .. } => LType::Ptr,
        Type::Var(_) => LType::I64,
    }
}

fn ltype_of_con(name: &str, args: &[Type]) -> LType {
    match name {
        // Int, unit and any travel as a machine word — as do fiber and channel
        // handles, which are runtime ids drawn from one shared counter, not
        // pointers.
        names::INT | names::UNIT | names::ANY | names::FIBER | names::CHANNEL => LType::I64,
        names::FLOAT => LType::Double,
        names::STRING => LType::Str,
        names::BOOL => LType::I1,
        // Result<T, E> at a value site carries its unwrapped success value (the
        // auto-unwrap the type checker already applied), so it travels as T.
        names::RESULT => args.first().map_or(LType::I64, ltype_of),
        // Collections and pointers — opaque runtime handles. A nullary user
        // type name (nominal record/union referenced by name) is also an
        // opaque handle, so the wildcard covers them all.
        _ => LType::Ptr,
    }
}

/// The Osprey owner type name to tag an aggregate value with, if `ty` is a
/// nominal record/union (so field access / match can recover its layout).
/// Scalars, collections and `Result` (auto-unwrapped at value sites) carry no
/// owner.
pub fn owner_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Record { name, .. } | Type::Union { name, .. } => Some(name.clone()),
        Type::Con { name, .. } => match name.as_str() {
            names::INT
            | names::FLOAT
            | names::STRING
            | names::BOOL
            | names::UNIT
            | names::ANY
            | names::RESULT
            | names::LIST
            | names::MAP
            | names::FIBER
            | names::CHANNEL
            | names::PTR => None,
            other => Some(other.to_string()),
        },
        _ => None,
    }
}

/// The type NAME a runtime value of `ty` provably carries as a heap
/// constructor block, if any — the [`crate::meta::MetaField::PtrDirect`]
/// proof obligation. The caller must still check the name against the
/// declared-union table (and the extern-return poison set): only then is
/// "every value of this type is a constructor-built ARC body or NULL" true.
/// `Result<T, E>` auto-unwraps at value sites, so it proves whatever `T` does.
pub fn proven_heap_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Union { name, .. } => Some(name),
        Type::Con { name, args } if name == names::RESULT => {
            args.first().and_then(proven_heap_name)
        }
        Type::Con { name, .. } => Some(name),
        _ => None,
    }
}

/// When `ty` is `Result<T, E>`, the inner success type `T` as an [`LType`].
/// Used to carry the `{ T, i8 }*` Result block across call/return boundaries.
pub fn result_inner(ty: &Type) -> Option<LType> {
    match ty {
        Type::Con { name, args } if name == names::RESULT => args.first().map(ltype_of),
        _ => None,
    }
}

/// The canonical function-VALUE return: `Result<T, E>` → `T`, for ANY `E`.
///
/// Maker and consumer of a closure cell derive its ABI from *different* types
/// that the checker only relates by assignability (the Result auto-unwrap
/// rule), so the ABI must be identical across an assignability-equivalence
/// class — stripping every Result wrapper is the only normalization with that
/// property. Semantically this is the language's value-site auto-unwrap
/// applied at the function-value boundary: a call through a function value
/// yields the success payload. (Revisit when [ERR-PAYLOAD] lands a real
/// error-carrying ABI.)
pub fn normalize_fn_ret(ty: &Type) -> &Type {
    if let Type::Con { name, args } = ty {
        if name == names::RESULT {
            if let Some(ok) = args.first() {
                return ok;
            }
        }
    }
    ty
}

/// Whether a function type yields a concrete closure ABI: every parameter and
/// the NORMALIZED return are variable-free. (`(int) -> Result<int, E>` with an
/// unresolved `E` is concrete — the canonical ABI strips the wrapper, so the
/// error slot never reaches the ABI.)
pub fn fn_value_concrete(ty: &Type) -> bool {
    match ty {
        Type::Fun { params, ret } => {
            !params.iter().any(osprey_types::has_type_var)
                && !osprey_types::has_type_var(normalize_fn_ret(ret))
        }
        _ => false,
    }
}
