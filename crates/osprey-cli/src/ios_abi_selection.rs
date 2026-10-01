//! Select the callable mobile C boundary from the compiled program.

use super::{
    c_signature, defines, export, reject_clashes, Export, HostAbi, Import, Program, ProgramTypes,
    Stmt, SOURCE_MAIN,
};

/// Derive the host ABI of `program` from its resolved types and the IR codegen
/// emitted for it. A function is exported only when codegen actually defined
/// it — a generic function is inlined at each call site and has no symbol.
///
/// # Errors
///
/// Two functions whose C names coincide (`app::x_y` and `app_x::y`), or an
/// export whose name an import already takes, would silently shadow one
/// another at link time, so the ABI refuses to be generated.
#[cfg(test)]
pub(super) fn host_abi(
    program: &Program,
    types: &ProgramTypes,
    ir: &str,
) -> Result<HostAbi, String> {
    host_abi_selected(program, types, ir, true)
}

pub(super) fn host_abi_selected(
    program: &Program,
    types: &ProgramTypes,
    ir: &str,
    export_functions: bool,
) -> Result<HostAbi, String> {
    let exports = program
        .statements
        .iter()
        .filter_map(|statement| function_export(statement, types, ir, export_functions))
        .collect();
    let imports = program
        .statements
        .iter()
        .map(|statement| extern_import(statement, types))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    let abi = HostAbi { exports, imports };
    reject_clashes(&abi, program, ir)?;
    check_export_effects(program, &abi)?;
    Ok(abi)
}

fn check_export_effects(program: &Program, abi: &HostAbi) -> Result<(), String> {
    let exports: Vec<_> = abi.exports.iter().map(|e| e.symbol.as_str()).collect();
    let errors = osprey_types::check_program_exports(program, &exports);
    if !errors.is_empty() {
        return Err(errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("\n"));
    }
    Ok(())
}

fn function_export(
    statement: &Stmt,
    types: &ProgramTypes,
    ir: &str,
    enabled: bool,
) -> Option<Export> {
    let Stmt::Function {
        name, parameters, ..
    } = statement
    else {
        return None;
    };
    if !enabled || name == SOURCE_MAIN || !defines(ir, name) {
        return None;
    }
    let names = parameters.iter().map(|p| p.name.as_str());
    c_signature(types, name, names).map(|(params, ret)| export(name, params, ret))
}

fn extern_import(statement: &Stmt, types: &ProgramTypes) -> Result<Option<Import>, String> {
    let Stmt::Extern {
        name, parameters, ..
    } = statement
    else {
        return Ok(None);
    };
    let names = parameters.iter().map(|p| p.name.as_str());
    let (params, ret) = c_signature(types, name, names).ok_or_else(|| format!(
        "extern `{name}` has an unsupported C ABI signature: parameters must be int, float, bool or string; returns may also be Unit"
    ))?;
    Ok(Some(Import {
        symbol: name.clone(),
        params,
        ret,
    }))
}
