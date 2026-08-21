//! The small slice of the LLVM type system the code generator emits. Osprey
//! primitives map to `i64` (int), `i1` (bool) and `i8*` (string); `i32` is the
//! C `main` return and `puts`/`sprintf` result. Records, unions, collections,
//! fibers, and effect closures use opaque pointer values.

use std::fmt;

/// An LLVM first-class type the emitter knows how to name and move around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LType {
    /// 64-bit integer — Osprey `int`.
    I64,
    /// 1-bit integer — Osprey `bool`.
    I1,
    /// `i8*` — an Osprey `string` (NUL-terminated C string).
    Str,
    /// 32-bit integer — `main` return / libc call results.
    I32,
    /// `double` — Osprey `float`.
    Double,
    /// `i8*` carrying a runtime handle/pointer (record, list, map, fiber, …).
    /// Distinguished from [`LType::Str`] so it is never strcmp'd or printed as
    /// text directly.
    Ptr,
    /// `i8*` to an erased-`any` box `{ i8* desc, i64 payload }` — a value that
    /// crossed into `any` and carries its runtime shape descriptor
    /// ([`crate::anybox`], [TYPE-ANY]). Distinct from [`LType::I64`] so
    /// ownership, the effect mailbox and rendering can SEE the erasure —
    /// `I64` being simultaneously every `int` and every erased word is what
    /// made #208's over-release and the address-as-integer prints
    /// undetectable.
    Any,
}

/// The generator role of a parameter register, in the shared role table of
/// [`osprey_ast::generated_name`].
const PARAM_ROLE: &str = "p";

/// The LLVM register name of parameter `index`. Parameters are named
/// POSITIONALLY, never after their source identifier: that is what makes an
/// ML/Default twin pair byte-identical when the two authors spell a parameter
/// differently — including the generated scrutinee of an equational clause set,
/// which has no source spelling at all ([FLAVOR-IR-EQUIV], [FLAVOR-ML-CLAUSES]).
/// The `$` sigil that keeps the name from colliding with a local comes from
/// [`osprey_ast::generated_name`], the one definition of the scheme; the source
/// name survives in DWARF via `emit_debug_param`.
#[must_use]
pub(crate) fn param_register(index: usize) -> String {
    osprey_ast::generated_name(PARAM_ROLE, index)
}

/// The LLVM literal that zero-initialises `ty` — the payload of an `Error`
/// block, an unfilled list slot, an uninitialised accumulator. One table, so a
/// new [`LType`] cannot acquire a different zero in each caller.
pub(crate) const fn zero_literal(ty: LType) -> &'static str {
    match ty {
        LType::Double => "0.0",
        LType::Str | LType::Ptr | LType::Any => "null",
        LType::I1 => "false",
        LType::I64 | LType::I32 => "0",
    }
}

impl LType {
    /// The textual LLVM spelling.
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LType::I64 => "i64",
            LType::I1 => "i1",
            LType::I32 => "i32",
            LType::Double => "double",
            // `Str`, `Ptr` and `Any` are semantically distinct handles that
            // share the same LLVM spelling `i8*`.
            LType::Str | LType::Ptr | LType::Any => "i8*",
        }
    }

    /// Whether a slot of this type holds a REFERENCE the owner must release.
    /// The one definition every ownership decision keys off — a container's
    /// element-kind flag, a rebind's release path, a fiber-result flag, a
    /// struct field's drop mask.
    ///
    /// Spelling this as a hand-written `Str | Ptr` at each site is what let
    /// [`LType::Any`] — managed since #208 gave it a box — slip past four of
    /// them at once: a list literal tagged its boxes unmanaged, a runtime
    /// container never released them, a reassigned `mut` leaked the box it
    /// replaced, and a `Result<any, _>` was marked payload-free. One predicate
    /// means the NEXT managed [`LType`] cannot be forgotten site by site.
    #[must_use]
    pub(crate) fn is_managed_ptr(self) -> bool {
        matches!(self, LType::Str | LType::Ptr | LType::Any)
    }
}

/// The element descriptor spelled after `prefix` in a value's owner tag:
/// `Gpu#double` on a buffer, `List#double` on a runtime list handle,
/// `[]double` on a flat list literal, `Map#List#i8*` on a map of string lists.
/// The container kinds differ; the element spelling does not, so every
/// container reads and writes its tag through this one vocabulary.
pub(crate) fn elem_of_tag(v: &Value, prefix: &str) -> Option<String> {
    Some(v.osp_ty.as_deref()?.strip_prefix(prefix)?.to_string())
}

/// The tag spelling of an erased-`any` element. It cannot use the LLVM
/// spelling: `i8*` is also every string and handle, and a tag must name the
/// element unambiguously so a read-back re-types the word correctly.
pub(crate) const ANY_TAG_SPELLING: &str = "any";

/// The tag spelling of a handle element with no owner to name — a closure, a
/// bare runtime pointer. Distinct from `i8*`, which means "string": recovering
/// an anonymous handle as [`LType::Str`] would strcmp and print it as text.
pub(crate) const PTR_TAG_SPELLING: &str = "ptr";

/// The element a tag spelling names: the [`LType`] its uniform `i64` storage
/// word is recovered at and, for a HANDLE element, the owner descriptor to
/// re-tag the recovered value with.
///
/// The handle case is what makes nesting work. A `Map<string, List<int>>` tags
/// `Map#List#i64`, so `mapGet` hands back a value that still knows it is a
/// `List<int>` — without it the retrieved word was typed `int`, and
/// `listLength` of it, or a `?: [0]` default beside it, could not be emitted at
/// all. [BUILTIN-MAP-GET] [BUILTIN-LIST-GET]
pub(crate) fn elem_of_spelling(tag: &str) -> (LType, Option<String>) {
    match tag {
        "i64" => (LType::I64, None),
        "double" => (LType::Double, None),
        "i1" => (LType::I1, None),
        // Before `i8*`: an erased box IS an `i8*`, but recovering it as
        // `LType::Str` would strcmp and print the box instead of dispatching
        // through its shape descriptor.
        ANY_TAG_SPELLING => (LType::Any, None),
        PTR_TAG_SPELLING => (LType::Ptr, None),
        "i8*" => (LType::Str, None),
        owner => (LType::Ptr, Some(owner.to_string())),
    }
}

/// The tag spelling of a value used as a container element: its own owner
/// descriptor when it is a handle that has one, else its LLVM spelling.
///
/// `Str` belongs with the scalars. The element ABI is a uniform `i64` word, so
/// an untagged string element came back typed `int`: a `char*` printed as its
/// own address, and `listGet(xs, 0) == "b"` reached `as_i64` on a pointer.
/// [BUILTIN-LIST-GET]
pub(crate) fn elem_spelling(elem: &Value) -> Option<String> {
    // An erased element is tagged by its ERASURE and nothing else. Its LLVM
    // spelling `i8*` already means "string element", and any owner it carried
    // names the shape it had BEFORE the box — either one makes the read-back
    // re-type a box pointer as the thing it erased, which is how `[erased()]`
    // came back as the box's own address.
    if elem.ty == LType::Any {
        return Some(ANY_TAG_SPELLING.to_string());
    }
    // A `Result` block is its own discriminated pointer, not an element shape a
    // tag could name; leaving it untagged keeps the storage word honest.
    if elem.result_inner.is_some() {
        return None;
    }
    elem.osp_ty
        .clone()
        .or_else(|| scalar_spelling(elem.ty).map(str::to_string))
}

/// The tag spelling of a scalar element `LType` — the whole vocabulary of a
/// container whose element is never a handle, such as a GPU buffer.
pub(crate) fn scalar_spelling(ty: LType) -> Option<&'static str> {
    match ty {
        LType::I64 | LType::Double | LType::I1 | LType::Str => Some(ty.as_str()),
        LType::Any => Some(ANY_TAG_SPELLING),
        LType::Ptr => Some(PTR_TAG_SPELLING),
        // The C `main` return width is never an Osprey element.
        LType::I32 => None,
    }
}

/// The owner tag for a container holding `elem` words: `<prefix><spelling>`
/// when the element has a spelling the uniform `i64` element ABI would
/// otherwise flatten, the untyped `bare` owner when it has none.
pub(crate) fn elem_tagged_owner(prefix: &str, bare: &str, elem: Option<&str>) -> String {
    match elem {
        Some(tag) => format!("{prefix}{tag}"),
        None => bare.to_string(),
    }
}

impl fmt::Display for LType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The LLVM spelling of a `Result<T, E>` heap block:
/// `{ T value, i8 disc, i8* errmsg }`. Slot 0 is the success payload; slot 1 the
/// discriminant (0 = Success, 1 = Error); slot 2 the error-message string
/// (`null` when Success or when the producer set no message). The single source
/// of truth for the Result ABI layout — every builder/reader spells it via here.
#[must_use]
pub fn result_struct_ty(inner: LType) -> String {
    format!("{{ {inner}, i8, i8* }}")
}

/// The LLVM spelling of a return slot: the Result block pointer when the
/// callee returns `Result<T, _>`, else the plain scalar type.
pub(crate) fn ret_spelling(ret_ty: LType, ret_inner: Option<LType>) -> String {
    match ret_inner {
        Some(inner) => format!("{}*", result_struct_ty(inner)),
        None => ret_ty.to_string(),
    }
}

/// Render each item and comma-join — the LLVM argument/parameter list shape.
pub(crate) fn comma_join<T>(xs: &[T], f: impl Fn(&T) -> String) -> String {
    xs.iter().map(f).collect::<Vec<_>>().join(", ")
}

/// An SSA value: a textual operand (`%3`, a literal like `42`, or a `getelementptr`
/// result) paired with its LLVM type.
#[derive(Debug, Clone)]
pub struct Value {
    /// The textual LLVM operand: a register (`%3`), a literal (`42`), or an
    /// instruction result.
    pub(crate) operand: String,
    /// The LLVM type the operand travels as.
    pub(crate) ty: LType,
    /// For aggregate handles ([`LType::Ptr`]): the Osprey owner type name
    /// (`Point`, `Shape`, `Result`, …) so field access and `match` can recover
    /// the heap layout. `None` for scalars and untyped handles.
    pub(crate) osp_ty: Option<String>,
    /// When `Some(inner)`, this value is a `Result<inner, _>` carried as a
    /// pointer to a heap block `{ inner, i8 disc }` (disc 0 = Success). Match,
    /// `?:`, failure-preserving arithmetic, and Result rendering read this to
    /// branch on the discriminant; ordinary value sites preserve the whole
    /// block. Every fallible producer in the backend builds this exact shape.
    pub(crate) result_inner: Option<LType>,
    /// Whether `result_inner` is only the physical placeholder layout chosen
    /// by a bare `Error { message }` constructor.  An Error has no success
    /// payload from which to discover `T`, so joins and contextual boundaries
    /// must re-layout it to a concrete Success arm before the Result escapes.
    pub(crate) result_inner_is_placeholder: bool,
    /// The Osprey owner type to tag the success payload with when this Result is
    /// unwrapped — e.g. a `Result<List<int>, _>` from indexing a list-of-lists
    /// carries `[]i64` so the unwrapped element is itself indexable. `None` for
    /// scalar payloads.
    pub(crate) payload_owner: Option<String>,
    /// For a `Fiber<T>` handle: the element type `T` the fiber's result was
    /// boxed from, so `await` can unbox the uniform `i64` result back to `T`
    /// (a string fiber result is a pointer, not an integer). `None` for
    /// non-fiber values (then `await` keeps the legacy `i64` result).
    pub(crate) fiber_elem: Option<LType>,
    /// Aggregate owner metadata for the element carried by a Fiber.
    pub(crate) fiber_elem_owner: Option<String>,
    /// Result layout metadata for a `Fiber<Result<T, E>>`; `await` restores the
    /// whole Result block rather than treating its pointer as the payload.
    pub(crate) fiber_elem_result_inner: Option<LType>,
    /// Aggregate owner metadata for the Success payload of a fiber Result.
    pub(crate) fiber_elem_payload_owner: Option<String>,
}

impl Value {
    /// A plain SSA value: an operand paired with its LLVM type.
    pub(crate) fn new(operand: impl Into<String>, ty: LType) -> Value {
        Value {
            operand: operand.into(),
            ty,
            osp_ty: None,
            result_inner: None,
            result_inner_is_placeholder: false,
            payload_owner: None,
            fiber_elem: None,
            fiber_elem_owner: None,
            fiber_elem_result_inner: None,
            fiber_elem_payload_owner: None,
        }
    }

    /// An aggregate handle tagged with its Osprey owner type name.
    pub(crate) fn handle(operand: impl Into<String>, owner: impl Into<String>) -> Value {
        Value {
            operand: operand.into(),
            ty: LType::Ptr,
            osp_ty: Some(owner.into()),
            result_inner: None,
            result_inner_is_placeholder: false,
            payload_owner: None,
            fiber_elem: None,
            fiber_elem_owner: None,
            fiber_elem_result_inner: None,
            fiber_elem_payload_owner: None,
        }
    }

    /// A `Result<inner, _>` value: `operand` points at a
    /// `{ inner, i8 disc, i8* errmsg }` block.
    pub(crate) fn result(operand: impl Into<String>, inner: LType) -> Value {
        Value {
            operand: operand.into(),
            ty: LType::Ptr,
            osp_ty: Some("Result".to_string()),
            result_inner: Some(inner),
            result_inner_is_placeholder: false,
            payload_owner: None,
            fiber_elem: None,
            fiber_elem_owner: None,
            fiber_elem_result_inner: None,
            fiber_elem_payload_owner: None,
        }
    }

    /// Tag this fiber handle with its element type so `await` can unbox the
    /// boxed `i64` result back to it.
    #[must_use]
    pub(crate) fn with_fiber_elem(mut self, elem: &Value) -> Value {
        self.fiber_elem = Some(elem.ty);
        self.fiber_elem_owner.clone_from(&elem.osp_ty);
        self.fiber_elem_result_inner = elem.result_inner;
        self.fiber_elem_payload_owner
            .clone_from(&elem.payload_owner);
        self
    }

    /// This value re-tagged with an Osprey owner type name.
    #[must_use]
    pub(crate) fn with_owner(mut self, owner: Option<String>) -> Value {
        self.osp_ty = owner;
        self
    }

    /// This Result re-tagged with the owner type of its success payload (so an
    /// unwrapped element keeps its handle identity — e.g. a nested list).
    #[must_use]
    pub(crate) fn with_payload_owner(mut self, owner: Option<String>) -> Value {
        self.payload_owner = owner;
        self
    }

    /// Mark this Result's success-slot layout as the unconstrained placeholder
    /// carried by a bare Error constructor.
    #[must_use]
    pub(crate) fn with_result_inner_placeholder(mut self) -> Value {
        self.result_inner_is_placeholder = true;
        self
    }

    /// The canonical Unit value — Osprey `Unit` carries no data, so it is the
    /// `i64 0` placeholder a side-effecting expression yields.
    #[must_use]
    pub(crate) fn unit() -> Value {
        Value::new("0", LType::I64)
    }

    /// The LLVM type spelling this value travels as — the precise Result block
    /// pointer for a Result, else the plain [`LType`].
    #[must_use]
    pub(crate) fn llvm_ty(&self) -> String {
        ret_spelling(self.ty, self.result_inner)
    }

    /// The Result block struct spelling (no pointer), or `None` for a non-Result.
    #[must_use]
    pub(crate) fn result_struct_ty(&self) -> Option<String> {
        self.result_inner.map(result_struct_ty)
    }

    /// Render as a typed operand, e.g. `i64 %3` — the form arguments and `ret`
    /// take.
    #[must_use]
    pub(crate) fn typed(&self) -> String {
        format!("{} {}", self.llvm_ty(), self.operand)
    }
}
