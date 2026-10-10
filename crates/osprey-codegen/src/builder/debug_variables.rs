//! Source-variable storage for native debuggers. [DEBUGGER-DBG-DECLARE]
use super::{CellSlot, Codegen, DebugState};
use crate::llty::Value;

impl Codegen {
    /// A declaration becomes visible only after its initializer and debug store.
    /// Assignments and capture reloads retain their existing lexical scopes.
    /// Implements [DEBUGGER-BINDING-LIFETIME].
    pub(crate) fn emit_debug_binding(&mut self, name: &str, value: &Value, declaration: bool) {
        if declaration {
            self.emit_debug_declaration(name, value, value.ty.as_str(), &value.operand, "");
        } else {
            self.emit_debug_local(name, value);
        }
    }

    fn emit_debug_declaration(
        &mut self,
        name: &str,
        value_type: &Value,
        storage: &str,
        value: &str,
        expression: &str,
    ) {
        let Some(type_id) = self.debug_value_type(value_type) else {
            return;
        };
        let Some((variable, scope)) = self
            .debug
            .as_mut()
            .and_then(|debug| debug.declaration(name, type_id))
        else {
            return;
        };
        let slot = self.debug_storage_slot(storage, value, false);
        if let Some(debug) = self.debug.as_mut() {
            debug.current_scope = Some(scope);
        }
        self.emit_debug_declare(variable, storage, &slot, expression);
    }

    /// The `DILocalVariable` metadata id for `name` of type `ty`, if a debug
    /// build is active. The single lookup both debug-recorders funnel through.
    /// An environment alias is not a variable anyone wrote, so it has none.
    fn debug_var_id(
        &mut self,
        name: &str,
        value: &Value,
        argument: Option<usize>,
    ) -> Option<usize> {
        if crate::closure::is_alias(name) {
            return None;
        }
        let type_id = self.debug_value_type(value)?;
        self.debug
            .as_mut()?
            .local_variable_id(name, type_id, argument)
    }

    /// Record a source-level **parameter** for native debuggers via
    /// `llvm.dbg.value`. [DEBUGGER-DBG-DECLARE]
    ///
    /// The one-based argument number makes this a formal parameter. Without
    /// it LLVM can discard the location during instruction selection (for
    /// example while expanding saturating arithmetic to wide integer ops).
    /// Emitting at entry also avoids an inter-statement line-0 row.
    pub(crate) fn emit_debug_param(&mut self, name: &str, value: &Value, index: usize) {
        let Some(var_id) = self.debug_var_id(name, value, Some(index.saturating_add(1))) else {
            return;
        };
        self.add_extern("declare void @llvm.dbg.value(metadata, metadata, metadata)");
        self.emit(format!(
            "call void @llvm.dbg.value(metadata {} {}, metadata !{var_id}, metadata !DIExpression())",
            value.ty.as_str(),
            value.operand
        ));
        self.emit_debug_storage(var_id, value.ty.as_str(), &value.operand, "", true);
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
        let Some(var_id) = self.debug_var_id(name, value, None) else {
            return;
        };
        self.emit_debug_storage(var_id, value.ty.as_str(), &value.operand, "", false);
    }

    /// A mutable variable's debug location follows its live heap cell.
    /// Store the address, not a snapshot of the value. [DEBUGGER-DBG-DECLARE]
    pub(super) fn emit_debug_cell(&mut self, name: &str, cell: &CellSlot, declaration: bool) {
        let value = cell.value("");
        let storage = format!("{}*", cell.pointee);
        if declaration {
            self.emit_debug_declaration(name, &value, &storage, &cell.ptr, "DW_OP_deref");
        } else if let Some(var_id) = self.debug_var_id(name, &value, None) {
            self.emit_debug_storage(var_id, &storage, &cell.ptr, "DW_OP_deref", false);
        }
    }

    /// Keep immutable values or mutable cell addresses in debug-only storage.
    /// The expression dereferences cell addresses; metadata retains argument IDs.
    /// Parameter initialization is prologue code: entry breakpoints must follow
    /// the store, otherwise the debugger reads an uninitialized source value.
    fn emit_debug_storage(
        &mut self,
        var_id: usize,
        ty: &str,
        operand: &str,
        expression: &str,
        prologue: bool,
    ) {
        let slot = self.debug_storage_slot(ty, operand, prologue);
        self.emit_debug_declare(var_id, ty, &slot, expression);
    }

    fn debug_storage_slot(&mut self, ty: &str, operand: &str, prologue: bool) -> String {
        let previous = self.debug.as_ref().and_then(|debug| debug.current_position);
        if prologue {
            self.restore_debug_position(None);
        }
        // Entry allocation gives DWARF a stable frame address after branches
        // and avoids allocating another debug slot on every loop iteration.
        let slot = self.fresh_reg();
        self.cur_lines.insert(
            usize::from(!self.cur_lines.is_empty()),
            format!("  {slot} = alloca {ty}"),
        );
        self.emit(format!("store {ty} {operand}, {ty}* {slot}"));
        self.restore_debug_position(previous);
        slot
    }

    fn emit_debug_declare(&mut self, var_id: usize, ty: &str, slot: &str, expression: &str) {
        self.add_extern("declare void @llvm.dbg.declare(metadata, metadata, metadata)");
        self.emit(format!(
            "call void @llvm.dbg.declare(metadata {ty}* {slot}, metadata !{var_id}, metadata !DIExpression({expression}))"
        ));
    }
}

impl DebugState {
    fn declaration(&mut self, name: &str, type_id: usize) -> Option<(usize, usize)> {
        let parent = self.begin_lexical_scope(None)?;
        let scope = self.current_scope?;
        let variable = self.local_variable_id(name, type_id, None)?;
        self.current_scope = Some(parent);
        Some((variable, scope))
    }
}
