//! The emitter state: a growing LLVM module (external declarations, string
//! globals, finished functions) plus the in-progress function (SSA counter,
//! current basic block, lexical scopes). Low-level helpers here only *emit*
//! text; the AST-walking lives in `lower.rs`.

mod allocation;
mod cells;
pub(crate) use cells::CellSlot;
mod debug_metadata;
use debug_metadata::DebugState;
mod debug_types;
mod emit;
mod function_state;
mod function_types;
mod layouts;
mod signatures;
pub(crate) use signatures::{FiberSig, FnSig, ParamSig};

mod debug_scopes;
mod debug_variables;
mod lexical_scope;
pub(crate) use lexical_scope::LexicalScopeState;

use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use crate::types::ltype_of;
use osprey_ast::{Expr, Position};
use osprey_debug::DebugSource;
use osprey_types::{ProgramTypes, Type};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

/// Code generation switches that alter the emitted module without changing
/// Osprey semantics.
#[derive(Debug, Clone, Default)]
pub(crate) struct CodegenOptions {
    /// Source file identity used for LLVM/DWARF debug metadata.
    pub debug_source: Option<DebugSource>,
    /// Instrument coverable lines with hit counters [TESTING-COVERAGE-CODEGEN].
    pub coverage: bool,
    /// Which lowering the GPU combinators use for their kernels
    /// [GPU-KERNEL-EXTRACT].
    pub gpu_kernels: crate::gpu_kernel::GpuKernelMode,
}

/// A lambda kept for inline application at its direct call sites: its
/// parameters, its body, and the position inference keyed its type by.
pub(crate) type LambdaDef = (Vec<osprey_ast::Parameter>, Expr, Option<Position>);

/// What the program turned out to CONTAIN — decided while lowering, read once
/// by `main`'s epilogue. These belong together because they are answered the
/// same way and spent in the same place: each one buys a cleanup call or an
/// exit status, and a program that never sets one must emit a module identical
/// to a program that could not have set it.
#[derive(Debug, Default)]
pub(crate) struct Lowered {
    /// A testing built-in was lowered — `main` returns the TAP epilogue's exit
    /// status [TESTING-EXIT].
    pub(crate) testing: bool,
    /// A fiber was spawned. Completed fiber results keep one runtime owner so
    /// repeated `await` calls can each receive a live value; `main` releases
    /// those runtime owners during its final cleanup.
    pub(crate) fibers: bool,
    /// A `send` or `recv` was lowered, so the program's exit must release
    /// whatever is still buffered in a channel ([CONCURRENCY-CHANNEL]).
    pub(crate) channels: bool,
}

/// Accumulates a whole module while lowering one function at a time.
pub(crate) struct Codegen {
    pub(crate) application_caller: Option<ProgramTypes>,
    /// `declare` lines, de-duplicated and stably ordered.
    externs: BTreeSet<String>,
    /// Global constant definitions (string literals).
    globals: Vec<String>,
    /// Rendered `define` blocks.
    funcs: Vec<String>,
    glob_count: usize,

    // ---- current function state ----
    reg_count: usize,
    label_count: usize,
    cur_lines: Vec<String>,
    cur_block: String,
    scopes: Vec<HashMap<String, Value>>,
    /// Stable identity of each lexical scope. Sibling blocks have equal depth
    /// but distinct ids, which ARC last-use bookkeeping must not conflate.
    scope_ids: Vec<usize>,
    next_scope_id: usize,

    /// Declared parameter names per function, for named-argument ordering.
    pub(crate) fn_params: HashMap<String, Vec<String>>,
    /// Extern parameter names. Kept separate because `fn_params` also identifies
    /// Osprey functions whose callbacks use closure cells rather than C pointers.
    pub(crate) extern_params: HashMap<String, Vec<String>>,
    /// Type names an `extern fn` claims to return (every name in the declared
    /// return type expression, conservatively). A foreign pointer typed as a
    /// union would break the `KIND_MASK_DIRECT` all-children-are-ARC-bodies
    /// proof, so these unions stay on the probing `KIND_MASK`. [GC-ARC-PERCEUS]
    extern_ret_types: BTreeSet<String>,
    /// Nullary union variant name → the module-constant handle every use of it
    /// shares. A payload-free variant (`Leaf`, `None`) is one immutable
    /// immortal value, so it is interned as a single `private global` with a
    /// baked ARC header (rc = -1) instead of a fresh heap block per occurrence —
    /// binarytrees alone stops allocating ~19.6M `Leaf`s. [GC-ARC-PERCEUS]
    nullary_singletons: HashMap<String, String>,
    /// Resolved signatures, constructor layouts and union tags from inference.
    pub(crate) prog: ProgramTypes,
    /// Erased-`any` descriptors, deep-box functions and the candidate row
    /// table ([`crate::anybox`], [TYPE-ANY]).
    pub(crate) anys: crate::anybox::AnyState,
    /// Stream-fusion pipeline: pending `map`/`filter` stages recorded by those
    /// builtins and replayed (in source order) when `forEach`/`fold` consumes
    /// the iterator. Cleared after each consumer.
    pub(crate) pending_iter_ops: Vec<crate::iter::IterOp>,
    /// Let-bound lambdas, kept for inline application at *direct* call sites
    /// (`let f = fn(x) => …` then `f(y)`) — a beta-reduction fast path. The
    /// same lambda is also materialized as a closure cell (`crate::closure`)
    /// so the name works as a first-class value.
    pub(crate) lambdas: HashMap<String, LambdaDef>,
    /// FILE-SCOPE lambdas that materialise no closure cell because their type
    /// is still generic. `lambdas` is per-function state cleared by
    /// [`Codegen::begin_function`], which is right for a local beta-reduction
    /// cache and wrong for a file-scope binding every function may read: the
    /// entry was wiped before the first function body was lowered, and
    /// `idl(7)` emitted a direct call to `@idl`, a symbol no definition
    /// produces ([TYPE-GENERICS-FN], [MODULES-FILE-SCOPE-BINDING]).
    pub(crate) file_lambdas: HashMap<String, LambdaDef>,
    /// Captured factory arguments of file-scope generic lambdas live in
    /// module globals, so readers in other functions load the value once made.
    pub(crate) file_lambda_prefix: HashMap<String, (Vec<osprey_ast::Parameter>, Vec<String>)>,
    /// Generic aliases seeded at file scope, before local aliases can shadow
    /// them while a file-scope lambda is inlined.
    pub(crate) file_aliases: HashMap<String, String>,
    /// Values already lowered for the CALLEE parameters a generic returned
    /// lambda closes over, keyed by the binding name
    /// (`crate::stmt::generic_returned_lambda`).
    ///
    /// `let c = constly("hi")` for `fn constly(v) = |x| => v` evaluates the
    /// argument ONCE here; each later `c(7)` prepends these values to its own,
    /// so the inlined body reads `v` from the binding's evaluation rather than
    /// from whatever scope it was inlined into. These are SSA registers of the
    /// function being emitted, so the map is cleared whenever emission moves to
    /// another function body.
    pub(crate) lambda_prefix: HashMap<String, (Vec<osprey_ast::Parameter>, Vec<Value>)>,
    /// A call-site-concrete ABI for the single returned lambda currently
    /// produced by an inlined handler factory.
    pub(crate) expected_lambda: Option<(Vec<Position>, Type)>,
    /// Top-level functions already wrapped as closure cells (name → the cell's
    /// constant global), so the forwarder is emitted once per module.
    pub(crate) fnval_cells: HashMap<String, String>,
    /// Whether the fiber-result global table has been emitted yet.
    /// Parsed `effect` operation signatures, keyed `"Effect.operation"`.
    /// Monotonic id giving each emitted handler function a unique name.
    handler_count: usize,
    /// Dense operation ids, keyed by `(runtime effect key, operation)`: the
    /// runtime keeps one evidence slot per id, so a `perform` indexes instead
    /// of scanning. Shared by every push and perform of the module.
    operation_ids: HashMap<(String, String), u32>,
    /// Monotonic id giving each lambda lifted to a top-level function (a lambda
    /// used as a value, e.g. passed to a function-typed parameter) a unique name.
    lambda_count: usize,
    /// The kernel lowering this module compiles under [GPU-KERNEL-EXTRACT].
    gpu_kernels: crate::gpu_kernel::GpuKernelMode,
    /// Monotonic id naming each extracted GPU kernel. A counter of its OWN —
    /// sharing `lambda_count` would renumber `__closure_fn_N` in every program
    /// that mixes closures with kernels.
    kernel_count: usize,
    /// Synthetic layouts of anonymous object literals (`{ a: 1, b: "x" }`),
    /// keyed by the generated owner name carried on the handle, so field access
    /// can recover the ordered `(field, LType)` slots.
    obj_layouts: HashMap<String, Vec<ObjField>>,
    /// Monotonic id giving each object literal a unique synthetic owner name.
    obj_count: usize,
    /// User function `(parameters, body)` defs, for inlining a *generic*
    /// function at each call site so its type variables monomorphize to the
    /// concrete argument types there (specialisation by inlining rather than by
    /// emitting a name-mangled copy per instantiation).
    pub(crate) fn_defs: HashMap<String, (Vec<osprey_ast::Parameter>, Expr, Option<Position>)>,
    /// Generic functions currently being inlined — a re-entry guard so a
    /// (mutually) recursive generic call falls back to a direct call instead of
    /// inlining forever.
    pub(crate) inlining: HashSet<String>,
    /// Emitted instantiations of RECURSIVE generic functions, keyed by the call
    /// site's argument representations ([`crate::monofn`]): the one form of
    /// polymorphism resolved by emitting a definition rather than by inlining.
    pub(crate) monofns: HashMap<String, crate::monofn::Instantiation>,
    /// Monotonic id naming each emitted instantiation.
    monofn_count: usize,
    /// Function-typed locals in the current function (a higher-order parameter
    /// `f: (int) -> int`): name → its signature ([`FnSig`]), so a call `f(x)`
    /// lowers to an indirect call through the `i8*` handle.
    pub(crate) fn_ptr_locals: HashMap<String, FnSig>,
    /// The full inferred [`Type`] of each function-typed local, so a chained
    /// application (`let inner = nested(10)` then `inner(20)`) can recover the
    /// returned function's signature.
    pub(crate) fn_value_types: HashMap<String, Type>,
    /// While inlining a generic function, a function-valued parameter bound to a
    /// callee *by name* (`apply(f: toString, …)`): the parameter redirects to
    /// that real callee, so `f(x)` in the body becomes `toString(x)`. This keeps
    /// a builtin or another generic function callable through the parameter.
    pub(crate) call_aliases: HashMap<String, String>,
    /// Names of mutable locals in the current function that an effect handler
    /// arm captures, so they must be promoted to shared heap cells (a plain
    /// `mut` becomes a reference cell the handler owns). Computed per function.
    pub(crate) cell_vars: HashSet<String>,
    /// Live cell-backed bindings (name → its heap slot): a read loads, a
    /// reassignment stores, and an effect handler captures the cell pointer so
    /// `get`/`set` arms share one mutable location — handler-owned state.
    pub(crate) cell_slots: HashMap<String, CellSlot>,
    /// File-scope bindings that some function body reads, and the module
    /// global each one lives in. Module-wide, so it is NOT part of
    /// [`SavedFn`]: a nested handler function reads the same storage its
    /// enclosing function does ([`crate::globals`]).
    ///
    /// Ordered so the end-of-`main` releases are emitted in one fixed order:
    /// register numbering must be a pure function of the source, or a
    /// Default/ML twin pair stops matching [FLAVOR-IR-EQUIV].
    pub(crate) module_globals: BTreeMap<String, crate::globals::GlobalSlot>,
    /// Continuation lowering context while emitting a resuming handler arm.
    pub(crate) resume_ctx: Option<ResumeCodegenContext>,
    /// Whether the expression currently being lowered sits in statement
    /// position, where its value is discarded. A `match` used purely for its
    /// side effects may then have arms of differing LLVM type — there is no
    /// `phi` to type. In value position that disagreement is a hard error
    /// ([`crate::pattern::finish_phi`]).
    pub(crate) value_discarded: bool,
    /// What the program turned out to contain, read once by `main`'s epilogue.
    pub(crate) lowered: Lowered,
    /// LLVM/DWARF debug metadata state, when `--debug` was requested.
    debug: Option<DebugState>,
    /// Coverage instrumentation state, when coverage was requested
    /// [TESTING-COVERAGE-CODEGEN].
    pub(crate) coverage: Option<crate::coverage::CoverageState>,
    /// The Perceus ownership ledger for the in-progress function
    /// [GC-ARC-PERCEUS] (see `crate::arc`).
    pub(crate) arc: crate::arc::ArcLedger,
    /// Monotonic id for hoisted ARC spill slots (`%arc.sN`), per function.
    arc_slot_count: usize,
    /// Registers this function materialised from a `private constant` global
    /// (string literals). They address **rodata**, never the managed heap, so
    /// every dup/drop on one is a guaranteed registry probe-miss — a locked
    /// hash probe that can only ever be a no-op. Classifying them here elides
    /// the call outright ([GC-ARC-PERCEUS] M6b, TR §2.4 "unique/static"
    /// classification). Per-function: register names restart at each
    /// `begin_function`.
    rodata_regs: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct ResumeCodegenContext {
    pub env: String,
    pub coro: String,
    pub drive_fn: String,
    pub answer_ty: LType,
    pub answer_result_inner: Option<LType>,
    pub answer_owner: Option<String>,
    pub answer_payload_owner: Option<String>,
    pub answer_inferred_type: Option<Type>,
    /// Concrete operation-result shape at this handler site. A plain resume
    /// value may be promoted to Success for a Result slot; the inverse is
    /// forbidden and rejected by codegen as well as by the checker.
    pub op_ret_ty: LType,
    pub op_ret_result_inner: Option<LType>,
}

/// One slot of a registered heap-block layout: field name, its LLVM type, and
/// the owner tag the value stored there carried — the only record of a generic
/// record's real field types, which its declaration spells as type parameters.
pub(crate) type ObjField = (String, Value);

/// Saved emission state of a suspended function (see [`Codegen::enter_nested_fn`]).
pub(crate) struct SavedFn {
    lines: Vec<String>,
    block: String,
    regs: usize,
    labels: usize,
    scopes: Vec<HashMap<String, Value>>,
    scope_ids: Vec<usize>,
    /// Saved with the rest of the function frame: the prefix values are SSA
    /// registers of the SUSPENDED function, so a nested body must not read them.
    lambda_prefix: HashMap<String, (Vec<osprey_ast::Parameter>, Vec<Value>)>,
    expected_lambda: Option<(Vec<Position>, Type)>,
    /// Stream-fusion stages are per-function: a stage recorded inside a nested
    /// function body must never replay in the suspended function's next loop.
    pending_iter_ops: Vec<crate::iter::IterOp>,
    /// Cell-promotion state is per-function: a handler arm (a nested function)
    /// gets its own captured cells, never the suspended outer function's.
    cell_vars: HashSet<String>,
    cell_slots: HashMap<String, CellSlot>,
    resume_ctx: Option<ResumeCodegenContext>,
    /// Ownership is per-function: a nested function drops its own owners at
    /// its own epilogue, never the suspended function's [GC-ARC-PERCEUS].
    arc: crate::arc::ArcLedger,
    arc_slot_count: usize,
    rodata_regs: HashSet<String>,
    debug_scope: Option<usize>,
    debug_function: Option<usize>,
    debug_position: Option<Position>,
    debug_retained_nodes: Option<usize>,
    debug_local_ids: Vec<usize>,
}

impl Codegen {
    pub(crate) fn new() -> Codegen {
        Codegen::with_types(ProgramTypes::default())
    }

    /// Build with the inferred program types that drive parameter/return/value
    /// typing.
    pub(crate) fn with_types(prog: ProgramTypes) -> Codegen {
        Codegen::with_options(prog, CodegenOptions::default())
    }

    /// Build with inferred program types and explicit code generation options.
    pub(crate) fn with_options(prog: ProgramTypes, options: CodegenOptions) -> Codegen {
        Codegen {
            externs: BTreeSet::new(),
            globals: Vec::new(),
            funcs: Vec::new(),
            glob_count: 0,
            reg_count: 0,
            label_count: 0,
            cur_lines: Vec::new(),
            cur_block: String::from("entry"),
            value_discarded: false,
            scopes: Vec::new(),
            scope_ids: Vec::new(),
            next_scope_id: 0,
            fn_params: HashMap::new(),
            extern_params: HashMap::new(),
            extern_ret_types: BTreeSet::new(),
            nullary_singletons: HashMap::new(),
            prog,
            pending_iter_ops: Vec::new(),
            lambdas: HashMap::new(),
            file_lambdas: HashMap::new(),
            application_caller: None,
            file_lambda_prefix: HashMap::new(),
            file_aliases: HashMap::new(),
            lambda_prefix: HashMap::new(),
            expected_lambda: None,
            fnval_cells: HashMap::new(),
            handler_count: 0,
            operation_ids: HashMap::new(),
            lambda_count: 0,
            gpu_kernels: options.gpu_kernels,
            kernel_count: 0,
            obj_layouts: HashMap::new(),
            obj_count: 0,
            fn_defs: HashMap::new(),
            inlining: HashSet::new(),
            monofns: HashMap::new(),
            monofn_count: 0,
            fn_ptr_locals: HashMap::new(),
            fn_value_types: HashMap::new(),
            call_aliases: HashMap::new(),
            cell_vars: HashSet::new(),
            cell_slots: HashMap::new(),
            module_globals: BTreeMap::new(),
            resume_ctx: None,
            lowered: Lowered::default(),
            debug: options.debug_source.map(DebugState::new),
            coverage: options
                .coverage
                .then(crate::coverage::CoverageState::default),
            arc: crate::arc::ArcLedger::new(),
            arc_slot_count: 0,
            rodata_regs: HashSet::new(),
            anys: crate::anybox::AnyState::default(),
        }
    }
}

/// Attribute group applied to every generated function: keep frame pointers in
/// ALL functions (Darwin's default drops them in leaves), so the profiler's
/// async frame-pointer chain walk is valid from any sample point. Implements
/// [PROF-CODEGEN-FP], docs/specs/0028-Profiler.md; cost is ~1% (arm64 reserves
/// x29 for the frame chain by ABI anyway).
/// Mirrors `OSP_MAX_OPERATION_IDS` in `compiler/runtime/effects_runtime.h`: the
/// size of the runtime's per-operation evidence table.
const MAX_OPERATION_IDS: u32 = 4096;

const FRAME_POINTER_ATTRS: &str = "attributes #0 = { \"frame-pointer\"=\"all\" }";

/// The swappable allocation hook declaration. `noalias` + the allocator
/// attributes (`allocsize`/`allockind`/`alloc-family`) make LLVM treat
/// `@osp_alloc` as an allocation function for dead-allocation elimination, while
/// a custom `alloc-family` ("osprey", not "malloc") stops LLVM rewriting it to
/// libc `calloc`/`realloc` and bypassing the backend. Implements [MEM-BACKENDS].
const OSP_ALLOC_DECL: &str = "declare noalias i8* @osp_alloc(i64) allocsize(0) allockind(\"alloc,uninitialized\") mustprogress nounwind willreturn \"alloc-family\"=\"osprey\"";

/// The layout-carrying twin of [`OSP_ALLOC_DECL`]: same allocator attributes
/// (so `-O2` dead-allocation elimination still applies), plus the meta word the
/// ARC backend stores in the object header. Implements [GC-ARC-PERCEUS].
const OSP_ALLOC_TAGGED_DECL: &str = "declare noalias i8* @osp_alloc_tagged(i64, i64) allocsize(0) allockind(\"alloc,uninitialized\") mustprogress nounwind willreturn \"alloc-family\"=\"osprey\"";

/// The non-pre-zeroing twin of [`OSP_ALLOC_TAGGED_DECL`] for caller-fully-
/// initialized blocks (`allockind` is already `uninitialized`; the difference
/// is only that the ARC backend skips its drop-safety memset). [GC-ARC-PERCEUS]
const OSP_ALLOC_TAGGED_NOINIT_DECL: &str = "declare noalias i8* @osp_alloc_tagged_noinit(i64, i64) allocsize(0) allockind(\"alloc,uninitialized\") mustprogress nounwind willreturn \"alloc-family\"=\"osprey\"";

/// The resolved heap layout of a constructor.
pub(crate) struct CtorView {
    pub owner: String,
    pub owner_is_record: bool,
    pub tag: i64,
    pub fields: Vec<(String, LType)>,
    /// The `@osp_alloc_tagged` layout word for the `{ i64 tag, fields… }`
    /// block, computed from the Osprey field types (which prove more than the
    /// erased `LType`s: an all-declared-union field set upgrades to the
    /// probe-free `KIND_MASK_DIRECT`). [GC-ARC-PERCEUS]
    pub meta: i64,
}

impl Default for Codegen {
    fn default() -> Self {
        Codegen::new()
    }
}

/// Escape a Rust string into an LLVM `c"..."` body, returning the escaped text
/// and the byte length **including** the trailing NUL. Bytes outside printable
/// ASCII (and `"`/`\`) are emitted as `\HH`.
fn escape_c_string(text: &str) -> (String, usize) {
    let mut out = String::new();
    let bytes = text.as_bytes();
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            0x20..=0x7e => out.push(char::from(b)),
            _ => {
                let _ = write!(out, "\\{b:02X}");
            }
        }
    }
    out.push_str("\\00");
    (out, bytes.len() + 1)
}

fn metadata_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn host_dwarf_version() -> u8 {
    if cfg!(target_os = "macos") {
        4
    } else {
        5
    }
}
