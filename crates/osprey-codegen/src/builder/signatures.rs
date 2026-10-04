//! Lowered function and stateful-handle signatures.
use super::{ltype_of, LType, ProgramTypes, Type, Value};

/// One function-parameter ABI slot. Result parameters travel as opaque `i8*`
/// arguments plus the success-layout metadata needed to reconstruct their
/// discriminant-bearing block inside the callee.
/// The element ABI of a STATEFUL HANDLE — `Fiber<T>` or `Channel<T>`.
///
/// Both travel as a bare `i64` runtime id and both hand their element back
/// through a uniform `i64` wire word (`fiber_await`, `channel_recv`), so the
/// receiving side can only reconstruct a pointer element from the handle's
/// static type. Named for the fiber case it was written for; a channel needs
/// exactly the same thing, and without it every `recv` of a list, map or
/// string handed back the raw word ([CONCURRENCY-CHANNEL]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FiberSig {
    pub(crate) elem: LType,
    pub(crate) result_inner: Option<LType>,
    /// The element's aggregate owner tag — `List#double`, `Map#i8*`, a record
    /// name. Carried HERE rather than rebuilt by each consumer: a handle's own
    /// `osp_ty` is empty (an id is a machine word owning nothing), so the
    /// element tag is the only record of what the uniform wire word means. When
    /// each consumer reconstructed it from a declaration instead, every route
    /// with no declaration to consult — a function-value parameter, a closure
    /// capture — silently bound `None` and `recv` unboxed an untagged pointer
    /// ([CONCURRENCY-CHANNEL]).
    pub(crate) elem_owner: Option<String>,
    /// The owner tag of the Success payload when the element is a `Result`.
    pub(crate) elem_payload_owner: Option<String>,
}

impl FiberSig {
    pub(crate) fn of(prog: &ProgramTypes, ty: &Type) -> Option<Self> {
        let Type::Con { name, args } = ty else {
            return None;
        };
        if name != osprey_types::names::FIBER && name != osprey_types::names::CHANNEL {
            return None;
        }
        let elem = args.first()?;
        let result_inner = crate::types::result_inner(elem);
        Some(Self {
            elem: if result_inner.is_some() {
                LType::Ptr
            } else {
                ltype_of(elem)
            },
            result_inner,
            elem_owner: crate::types::elem_tag(prog, Some(elem)),
            elem_payload_owner: crate::types::result_payload_owner(prog, elem),
        })
    }

    pub(crate) fn restore(self, mut value: Value) -> Value {
        value.fiber_elem = Some(self.elem);
        value.fiber_elem_result_inner = self.result_inner;
        value.fiber_elem_owner = self.elem_owner;
        value.fiber_elem_payload_owner = self.elem_payload_owner;
        value
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParamSig {
    pub(crate) ty: LType,
    pub(crate) result_inner: Option<LType>,
    pub(crate) fiber: Option<FiberSig>,
    pub(crate) inferred_type: Option<Type>,
}

impl ParamSig {
    pub(crate) fn of(prog: &ProgramTypes, ty: &Type) -> Self {
        let fiber = FiberSig::of(prog, ty);
        match crate::types::result_inner(ty) {
            Some(inner) => Self {
                ty: LType::Ptr,
                result_inner: Some(inner),
                fiber,
                inferred_type: Some(ty.clone()),
            },
            None => Self {
                ty: ltype_of(ty),
                result_inner: None,
                fiber,
                inferred_type: Some(ty.clone()),
            },
        }
    }
}

/// A function value's lowered signature: parameter ABI slots, the return
/// [`LType`], (when it returns `Result<T, _>`) the success inner type, any
/// Fiber element shape that must survive the erased integer ABI, and the
/// return's OWNER tag (the Success payload owner for a Result).
///
/// The owner is the fifth slot because a closure call had nowhere to put it: a
/// named function recovers it through [`super::Codegen::fn_ret_owner`], but a call
/// through a function value has only this signature to go on, so a lambda
/// returning `List<float>` handed back an untagged `i8*` and `listGet` on it
/// met an `i64` payload against a `double` default. Implements
/// [BUILTIN-LIST-GET], [TYPE-GENERICS-FN].
pub(crate) type FnSig = (
    Vec<ParamSig>,
    LType,
    Option<LType>,
    Option<FiberSig>,
    Option<String>,
);
