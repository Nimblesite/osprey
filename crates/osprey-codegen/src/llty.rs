//! The small slice of the LLVM type system the code generator emits. Osprey
//! primitives map to `i64` (int), `i1` (bool) and `i8*` (string); `i32` is the
//! C `main` return and `puts`/`sprintf` result. Records, unions and effect
//! closures are not yet lowered — those callers fail loudly rather than guess.

use std::fmt;

/// An LLVM first-class type the emitter knows how to name and move around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        LType::Str | LType::Ptr => "null",
        LType::I1 => "false",
        LType::I64 | LType::I32 => "0",
    }
}

impl LType {
    /// The textual LLVM spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LType::I64 => "i64",
            LType::I1 => "i1",
            LType::I32 => "i32",
            LType::Double => "double",
            // `Str` and `Ptr` are semantically distinct handles that share the
            // same LLVM spelling `i8*`.
            LType::Str | LType::Ptr => "i8*",
        }
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
    pub operand: String,
    /// The LLVM type the operand travels as.
    pub ty: LType,
    /// For aggregate handles ([`LType::Ptr`]): the Osprey owner type name
    /// (`Point`, `Shape`, `Result`, …) so field access and `match` can recover
    /// the heap layout. `None` for scalars and untyped handles.
    pub osp_ty: Option<String>,
    /// When `Some(inner)`, this value is a `Result<inner, _>` carried as a
    /// pointer to a heap block `{ inner, i8 disc }` (disc 0 = Success). Match,
    /// `toString` and value-site coercion read this to branch on the
    /// discriminant or auto-unwrap the success payload — every fallible
    /// producer in the backend builds exactly this block shape.
    pub result_inner: Option<LType>,
    /// The Osprey owner type to tag the success payload with when this Result is
    /// unwrapped — e.g. a `Result<List<int>, _>` from indexing a list-of-lists
    /// carries `[]i64` so the unwrapped element is itself indexable. `None` for
    /// scalar payloads.
    pub payload_owner: Option<String>,
    /// For a `Fiber<T>` handle: the element type `T` the fiber's result was
    /// boxed from, so `await` can unbox the uniform `i64` result back to `T`
    /// (a string fiber result is a pointer, not an integer). `None` for
    /// non-fiber values (then `await` keeps the legacy `i64` result).
    pub fiber_elem: Option<LType>,
}

impl Value {
    /// A plain SSA value: an operand paired with its LLVM type.
    pub fn new(operand: impl Into<String>, ty: LType) -> Value {
        Value {
            operand: operand.into(),
            ty,
            osp_ty: None,
            result_inner: None,
            payload_owner: None,
            fiber_elem: None,
        }
    }

    /// An aggregate handle tagged with its Osprey owner type name.
    pub fn handle(operand: impl Into<String>, owner: impl Into<String>) -> Value {
        Value {
            operand: operand.into(),
            ty: LType::Ptr,
            osp_ty: Some(owner.into()),
            result_inner: None,
            payload_owner: None,
            fiber_elem: None,
        }
    }

    /// A `Result<inner, _>` value: `operand` points at a
    /// `{ inner, i8 disc, i8* errmsg }` block.
    pub fn result(operand: impl Into<String>, inner: LType) -> Value {
        Value {
            operand: operand.into(),
            ty: LType::Ptr,
            osp_ty: Some("Result".to_string()),
            result_inner: Some(inner),
            payload_owner: None,
            fiber_elem: None,
        }
    }

    /// Tag this fiber handle with its element type so `await` can unbox the
    /// boxed `i64` result back to it.
    #[must_use]
    pub fn with_fiber_elem(mut self, elem: LType) -> Value {
        self.fiber_elem = Some(elem);
        self
    }

    /// This value re-tagged with an Osprey owner type name.
    #[must_use]
    pub fn with_owner(mut self, owner: Option<String>) -> Value {
        self.osp_ty = owner;
        self
    }

    /// This Result re-tagged with the owner type of its success payload (so an
    /// unwrapped element keeps its handle identity — e.g. a nested list).
    #[must_use]
    pub fn with_payload_owner(mut self, owner: Option<String>) -> Value {
        self.payload_owner = owner;
        self
    }

    /// The canonical Unit value — Osprey `Unit` carries no data, so it is the
    /// `i64 0` placeholder a side-effecting expression yields.
    #[must_use]
    pub fn unit() -> Value {
        Value::new("0", LType::I64)
    }

    /// The LLVM type spelling this value travels as — the precise Result block
    /// pointer for a Result, else the plain [`LType`].
    #[must_use]
    pub fn llvm_ty(&self) -> String {
        ret_spelling(self.ty, self.result_inner)
    }

    /// The Result block struct spelling (no pointer), or `None` for a non-Result.
    #[must_use]
    pub fn result_struct_ty(&self) -> Option<String> {
        self.result_inner.map(result_struct_ty)
    }

    /// Render as a typed operand, e.g. `i64 %3` — the form arguments and `ret`
    /// take.
    #[must_use]
    pub fn typed(&self) -> String {
        format!("{} {}", self.llvm_ty(), self.operand)
    }
}
