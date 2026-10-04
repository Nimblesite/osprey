//! Resolve the active source binding before file-level storage or declarations.
//! Implements [BLOCK-SCOPE] and [MODULES-FILE-SCOPE-BINDING].
use super::{with_lambda_captures, Codegen, CodegenError, Result, Value};

pub(super) fn gen(cg: &mut Codegen, name: &str) -> Result<Value> {
    if cg.cell_slots.contains_key(name) {
        return cg
            .cell_read(name)
            .ok_or_else(|| CodegenError::unknown(name));
    }
    if let Some(value) = cg.lookup(name) {
        return Ok(value);
    }
    if let Some(target) = cg.call_aliases.get(name).cloned() {
        return crate::closure::named_fn_cell(cg, &target);
    }
    if cg.lambdas.contains_key(name) {
        return lambda(cg, name);
    }
    file_binding(cg, name)
}

fn file_binding(cg: &mut Codegen, name: &str) -> Result<Value> {
    if cg.module_globals.contains_key(name) {
        return crate::globals::read(cg, name).ok_or_else(|| CodegenError::unknown(name));
    }
    if cg.is_ctor(name) {
        return crate::aggregate::gen_constructor(cg, name, &[]);
    }
    if cg.fn_params.contains_key(name) {
        return crate::closure::named_fn_cell(cg, name);
    }
    lambda(cg, name)
}

fn lambda(cg: &mut Codegen, name: &str) -> Result<Value> {
    let Some((parameters, body, position)) = cg.lambda_def(name).cloned() else {
        return Err(CodegenError::unknown(name));
    };
    with_lambda_captures(cg, name, |cg| {
        crate::closure::lambda_value(cg, &parameters, &body, position)
    })
}
