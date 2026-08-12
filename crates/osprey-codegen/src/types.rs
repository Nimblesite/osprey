//! Bridges Hindley-Milner inference ([`osprey_types`]) to the backend's LLVM
//! type lattice. Inference runs to completion before emission starts and lands
//! in a finished table ([`ProgramTypes`]); this module just maps an inferred
//! [`Type`] to the [`LType`] the value travels as, so emission never threads
//! inference state. Unresolved/polymorphic variables degrade to `i64`, matching
//! the C runtime's uniform machine-word representation for generic values.

use crate::llty::LType;
use osprey_types::{names, Type};

/// The `LType` of a container's element type argument when inference resolved
/// it concretely — the input to an element-typed owner tag
/// ([`crate::llty::elem_tagged_owner`]). A still-polymorphic element has no
/// type to record, so its container stays untagged.
pub(crate) fn scalar_elem(elem: Option<&Type>) -> Option<LType> {
    elem.filter(|ty| !osprey_types::has_type_var(ty))
        .map(ltype_of)
}

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
        // `Value::result` separately records the discriminant-bearing pointer
        // ABI; this is the success-slot type inside that block.
        names::RESULT => args.first().map_or(LType::I64, ltype_of),
        // Collections and pointers — opaque runtime handles. A nullary user
        // type name (nominal record/union referenced by name) is also an
        // opaque handle, so the wildcard covers them all.
        _ => LType::Ptr,
    }
}

/// The Osprey owner type name to tag an aggregate value with, if `ty` is a
/// nominal record/union (so field access / match can recover its layout).
/// Scalars, collections and `Result` carry no nominal aggregate owner. A
/// `GpuBuffer<elem>` with a concrete scalar element carries the element-typed
/// tag `Gpu#<spelling>` ([`crate::gpu::GPU_TAG`]), the same convention flat
/// list literals use (`[]double`), so combinator lowering recovers the
/// element's `LType` through parameters and returns [GPU-BUFFER-ELEM].
pub fn owner_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Record { name, .. } | Type::Union { name, .. } => Some(name.clone()),
        Type::Con { name, args } => match name.as_str() {
            names::INT
            | names::FLOAT
            | names::STRING
            | names::BOOL
            | names::UNIT
            | names::ANY
            | names::RESULT
            | names::MAP
            | names::ITERATOR
            | names::FIBER
            | names::CHANNEL
            | names::PTR => None,
            names::GPU_BUFFER => Some(crate::gpu::buffer_owner(args.first())),
            // A `List<T>` handle carries its element the same way, so a float
            // list read back through a parameter, a return or a field is
            // floats rather than the `i64` words the runtime stores it as
            // ([`crate::collections::LIST_TAG`]).
            names::LIST => Some(crate::collections::list_owner(scalar_elem(args.first()))),
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
/// A `Result<T, E>` is its own discriminated block and therefore does not prove
/// that the outer value is a heap value of `T`.
pub fn proven_heap_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Con { name, .. } if name == names::RESULT => None,
        // A declared union or any other named constructor: the name IS the proof.
        Type::Union { name, .. } | Type::Con { name, .. } => Some(name),
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

/// Whether a function type yields a concrete closure ABI: every parameter and
/// return are variable-free. Result returns keep their wrapper in the ABI.
pub fn fn_value_concrete(ty: &Type) -> bool {
    match ty {
        Type::Fun { params, ret } => {
            !params.iter().any(osprey_types::has_type_var) && !osprey_types::has_type_var(ret)
        }
        _ => false,
    }
}
