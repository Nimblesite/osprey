//! The emitter state: a growing LLVM module (external declarations, string
//! globals, finished functions) plus the in-progress function (SSA counter,
//! current basic block, lexical scopes). Low-level helpers here only *emit*
//! text; the AST-walking lives in `lower.rs`.

use crate::llty::{LType, Value};
use crate::types::ltype_of;
use osprey_ast::{Expr, Position};
use osprey_debug::DebugSource;
use osprey_types::{ProgramTypes, Type};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

/// Code generation switches that alter the emitted module without changing
/// Osprey semantics.
#[derive(Debug, Clone, Default)]
pub struct CodegenOptions {
    /// Source file identity used for LLVM/DWARF debug metadata.
    pub debug_source: Option<DebugSource>,
    /// Instrument coverable lines with hit counters [TESTING-COVERAGE-CODEGEN].
    pub coverage: bool,
}

/// Accumulates a whole module while lowering one function at a time.
pub struct Codegen {
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

    /// Declared parameter names per function, for named-argument ordering.
    pub(crate) fn_params: HashMap<String, Vec<String>>,
    /// Resolved signatures, constructor layouts and union tags from inference.
    pub(crate) prog: ProgramTypes,
    /// Stream-fusion pipeline: pending `map`/`filter` stages recorded by those
    /// builtins and replayed (in source order) when `forEach`/`fold` consumes
    /// the iterator. Cleared after each consumer.
    pub(crate) pending_iter_ops: Vec<crate::iter::IterOp>,
    /// Let-bound lambdas, kept for inline application at *direct* call sites
    /// (`let f = fn(x) => …` then `f(y)`) — a beta-reduction fast path. The
    /// same lambda is also materialized as a closure cell (`crate::closure`)
    /// so the name works as a first-class value.
    pub(crate) lambdas: HashMap<String, (Vec<osprey_ast::Parameter>, osprey_ast::Expr)>,
    /// Top-level functions already wrapped as closure cells (name → the cell's
    /// constant global), so the forwarder is emitted once per module.
    pub(crate) fnval_cells: HashMap<String, String>,
    /// Whether the fiber-result global table has been emitted yet.
    /// Parsed `effect` operation signatures, keyed `"Effect.operation"`.
    pub(crate) effect_ops: HashMap<String, crate::effects::OpSig>,
    /// Monotonic id giving each emitted handler function a unique name.
    pub(crate) handler_count: usize,
    /// Monotonic id giving each lambda lifted to a top-level function (a lambda
    /// used as a value, e.g. passed to a function-typed parameter) a unique name.
    pub(crate) lambda_count: usize,
    /// Synthetic layouts of anonymous object literals (`{ a: 1, b: "x" }`),
    /// keyed by the generated owner name carried on the handle, so field access
    /// can recover the ordered `(field, LType)` slots.
    pub(crate) obj_layouts: HashMap<String, Vec<(String, LType)>>,
    /// Monotonic id giving each object literal a unique synthetic owner name.
    pub(crate) obj_count: usize,
    /// User function `(parameters, body)` defs, for inlining a *generic*
    /// function at each call site so its type variables monomorphize to the
    /// concrete argument types there (specialisation by inlining rather than by
    /// emitting a name-mangled copy per instantiation).
    pub(crate) fn_defs: HashMap<String, (Vec<osprey_ast::Parameter>, osprey_ast::Expr)>,
    /// Generic functions currently being inlined — a re-entry guard so a
    /// (mutually) recursive generic call falls back to a direct call instead of
    /// inlining forever.
    pub(crate) inlining: HashSet<String>,
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
    /// Continuation lowering context while emitting a resuming handler arm.
    pub(crate) resume_ctx: Option<ResumeCodegenContext>,
    /// Whether any testing built-in was lowered — makes `main` return the TAP
    /// epilogue's exit status [TESTING-EXIT].
    pub(crate) testing_used: bool,
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
}

/// A mutable variable promoted to a heap cell so an effect handler can own it.
/// `ptr` is a `{pointee}*` operand; reads `load` it, writes `store` to it.
#[derive(Clone)]
pub(crate) struct CellSlot {
    pub ptr: String,
    pub pointee: LType,
    pub osp_ty: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ResumeCodegenContext {
    pub env: String,
    pub coro: String,
    pub drive_fn: String,
    pub answer_ty: LType,
    /// Whether the resumed operation's result type (at this handle site's
    /// instantiation) is a `Result` — a resume value then boxes the WHOLE
    /// Result block, never its unwrapped payload, because the perform site
    /// unboxes the slot as a Result pointer. Implements
    /// [EFFECTS-GENERIC-RUNTIME].
    pub op_ret_is_result: bool,
}

/// A function value's lowered signature: parameter [`LType`]s, the return
/// [`LType`], and (when it returns `Result<T, _>`) the success inner type.
pub(crate) type FnSig = (Vec<LType>, LType, Option<LType>);

/// Saved emission state of a suspended function (see [`Codegen::enter_nested_fn`]).
pub(crate) struct SavedFn {
    lines: Vec<String>,
    block: String,
    regs: usize,
    labels: usize,
    scopes: Vec<HashMap<String, Value>>,
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
    debug_scope: Option<usize>,
    debug_position: Option<Position>,
    debug_retained_nodes: Option<usize>,
    debug_local_ids: Vec<usize>,
}

#[derive(Debug, Clone)]
struct DebugState {
    source: DebugSource,
    current_scope: Option<usize>,
    current_position: Option<Position>,
    current_retained_nodes: Option<usize>,
    current_local_ids: Vec<usize>,
    next_id: usize,
    file_id: usize,
    cu_id: usize,
    empty_id: usize,
    subroutine_type_id: usize,
    dwarf_flag_id: usize,
    debug_version_flag_id: usize,
    ident_id: usize,
    dwarf_version: u8,
    i64_type_id: usize,
    i32_type_id: usize,
    bool_type_id: usize,
    double_type_id: usize,
    char_type_id: usize,
    ptr_type_id: usize,
    dynamic: Vec<(usize, String)>,
}

impl DebugState {
    fn new(source: DebugSource) -> Self {
        DebugState {
            source,
            current_scope: None,
            current_position: None,
            current_retained_nodes: None,
            current_local_ids: Vec::new(),
            next_id: 13,
            file_id: 0,
            cu_id: 1,
            empty_id: 2,
            subroutine_type_id: 3,
            dwarf_flag_id: 4,
            debug_version_flag_id: 5,
            ident_id: 6,
            dwarf_version: host_dwarf_version(),
            i64_type_id: 7,
            i32_type_id: 8,
            bool_type_id: 9,
            double_type_id: 10,
            char_type_id: 11,
            ptr_type_id: 12,
            dynamic: Vec::new(),
        }
    }

    fn source_filename(&self) -> String {
        metadata_escape(&self.source.path().display().to_string())
    }

    fn begin_function(&mut self, name: &str, position: Option<Position>) -> usize {
        let retained_id = self.alloc_id();
        let id = self.alloc_id();
        let line = position.map_or(1, |p| p.line.max(1));
        let name = metadata_escape(name);
        self.dynamic.push((
            id,
            format!(
                "!{id} = distinct !DISubprogram(name: \"{name}\", linkageName: \"{name}\", scope: !{}, file: !{}, line: {line}, type: !{}, scopeLine: {line}, spFlags: DISPFlagDefinition, unit: !{}, retainedNodes: !{})",
                self.file_id,
                self.file_id,
                self.subroutine_type_id,
                self.cu_id,
                retained_id
            ),
        ));
        self.current_scope = Some(id);
        self.current_position = position;
        self.current_retained_nodes = Some(retained_id);
        self.current_local_ids.clear();
        id
    }

    fn finish_function(&mut self) {
        if let Some(id) = self.current_retained_nodes.take() {
            let locals = self
                .current_local_ids
                .iter()
                .map(|local| format!("!{local}"))
                .collect::<Vec<_>>()
                .join(", ");
            self.dynamic.push((id, format!("!{id} = !{{{locals}}}")));
        }
        self.current_local_ids.clear();
    }

    fn clear_function(&mut self) {
        self.current_scope = None;
        self.current_position = None;
        self.current_retained_nodes = None;
        self.current_local_ids.clear();
    }

    fn set_position(&mut self, position: Option<Position>) -> Option<Position> {
        let previous = self.current_position;
        if position.is_some() {
            self.current_position = position;
        }
        previous
    }

    fn location_id(&mut self) -> Option<usize> {
        let scope = self.current_scope?;
        let position = self.current_position?;
        let id = self.alloc_id();
        let line = position.line.max(1);
        let column = position.column.saturating_add(1).max(1);
        self.dynamic.push((
            id,
            format!("!{id} = !DILocation(line: {line}, column: {column}, scope: !{scope})"),
        ));
        Some(id)
    }

    fn local_variable_id(&mut self, name: &str, ty: LType) -> Option<usize> {
        let scope = self.current_scope?;
        let position = self.current_position?;
        let id = self.alloc_id();
        let line = position.line.max(1);
        let type_id = self.debug_type_id(ty);
        let name = metadata_escape(name);
        self.dynamic.push((
            id,
            format!(
                "!{id} = !DILocalVariable(name: \"{name}\", scope: !{scope}, file: !{}, line: {line}, type: !{type_id})",
                self.file_id
            ),
        ));
        self.current_local_ids.push(id);
        Some(id)
    }

    fn debug_type_id(&self, ty: LType) -> usize {
        match ty {
            LType::I64 => self.i64_type_id,
            LType::I32 => self.i32_type_id,
            LType::I1 => self.bool_type_id,
            LType::Double => self.double_type_id,
            LType::Str | LType::Ptr => self.ptr_type_id,
        }
    }

    fn metadata_lines(&self) -> Vec<String> {
        let file = metadata_escape(&self.source.filename);
        let dir = metadata_escape(&self.source.directory);
        let mut out = vec![
            format!(
                "!{} = !DIFile(filename: \"{file}\", directory: \"{dir}\")",
                self.file_id
            ),
            format!(
                "!{} = distinct !DICompileUnit(language: DW_LANG_C, file: !{}, producer: \"osprey\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)",
                self.cu_id, self.file_id
            ),
            format!("!{} = !{{}}", self.empty_id),
            format!(
                "!{} = !DISubroutineType(types: !{})",
                self.subroutine_type_id, self.empty_id
            ),
            format!(
                "!{} = !{{i32 2, !\"Dwarf Version\", i32 {}}}",
                self.dwarf_flag_id, self.dwarf_version
            ),
            format!(
                "!{} = !{{i32 2, !\"Debug Info Version\", i32 3}}",
                self.debug_version_flag_id
            ),
            format!("!{} = !{{!\"osprey\"}}", self.ident_id),
            format!(
                "!{} = !DIBasicType(name: \"int\", size: 64, encoding: DW_ATE_signed)",
                self.i64_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"c_int\", size: 32, encoding: DW_ATE_signed)",
                self.i32_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"bool\", size: 1, encoding: DW_ATE_boolean)",
                self.bool_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"float\", size: 64, encoding: DW_ATE_float)",
                self.double_type_id
            ),
            format!(
                "!{} = !DIBasicType(name: \"char\", size: 8, encoding: DW_ATE_signed_char)",
                self.char_type_id
            ),
            format!(
                "!{} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{}, size: 64)",
                self.ptr_type_id, self.char_type_id
            ),
        ];
        out.extend(self.dynamic.iter().map(|(_, line)| line.clone()));
        out
    }

    fn module_flags(&self) -> String {
        format!(
            "!llvm.module.flags = !{{!{}, !{}}}",
            self.dwarf_flag_id, self.debug_version_flag_id
        )
    }

    fn compile_units(&self) -> String {
        format!("!llvm.dbg.cu = !{{!{}}}", self.cu_id)
    }

    fn ident(&self) -> String {
        format!("!llvm.ident = !{{!{}}}", self.ident_id)
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Codegen {
    pub fn new() -> Codegen {
        Codegen::with_types(ProgramTypes::default())
    }

    /// Build with the inferred program types that drive parameter/return/value
    /// typing.
    pub fn with_types(prog: ProgramTypes) -> Codegen {
        Codegen::with_options(prog, CodegenOptions::default())
    }

    /// Build with inferred program types and explicit code generation options.
    pub fn with_options(prog: ProgramTypes, options: CodegenOptions) -> Codegen {
        Codegen {
            externs: BTreeSet::new(),
            globals: Vec::new(),
            funcs: Vec::new(),
            glob_count: 0,
            reg_count: 0,
            label_count: 0,
            cur_lines: Vec::new(),
            cur_block: String::from("entry"),
            scopes: Vec::new(),
            fn_params: HashMap::new(),
            prog,
            pending_iter_ops: Vec::new(),
            lambdas: HashMap::new(),
            fnval_cells: HashMap::new(),
            effect_ops: HashMap::new(),
            handler_count: 0,
            lambda_count: 0,
            obj_layouts: HashMap::new(),
            obj_count: 0,
            fn_defs: HashMap::new(),
            inlining: HashSet::new(),
            fn_ptr_locals: HashMap::new(),
            fn_value_types: HashMap::new(),
            call_aliases: HashMap::new(),
            cell_vars: HashSet::new(),
            cell_slots: HashMap::new(),
            resume_ctx: None,
            testing_used: false,
            debug: options.debug_source.map(DebugState::new),
            coverage: options
                .coverage
                .then(crate::coverage::CoverageState::default),
            arc: crate::arc::ArcLedger::new(),
            arc_slot_count: 0,
        }
    }

    /// Register a function-typed local: its lowered signature for indirect
    /// calls plus its full [`Type`] for chained applications.
    pub(crate) fn bind_fn_local(&mut self, name: &str, ty: Type) {
        if let Some(sig) = Codegen::fn_value_sig(&ty) {
            let _ = self.fn_ptr_locals.insert(name.to_string(), sig);
            let _ = self.fn_value_types.insert(name.to_string(), ty);
        }
    }

    /// The function type of the value a call to `f` returns, when `f` is a
    /// top-level function or a function-typed local that returns a function.
    pub(crate) fn call_result_fn_type(&self, f: &str) -> Option<Type> {
        let ret = self
            .prog
            .return_type(f)
            .or_else(|| match self.fn_value_types.get(f) {
                Some(Type::Fun { ret, .. }) => Some(&**ret),
                _ => None,
            })?;
        match ret {
            Type::Fun { .. } => Some(ret.clone()),
            _ => None,
        }
    }

    /// The function [`Type`] an expression evaluates to in callee/callback
    /// position — `None` when it is not (statically) a function value. Powers
    /// higher-order calls through arbitrary callee expressions: a chained
    /// application (`add3(1)(2)(3)`), a function held in a record field
    /// (`cfg.processor`), or a function-typed local.
    pub(crate) fn callee_fn_type(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Identifier(name) => self.identifier_fn_type(name),
            // A call evaluates to its callee's return type — recurse so a chain
            // peels one arrow per application.
            Expr::Call { function, .. } => match self.callee_fn_type(function)? {
                Type::Fun { ret, .. } => Some(*ret),
                _ => None,
            },
            Expr::FieldAccess { target, field } => self.field_fn_type(target, field),
            _ => None,
        }
    }

    /// The function type of a named callee: a function-typed local first (its
    /// inferred value type), else a top-level function's resolved signature.
    fn identifier_fn_type(&self, name: &str) -> Option<Type> {
        if let Some(t) = self.fn_value_types.get(name) {
            return Some(t.clone());
        }
        let (params, ret) = self.prog.functions.get(name)?;
        Some(Type::Fun {
            params: params.clone(),
            ret: Box::new(ret.clone()),
        })
    }

    /// The type of `target.field` when `field` names a function-valued record
    /// field — resolving `target`'s owner from the bound value's type tag, or a
    /// unique field-name match across known layouts.
    fn field_fn_type(&self, target: &Expr, field: &str) -> Option<Type> {
        let owner = self.callee_field_owner(target, field)?;
        self.prog
            .ctors
            .get(&owner)?
            .fields
            .iter()
            .find(|(f, _)| f == field)
            .map(|(_, t)| t.clone())
    }

    /// Resolve the owner type of `target.field`: prefer a bound identifier's
    /// static type tag, else the unique constructor declaring `field`.
    fn callee_field_owner(&self, target: &Expr, field: &str) -> Option<String> {
        if let Expr::Identifier(name) = target {
            let tagged = self.lookup(name).and_then(|v| v.osp_ty);
            if let Some(owner) = tagged {
                if self.declares_field(&owner, field) {
                    return Some(owner);
                }
            }
        }
        self.find_field_owner(field)
    }

    /// Whether `owner`'s layout declares `field`.
    fn declares_field(&self, owner: &str, field: &str) -> bool {
        self.prog
            .ctors
            .get(owner)
            .is_some_and(|c| c.fields.iter().any(|(f, _)| f == field))
    }

    /// Whether `name` is a user function whose inferred signature still contains
    /// a type variable (in a parameter or the return) — i.e. it is polymorphic
    /// and must be specialised to the concrete call-site types.
    pub(crate) fn is_generic_fn(&self, name: &str) -> bool {
        let Some((params, ret)) = self.prog.functions.get(name) else {
            return false;
        };
        params
            .iter()
            .chain(std::iter::once(ret))
            .any(osprey_types::has_type_var)
    }

    /// The declared type parameters of a constructor's owner (`["T"]` for
    /// `Generic<T>`), used to spot a generic field whose LLVM type is fixed per
    /// construction rather than by the (placeholder) written type.
    pub(crate) fn ctor_type_params(&self, name: &str) -> Vec<String> {
        self.prog
            .ctors
            .get(name)
            .map(|c| c.type_params.clone())
            .unwrap_or_default()
    }

    /// The lowered [`FnSig`] of a function-typed value `ty`, for the closure
    /// ABI — `None` if `ty` is not a function. The return is normalized via
    /// [`crate::types::normalize_fn_ret`] so maker and consumer derive the
    /// same ABI even when the assignable-unwrap rule let their types differ by
    /// a Result wrapper.
    pub(crate) fn fn_value_sig(ty: &Type) -> Option<FnSig> {
        match ty {
            Type::Fun { params, ret } => {
                let ret = crate::types::normalize_fn_ret(ret);
                Some((
                    params.iter().map(ltype_of).collect(),
                    ltype_of(ret),
                    crate::types::result_inner(ret),
                ))
            }
            _ => None,
        }
    }

    /// Register an anonymous object literal's ordered field layout and return the
    /// synthetic owner name to tag its handle with.
    pub(crate) fn register_obj_layout(&mut self, fields: Vec<(String, LType)>) -> String {
        let name = format!("__obj_{}", self.obj_count);
        self.obj_count += 1;
        let _ = self.obj_layouts.insert(name.clone(), fields);
        name
    }

    /// The struct spelling and ordered fields of an owner — a real constructor or
    /// a synthetic object literal — for unified field access.
    pub(crate) fn record_layout(&self, owner: &str) -> Option<(String, Vec<(String, LType)>)> {
        if let Some(fields) = self.obj_layouts.get(owner) {
            let mut parts = vec!["i64".to_string()];
            parts.extend(fields.iter().map(|(_, lt)| lt.as_str().to_string()));
            return Some((format!("{{ {} }}", parts.join(", ")), fields.clone()));
        }
        let view = self.ctor_layout(owner)?;
        Some((self.ctor_struct_ty(owner)?, view.fields))
    }

    /// A fresh, module-unique handler-function id.
    pub(crate) fn next_handler_id(&mut self) -> usize {
        let id = self.handler_count;
        self.handler_count += 1;
        id
    }

    /// A fresh, module-unique id for a lifted lambda's function name.
    pub(crate) fn next_lambda_id(&mut self) -> usize {
        let id = self.lambda_count;
        self.lambda_count += 1;
        id
    }

    /// Append a module-level global definition (e.g. the fiber-result table).
    pub(crate) fn add_global_def(&mut self, def: impl Into<String>) {
        self.globals.push(def.into());
    }

    /// Suspend the in-progress function and start a fresh one (a handler function
    /// emitted while lowering its enclosing `handle`). Returns the saved state to
    /// hand back to [`Codegen::exit_nested_fn`]. The new function gets its own
    /// SSA/label counters and an isolated scope stack (handlers capture nothing).
    pub(crate) fn enter_nested_fn(&mut self) -> SavedFn {
        let saved = SavedFn {
            lines: std::mem::take(&mut self.cur_lines),
            block: std::mem::replace(&mut self.cur_block, String::from("entry")),
            regs: self.reg_count,
            labels: self.label_count,
            scopes: std::mem::take(&mut self.scopes),
            pending_iter_ops: std::mem::take(&mut self.pending_iter_ops),
            cell_vars: std::mem::take(&mut self.cell_vars),
            cell_slots: std::mem::take(&mut self.cell_slots),
            resume_ctx: self.resume_ctx.take(),
            arc: std::mem::take(&mut self.arc),
            arc_slot_count: self.arc_slot_count,
            debug_scope: self.debug.as_ref().and_then(|d| d.current_scope),
            debug_position: self.debug.as_ref().and_then(|d| d.current_position),
            debug_retained_nodes: self.debug.as_ref().and_then(|d| d.current_retained_nodes),
            debug_local_ids: self
                .debug
                .as_ref()
                .map(|d| d.current_local_ids.clone())
                .unwrap_or_default(),
        };
        if let Some(debug) = self.debug.as_mut() {
            debug.clear_function();
        }
        self.reg_count = 0;
        self.label_count = 0;
        self.cur_lines = vec!["entry:".to_string()];
        self.scopes = vec![HashMap::new()];
        self.arc_slot_count = 0;
        saved
    }

    /// Finish the nested function (append it) and resume the suspended one.
    pub(crate) fn exit_nested_fn(
        &mut self,
        saved: SavedFn,
        ret: &str,
        name: &str,
        params: &[(LType, String)],
    ) {
        self.finish_function(ret, name, params);
        self.cur_lines = saved.lines;
        self.cur_block = saved.block;
        self.reg_count = saved.regs;
        self.label_count = saved.labels;
        self.scopes = saved.scopes;
        self.pending_iter_ops = saved.pending_iter_ops;
        self.cell_vars = saved.cell_vars;
        self.cell_slots = saved.cell_slots;
        self.resume_ctx = saved.resume_ctx;
        self.arc = saved.arc;
        self.arc_slot_count = saved.arc_slot_count;
        if let Some(debug) = self.debug.as_mut() {
            debug.current_scope = saved.debug_scope;
            debug.current_position = saved.debug_position;
            debug.current_retained_nodes = saved.debug_retained_nodes;
            debug.current_local_ids = saved.debug_local_ids;
        }
    }

    /// Register an `effect` operation's parsed signature for `handle`/`perform`.
    pub(crate) fn register_effect_op(&mut self, key: String, sig: crate::effects::OpSig) {
        let _ = self.effect_ops.insert(key, sig);
    }

    /// The parsed signature of `Effect.operation`, if declared.
    pub(crate) fn effect_op(&self, key: &str) -> Option<crate::effects::OpSig> {
        self.effect_ops.get(key).cloned()
    }

    // ---- inferred typing ----

    /// The LLVM return type of a user/runtime function, from inference.
    pub(crate) fn fn_ret_ltype(&self, name: &str) -> Option<LType> {
        self.prog.return_type(name).map(ltype_of)
    }

    /// The LLVM parameter types of a user function, from inference.
    pub(crate) fn fn_param_ltypes(&self, name: &str) -> Option<Vec<LType>> {
        self.prog
            .param_types(name)
            .map(|ps| ps.iter().map(ltype_of).collect())
    }

    /// The `(LType, owner)` parameter signature — `owner` tags record/union
    /// parameters so their fields are reachable inside the body.
    pub(crate) fn fn_param_sig(&self, name: &str) -> Option<Vec<(LType, Option<String>)>> {
        self.prog.param_types(name).map(|ps| {
            ps.iter()
                .map(|t| (ltype_of(t), crate::types::owner_name(t)))
                .collect()
        })
    }

    /// The owner type name of a function's return value, if it is a record/union.
    pub(crate) fn fn_ret_owner(&self, name: &str) -> Option<String> {
        self.prog
            .return_type(name)
            .and_then(crate::types::owner_name)
    }

    /// The inner [`LType`] when a function is declared to return `Result<T, E>`
    /// — the success payload's LLVM type — so calls and returns carry the
    /// `{ T, i8 }*` Result block rather than a bare `T`.
    pub(crate) fn fn_ret_result_inner(&self, name: &str) -> Option<LType> {
        crate::types::result_inner(self.prog.return_type(name)?)
    }

    /// The LLVM spelling of `name`'s emitted return slot (Result block or
    /// scalar) — the type its `define`/`call` lines actually carry.
    pub(crate) fn fn_ret_spelling(&self, name: &str) -> String {
        crate::llty::ret_spelling(
            self.fn_ret_ltype(name).unwrap_or(LType::I64),
            self.fn_ret_result_inner(name),
        )
    }

    /// The full heap layout of a constructor: owning type, whether it is a
    /// record, the discriminant tag (variant index within its union; 0 for a
    /// record), and ordered `(field, LType)` pairs.
    pub(crate) fn ctor_layout(&self, name: &str) -> Option<CtorView> {
        let c = self.prog.ctors.get(name)?;
        let tag = i64::try_from(
            self.prog
                .unions
                .get(&c.owner)
                .and_then(|vs| vs.iter().position(|v| v == name))
                .unwrap_or(0),
        )
        .unwrap_or(0);
        let fields = c
            .fields
            .iter()
            .map(|(f, t)| (f.clone(), ltype_of(t)))
            .collect();
        Some(CtorView {
            owner: c.owner.clone(),
            owner_is_record: c.owner_is_record,
            tag,
            fields,
        })
    }

    /// Resolve a field name to an owning constructor when the target's static
    /// type is unknown — the polymorphic field-access fallback for a generic
    /// accessor like `fn getFirst(p) = p.first`, where `p` infers to a type
    /// variable. Prefers a layout whose field type is a concrete scalar (so the
    /// load type and `toString` match the runtime value), breaking ties by
    /// owner name for deterministic output.
    pub(crate) fn find_field_owner(&self, field: &str) -> Option<String> {
        let mut candidates: Vec<(&String, LType)> = self
            .prog
            .ctors
            .iter()
            .filter_map(|(name, c)| {
                c.fields
                    .iter()
                    .find(|(f, _)| f == field)
                    .map(|(_, t)| (name, ltype_of(t)))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(b.0));
        candidates
            .iter()
            .find(|(_, lt)| *lt != LType::Ptr)
            .or_else(|| candidates.first())
            .map(|(name, _)| (*name).clone())
    }

    /// The LLVM struct spelling for a constructor's heap block: `{ i64, f0, … }`
    /// — a leading `i64` discriminant tag followed by each field's LLVM type.
    pub(crate) fn ctor_struct_ty(&self, name: &str) -> Option<String> {
        let view = self.ctor_layout(name)?;
        let mut parts = vec!["i64".to_string()];
        for (_, lt) in &view.fields {
            parts.push(lt.as_str().to_string());
        }
        Some(format!("{{ {} }}", parts.join(", ")))
    }

    /// Whether a name is a known constructor.
    pub(crate) fn is_ctor(&self, name: &str) -> bool {
        self.prog.ctors.contains_key(name)
    }

    /// The owner type name to tag a loaded aggregate field with: the field's
    /// resolved type when that type is itself a known record/union, else `None`
    /// (scalars carry no owner).
    pub(crate) fn ctor_field_owner(&self, owner: &str, field: &str) -> Option<String> {
        let ty = self
            .prog
            .ctors
            .get(owner)?
            .fields
            .iter()
            .find(|(f, _)| f == field)
            .map(|(_, t)| t.clone())?;
        let head = crate::types::owner_name(&ty)?;
        if self.prog.ctors.contains_key(&head) || self.prog.unions.contains_key(&head) {
            Some(head)
        } else {
            None
        }
    }

    /// The variant constructor names of a union owner, in tag order.
    pub(crate) fn union_variants(&self, owner: &str) -> Option<&[String]> {
        self.prog.unions.get(owner).map(std::vec::Vec::as_slice)
    }

    // ---- SSA + block naming (function-local) ----

    pub(crate) fn fresh_reg(&mut self) -> String {
        let r = format!("%r{}", self.reg_count);
        self.reg_count += 1;
        r
    }

    pub(crate) fn fresh_label(&mut self) -> String {
        let l = format!("L{}", self.label_count);
        self.label_count += 1;
        l
    }

    pub(crate) fn cur_block(&self) -> &str {
        &self.cur_block
    }

    // ---- emission ----

    pub(crate) fn emit(&mut self, line: impl Into<String>) {
        let mut line = line.into();
        if let Some(id) = self.debug.as_mut().and_then(DebugState::location_id) {
            let _ = write!(line, ", !dbg !{id}");
        }
        self.cur_lines.push(format!("  {line}"));
    }

    /// Emit `r = {rhs}` to a fresh SSA register and return `r` — the ubiquitous
    /// "name the result of one instruction" step (`zext …`, `icmp …`, `fneg …`).
    pub(crate) fn emit_reg(&mut self, rhs: impl std::fmt::Display) -> String {
        let r = self.fresh_reg();
        self.emit(format!("{r} = {rhs}"));
        r
    }

    /// The `DILocalVariable` metadata id for `name` of type `ty`, if a debug
    /// build is active. The single lookup both debug-recorders funnel through.
    fn debug_var_id(&mut self, name: &str, ty: LType) -> Option<usize> {
        self.debug
            .as_mut()
            .and_then(|debug| debug.local_variable_id(name, ty))
    }

    /// Record a source-level **parameter** for native debuggers via
    /// `llvm.dbg.value`. [DEBUGGER-DBG-DECLARE]
    ///
    /// A parameter is an SSA argument live for the whole function, so dbg.value
    /// (no stack slot) is correct and immediately readable — including by a
    /// conditional breakpoint on the function's first line. Emitted at function
    /// entry inside the prologue, it never produces the inter-statement line-0
    /// row that an inline `let` dbg.value would.
    pub(crate) fn emit_debug_param(&mut self, name: &str, value: &Value) {
        let Some(var_id) = self.debug_var_id(name, value.ty) else {
            return;
        };
        self.add_extern("declare void @llvm.dbg.value(metadata, metadata, metadata)");
        self.emit(format!(
            "call void @llvm.dbg.value(metadata {} {}, metadata !{var_id}, metadata !DIExpression())",
            value.ty.as_str(),
            value.operand
        ));
    }

    /// Record a source-level **local** (`let` binding) for native debuggers via
    /// `llvm.dbg.declare` over a dedicated stack slot. [DEBUGGER-DBG-DECLARE]
    ///
    /// dbg.declare is the robust -O0 representation for an addressable local:
    /// lldb reads it from the slot at every PC in scope. An inline `dbg.value`
    /// would instead lower to a `DBG_VALUE` whose line-table row is line 0;
    /// between two statements that becomes a stray line-0 entry that derails
    /// `x86_64` lldb-dap breakpoint line resolution (a frame reports `line 0`, so
    /// a breakpoint "stops on line 0"). The slot is debug-only — codegen keeps
    /// using `value.operand`, and Osprey `let` bindings are immutable, so the
    /// once-written slot stays correct.
    pub(crate) fn emit_debug_local(&mut self, name: &str, value: &Value) {
        let Some(var_id) = self.debug_var_id(name, value.ty) else {
            return;
        };
        self.add_extern("declare void @llvm.dbg.declare(metadata, metadata, metadata)");
        let ty = value.ty.as_str();
        let slot = self.fresh_reg();
        self.emit(format!("{slot} = alloca {ty}"));
        self.emit(format!("store {ty} {}, {ty}* {slot}", value.operand));
        self.emit(format!(
            "call void @llvm.dbg.declare(metadata {ty}* {slot}, metadata !{var_id}, metadata !DIExpression())"
        ));
    }

    /// Start a new basic block and make it current (its label becomes the
    /// predecessor recorded for any `phi` that follows).
    pub(crate) fn start_block(&mut self, label: &str) {
        self.cur_lines.push(format!("{label}:"));
        self.cur_block = label.to_string();
    }

    /// Snapshot the current block label, then branch to `end` — the predecessor
    /// a `phi` at `end` reads back. Closes a one-arm path of a Result/match split.
    pub(crate) fn snapshot_to(&mut self, end: &str) -> String {
        let block = self.cur_block.clone();
        self.emit(format!("br label %{end}"));
        block
    }

    pub(crate) fn add_extern(&mut self, decl: impl Into<String>) {
        let _ = self.externs.insert(decl.into());
    }

    /// Append a module-level global definition (counter globals, tables).
    pub(crate) fn add_global(&mut self, def: impl Into<String>) {
        self.globals.push(def.into());
    }

    /// Intern a string literal as a private global and return an `i8*` pointing
    /// at its first byte.
    pub(crate) fn string_constant(&mut self, text: &str) -> Value {
        let (escaped, len) = escape_c_string(text);
        let name = format!("@.str.{}", self.glob_count);
        self.glob_count += 1;
        self.globals.push(format!(
            "{name} = private unnamed_addr constant [{len} x i8] c\"{escaped}\""
        ));
        let reg = self.fresh_reg();
        self.emit(format!(
            "{reg} = getelementptr [{len} x i8], [{len} x i8]* {name}, i64 0, i64 0"
        ));
        Value::new(reg, LType::Str)
    }

    // ---- scopes ----

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        let _ = self.scopes.pop();
    }

    pub(crate) fn bind(&mut self, name: impl Into<String>, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.insert(name.into(), value);
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Value> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
    }

    /// Read a cell-backed variable: `load` its current value from the heap slot.
    /// `None` when `name` is not promoted to a cell (the caller falls back to a
    /// normal scope lookup).
    pub(crate) fn cell_read(&mut self, name: &str) -> Option<Value> {
        let slot = self.cell_slots.get(name).cloned()?;
        let ty = slot.pointee.as_str();
        let r = self.emit_reg(format!("load {ty}, {ty}* {}", slot.ptr));
        Some(Value::new(r, slot.pointee).with_owner(slot.osp_ty))
    }

    // ---- function framing ----

    /// Reset per-function state and open a fresh `entry` block + scope.
    pub(crate) fn begin_function(&mut self, name: &str, position: Option<Position>) {
        self.reg_count = 0;
        self.label_count = 0;
        self.cur_lines.clear();
        self.cur_block = String::from("entry");
        self.fn_ptr_locals.clear();
        self.fn_value_types.clear();
        // The beta-reduction cache is per-function too: a stale entry from an
        // earlier function must not hijack a same-named local here.
        self.lambdas.clear();
        // Cell-promotion is per-function; `lower` repopulates `cell_vars` from
        // this function's body before lowering it.
        self.cell_vars.clear();
        self.cell_slots.clear();
        self.resume_ctx = None;
        self.arc = crate::arc::ArcLedger::new();
        self.arc_slot_count = 0;
        self.push_scope();
        self.cur_lines.push("entry:".to_string());
        if let Some(debug) = self.debug.as_mut() {
            let _ = debug.begin_function(name, position);
        }
    }

    /// Render the in-progress function and append it to the module. `ret` is the
    /// already-rendered LLVM return type (`i64`, `{ i1, i8 }*`, …).
    pub(crate) fn finish_function(&mut self, ret: &str, name: &str, params: &[(LType, String)]) {
        let param_list = params
            .iter()
            .map(|(ty, n)| format!("{ty} %{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let body = std::mem::take(&mut self.cur_lines).join("\n");
        let dbg = self
            .debug
            .as_ref()
            .and_then(|d| d.current_scope)
            .map_or_else(String::new, |id| format!(" !dbg !{id}"));
        self.funcs.push(format!(
            "define {ret} @{name}({param_list}) #0{dbg} {{\n{body}\n}}"
        ));
        self.pop_scope();
        if let Some(debug) = self.debug.as_mut() {
            debug.finish_function();
            debug.clear_function();
        }
    }

    /// Assemble the final module text: header, externals, globals, functions.
    pub(crate) fn render(&self) -> String {
        let mut out = String::from("; Generated by osprey-rs (Rust LLVM-text backend)\n\n");
        if let Some(debug) = &self.debug {
            let _ = write!(out, "source_filename = \"{}\"\n\n", debug.source_filename());
        }
        for decl in &self.externs {
            out.push_str(decl);
            out.push('\n');
        }
        out.push('\n');
        for g in &self.globals {
            out.push_str(g);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&self.funcs.join("\n\n"));
        out.push('\n');
        if let Some(init) = self.cov_render_init() {
            out.push('\n');
            out.push_str(&init);
            out.push('\n');
        }
        out.push_str(FRAME_POINTER_ATTRS);
        out.push('\n');
        if let Some(debug) = &self.debug {
            out.push('\n');
            out.push_str(&debug.compile_units());
            out.push('\n');
            out.push_str(&debug.module_flags());
            out.push('\n');
            out.push_str(&debug.ident());
            out.push('\n');
            for line in debug.metadata_lines() {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Allocate `size` bytes through the swappable Osprey allocation hook and
    /// return the raw `i8*` register. The single heap-allocation primitive every
    /// codegen site funnels through, so the memory backend is chosen at link time
    /// (default `osp_alloc` = `malloc`; ARC / tracing-GC / arena swap in behind
    /// the same symbol) — never baked into the IR. Implements [MEM-BACKENDS],
    /// docs/specs/0018. The allocator attributes are load-bearing: they let LLVM
    /// recognise `@osp_alloc` as an allocation function, so `-O2` proves
    /// non-escaping allocations dead and removes them entirely (the
    /// [MEM-OWNERSHIP] static free-at-last-use, achieved by the optimizer).
    pub(crate) fn heap_alloc(&mut self, size: &str) -> String {
        self.add_extern(OSP_ALLOC_DECL);
        self.emit_reg(format!("call i8* @osp_alloc(i64 {size})"))
    }

    /// [`heap_alloc`] carrying the per-site layout word (kind + managed-pointer
    /// mask, see [`crate::meta`]): the ARC backend stores it in the object
    /// header so `osp_release` can drop children precisely; other backends
    /// ignore it. Implements [GC-ARC-PERCEUS], docs/plans/0011 phase 2.
    pub(crate) fn heap_alloc_tagged(&mut self, size: &str, meta: i64) -> String {
        if meta == crate::meta::KIND_RAW {
            return self.heap_alloc(size);
        }
        self.add_extern(OSP_ALLOC_TAGGED_DECL);
        self.emit_reg(format!(
            "call i8* @osp_alloc_tagged(i64 {size}, i64 {meta})"
        ))
    }

    /// Allocate a heap block sized for the LLVM struct type `struct_ty`, via the
    /// portable `getelementptr null, 1` sizeof trick, and return the typed
    /// pointer register (`{TY}*`). `meta` is the site's layout word
    /// ([`crate::meta`]).
    pub(crate) fn malloc_struct(&mut self, struct_ty: &str, meta: i64) -> String {
        let szp = self.fresh_reg();
        self.emit(format!(
            "{szp} = getelementptr {struct_ty}, {struct_ty}* null, i64 1"
        ));
        let sz = self.fresh_reg();
        self.emit(format!("{sz} = ptrtoint {struct_ty}* {szp} to i64"));
        let raw = self.heap_alloc_tagged(&sz, meta);
        let obj = self.fresh_reg();
        self.emit(format!("{obj} = bitcast i8* {raw} to {struct_ty}*"));
        obj
    }

    /// Hoist a null-initialized `i8*` spill slot into the entry block and
    /// return its register. ARC region drops load these slots, so a drop is
    /// valid from any block and untaken paths release `null` (a no-op) —
    /// ownership without dominance analysis [GC-ARC-PERCEUS].
    pub(crate) fn hoist_arc_slot(&mut self) -> String {
        let name = format!("%arc.s{}", self.arc_slot_count);
        self.arc_slot_count += 1;
        self.cur_lines.insert(1, format!("  {name} = alloca i8*"));
        self.cur_lines
            .insert(2, format!("  store i8* null, i8** {name}"));
        name
    }

    /// Set the current debug source position, returning the previous position.
    pub(crate) fn set_debug_position(&mut self, position: Option<Position>) -> Option<Position> {
        self.debug
            .as_mut()
            .and_then(|debug| debug.set_position(position))
    }

    /// Restore the debug source position captured by [`set_debug_position`].
    pub(crate) fn restore_debug_position(&mut self, previous: Option<Position>) {
        if let Some(debug) = self.debug.as_mut() {
            debug.current_position = previous;
        }
    }
}

/// Attribute group applied to every generated function: keep frame pointers in
/// ALL functions (Darwin's default drops them in leaves), so the profiler's
/// async frame-pointer chain walk is valid from any sample point. Implements
/// [PROF-CODEGEN-FP], docs/specs/0028-Profiler.md; cost is ~1% (arm64 reserves
/// x29 for the frame chain by ABI anyway).
pub(crate) const FRAME_POINTER_ATTRS: &str = "attributes #0 = { \"frame-pointer\"=\"all\" }";

/// The swappable allocation hook declaration. `noalias` + the allocator
/// attributes (`allocsize`/`allockind`/`alloc-family`) make LLVM treat
/// `@osp_alloc` as an allocation function for dead-allocation elimination, while
/// a custom `alloc-family` ("osprey", not "malloc") stops LLVM rewriting it to
/// libc `calloc`/`realloc` and bypassing the backend. Implements [MEM-BACKENDS].
pub(crate) const OSP_ALLOC_DECL: &str = "declare noalias i8* @osp_alloc(i64) allocsize(0) allockind(\"alloc,uninitialized\") mustprogress nounwind willreturn \"alloc-family\"=\"osprey\"";

/// The layout-carrying twin of [`OSP_ALLOC_DECL`]: same allocator attributes
/// (so `-O2` dead-allocation elimination still applies), plus the meta word the
/// ARC backend stores in the object header. Implements [GC-ARC-PERCEUS].
pub(crate) const OSP_ALLOC_TAGGED_DECL: &str = "declare noalias i8* @osp_alloc_tagged(i64, i64) allocsize(0) allockind(\"alloc,uninitialized\") mustprogress nounwind willreturn \"alloc-family\"=\"osprey\"";

/// The resolved heap layout of a constructor.
pub(crate) struct CtorView {
    pub owner: String,
    pub owner_is_record: bool,
    pub tag: i64,
    pub fields: Vec<(String, LType)>,
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
