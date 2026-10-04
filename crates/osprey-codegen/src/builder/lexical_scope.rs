//! Preserve declaration scope while specialization emits into a caller's frame.
use super::{CellSlot, Codegen, FnSig, LambdaDef};
use crate::llty::Value;
use osprey_ast::Parameter;
use osprey_types::Type;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Default)]
pub(crate) struct LexicalScopeState {
    scopes: Vec<HashMap<String, Value>>,
    scope_ids: Vec<usize>,
    lambdas: HashMap<String, LambdaDef>,
    lambda_prefix: HashMap<String, (Vec<Parameter>, Vec<Value>)>,
    call_aliases: HashMap<String, String>,
    fn_ptr_locals: HashMap<String, FnSig>,
    fn_value_types: HashMap<String, Type>,
    cell_vars: HashSet<String>,
    inlining: HashSet<String>,
    cell_slots: HashMap<String, CellSlot>,
}

impl LexicalScopeState {
    pub(crate) fn child(cg: &mut Codegen) -> Self {
        let mut saved = Self::default();
        saved.swap(cg);
        saved.clone().swap(cg);
        cg.push_scope();
        saved
    }

    pub(crate) fn enter(cg: &mut Codegen) -> Self {
        let mut saved = Self {
            call_aliases: cg.file_aliases.clone(),
            inlining: cg.inlining.clone(),
            ..Self::default()
        };
        saved.swap(cg);
        cg.push_scope();
        saved
    }

    pub(crate) fn restore(mut self, cg: &mut Codegen) {
        self.swap(cg);
    }

    /// Evaluate an argument in its caller at the point the application reaches it.
    pub(crate) fn with<T>(&mut self, cg: &mut Codegen, emit: impl FnOnce(&mut Codegen) -> T) -> T {
        self.swap(cg);
        let result = emit(cg);
        self.swap(cg);
        result
    }

    fn swap(&mut self, cg: &mut Codegen) {
        std::mem::swap(&mut self.scopes, &mut cg.scopes);
        std::mem::swap(&mut self.scope_ids, &mut cg.scope_ids);
        std::mem::swap(&mut self.lambdas, &mut cg.lambdas);
        std::mem::swap(&mut self.lambda_prefix, &mut cg.lambda_prefix);
        std::mem::swap(&mut self.call_aliases, &mut cg.call_aliases);
        std::mem::swap(&mut self.fn_ptr_locals, &mut cg.fn_ptr_locals);
        std::mem::swap(&mut self.fn_value_types, &mut cg.fn_value_types);
        std::mem::swap(&mut self.cell_vars, &mut cg.cell_vars);
        std::mem::swap(&mut self.inlining, &mut cg.inlining);
        std::mem::swap(&mut self.cell_slots, &mut cg.cell_slots);
    }
}

impl Codegen {
    /// A completed declaration replaces every representation of its name.
    /// The containing lexical scope restores enclosing bindings on exit.
    /// Implements [BLOCK-SCOPE] and [PATTERN-BINDING-SCOPE].
    pub(crate) fn forget_binding(&mut self, name: &str) {
        for scope in &mut self.scopes {
            let _ = scope.remove(name);
        }
        let _ = self.cell_slots.remove(name);
        let _ = self.call_aliases.remove(name);
        let _ = self.lambdas.remove(name);
        let _ = self.lambda_prefix.remove(name);
        let _ = self.fn_ptr_locals.remove(name);
        let _ = self.fn_value_types.remove(name);
    }

    pub(crate) fn with_local_scope<T>(&mut self, emit: impl FnOnce(&mut Self) -> T) -> T {
        let saved = LexicalScopeState::child(self);
        let result = emit(self);
        saved.restore(self);
        result
    }

    /// Caller expressions must not inherit the callee's type substitution.
    pub(crate) fn with_caller_types<T>(&mut self, emit: impl FnOnce(&mut Self) -> T) -> T {
        let original = self
            .application_caller
            .clone()
            .map(|caller| std::mem::replace(&mut self.prog, caller));
        let result = emit(self);
        if let Some(original) = original {
            self.prog = original;
        }
        result
    }
}

impl Codegen {
    // ---- scopes ----

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_ids.push(self.next_scope_id);
        self.next_scope_id = self.next_scope_id.saturating_add(1);
    }

    pub(crate) fn with_file_scope<T>(&mut self, emit: impl FnOnce(&mut Self) -> T) -> T {
        let saved = LexicalScopeState::enter(self);
        let result = emit(self);
        saved.restore(self);
        result
    }

    pub(crate) fn pop_scope(&mut self) {
        let _ = self.scopes.pop();
        let _ = self.scope_ids.pop();
    }

    pub(crate) fn scope_id(&self) -> Option<usize> {
        self.scope_ids.last().copied()
    }

    pub(crate) fn bind(&mut self, name: impl Into<String>, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.insert(name.into(), value);
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Value> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
    }

    /// Re-tag an already-bound name in the innermost scope that holds it. Used
    /// to record a channel's element type at the `send` that establishes it, so
    /// a later `recv` on the same binding unboxes to that type rather than to
    /// the uniform `i64` wire word ([CONCURRENCY-CHANNEL]).
    pub(crate) fn retag(&mut self, name: &str, retag: impl FnOnce(Value) -> Value) {
        if let Some(scope) = self.scopes.iter_mut().rev().find(|s| s.contains_key(name)) {
            if let Some(stored) = scope.remove(name) {
                let _ = scope.insert(String::from(name), retag(stored));
            }
        }
    }

    /// Read a cell-backed variable: `load` its current value from the heap slot.
    /// `None` when `name` is not promoted to a cell (the caller falls back to a
    /// normal scope lookup).
    pub(crate) fn cell_read(&mut self, name: &str) -> Option<Value> {
        let slot = self.cell_slots.get(name).cloned()?;
        let ty = slot.pointee.as_str();
        let r = self.emit_reg(format!("load {ty}, {ty}* {}", slot.ptr));
        Some(slot.value(r))
    }

    /// The lambda `name` is bound to for inline application: this function's
    /// own beta-reduction cache first, then the file-scope bindings that
    /// outlive it ([`Codegen::file_lambdas`]).
    pub(crate) fn lambda_def(&self, name: &str) -> Option<&LambdaDef> {
        self.lambdas
            .get(name)
            .or_else(|| self.file_lambdas.get(name))
    }
}
