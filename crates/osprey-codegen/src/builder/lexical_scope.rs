//! Preserve declaration scope while specialization emits into a caller's frame.
use super::{CellSlot, Codegen, FnSig, LambdaDef};
use crate::llty::Value;
use osprey_ast::Parameter;
use osprey_types::Type;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct FileScopeState {
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

impl FileScopeState {
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
