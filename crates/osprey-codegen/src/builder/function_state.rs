//! Emitter function state.
use super::{Codegen, CodegenError, LType, Position, Result, SavedFn, MAX_OPERATION_IDS};

impl Codegen {
    /// A fresh, module-unique handler-function id.
    pub(crate) fn next_handler_id(&mut self) -> usize {
        let id = self.handler_count;
        self.handler_count += 1;
        id
    }

    /// The operation id `(effect_key, operation)` is interned to, allocating
    /// the next dense id on first sight. Errors once the runtime's evidence
    /// table (`OSP_MAX_OPERATION_IDS` in `effects_runtime.h`) would overflow.
    pub(crate) fn operation_id(&mut self, effect_key: &str, operation: &str) -> Result<u32> {
        let key = (effect_key.to_string(), operation.to_string());
        if let Some(id) = self.operation_ids.get(&key) {
            return Ok(*id);
        }
        let id = u32::try_from(self.operation_ids.len()).unwrap_or(u32::MAX);
        if id >= MAX_OPERATION_IDS {
            return Err(CodegenError::invalid(format!(
                "program performs more than {MAX_OPERATION_IDS} distinct effect operations"
            )));
        }
        let _ = self.operation_ids.insert(key, id);
        Ok(id)
    }

    /// A fresh id for an environment alias. Advanced only by environment
    /// capture, so a Default/ML twin pair numbers alike [FLAVOR-IR-EQUIV].
    pub(crate) fn next_env_id(&mut self) -> usize {
        let id = self.env_count;
        self.env_count += 1;
        id
    }

    /// A fresh, module-unique id for a lifted lambda's function name.
    pub(crate) fn next_lambda_id(&mut self) -> usize {
        let id = self.lambda_count;
        self.lambda_count += 1;
        id
    }

    /// The kernel lowering this module compiles under [GPU-KERNEL-EXTRACT].
    pub(crate) fn gpu_kernels(&self) -> crate::gpu_kernel::GpuKernelMode {
        self.gpu_kernels
    }

    /// A fresh id naming an extracted GPU kernel. Advanced ONLY by extraction,
    /// so it is a pure function of AST walk order and a Default/ML twin pair
    /// numbers its kernels identically [FLAVOR-IR-EQUIV].
    pub(crate) fn next_kernel_id(&mut self) -> usize {
        let id = self.kernel_count;
        self.kernel_count += 1;
        id
    }

    /// A fresh id naming an emitted instantiation of a recursive generic
    /// function. Advanced only by specialisation, so it is a pure function of
    /// AST walk order and a Default/ML twin pair numbers alike [FLAVOR-IR-EQUIV].
    pub(crate) fn next_monofn_id(&mut self) -> usize {
        let id = self.monofn_count;
        self.monofn_count += 1;
        id
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
            expected_lambda: self.expected_lambda.take(),
            scope_ids: std::mem::take(&mut self.scope_ids),
            iter_stages: std::mem::take(&mut self.iter_stages),
            cell_vars: std::mem::take(&mut self.cell_vars),
            cell_slots: std::mem::take(&mut self.cell_slots),
            resume_ctx: self.resume_ctx.take(),
            arc: std::mem::take(&mut self.arc),
            arc_slot_count: self.arc_slot_count,
            rodata_regs: std::mem::take(&mut self.rodata_regs),
            debug_scope: self.debug.as_ref().and_then(|d| d.current_scope),
            debug_function: self.debug.as_ref().and_then(|d| d.current_function),
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
        self.push_scope();
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
        self.scope_ids = saved.scope_ids;
        self.expected_lambda = saved.expected_lambda;
        self.iter_stages = saved.iter_stages;
        self.cell_vars = saved.cell_vars;
        self.cell_slots = saved.cell_slots;
        self.resume_ctx = saved.resume_ctx;
        self.arc = saved.arc;
        self.arc_slot_count = saved.arc_slot_count;
        self.rodata_regs = saved.rodata_regs;
        if let Some(debug) = self.debug.as_mut() {
            debug.current_scope = saved.debug_scope;
            debug.current_function = saved.debug_function;
            debug.current_position = saved.debug_position;
            debug.current_retained_nodes = saved.debug_retained_nodes;
            debug.current_local_ids = saved.debug_local_ids;
        }
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
        self.lambda_envs.clear();
        self.iter_stages.clear();
        // Cell-promotion is per-function; `lower` repopulates `cell_vars` from
        // this function's body before lowering it.
        self.cell_vars.clear();
        self.cell_slots.clear();
        self.resume_ctx = None;
        self.arc = crate::arc::ArcLedger::new();
        self.arc_slot_count = 0;
        // Register names restart here, so a previous function's rodata
        // registers would otherwise alias this one's heap values and silently
        // elide their dup/drop [GC-ARC-PERCEUS].
        self.rodata_regs.clear();
        self.push_scope();
        self.cur_lines.push("entry:".to_string());
        if let Some(debug) = self.debug.as_mut() {
            let _ = debug.begin_function(name, position);
        }
    }

    /// Open a debug scope for a nested function whose body is user-written code.
    ///
    /// [`enter_nested_fn`](Self::enter_nested_fn) clears the debug scope, and for
    /// the synthetic trampolines (suspend, drive, fn-value forwarders) that is
    /// what we want — a `DISubprogram` there would surface compiler-invented
    /// frames the author never wrote. A handler arm is the opposite case: its
    /// body is source someone can set a breakpoint in. Without a scope,
    /// `location_id` returns `None` for every instruction in it, so the arm's
    /// lines never reach the line table and a breakpoint there can never bind —
    /// the arm runs but the debugger sails past it. [DEBUGGER-DBG-DECLARE]
    pub(crate) fn begin_nested_debug(&mut self, name: &str, position: Option<Position>) {
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
            .and_then(|d| d.current_function)
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
}
