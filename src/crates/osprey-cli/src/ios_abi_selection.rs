//! Select the callable mobile C boundary from the compiled program.

use super::{
    c_signature, defines, export, reject_clashes, Export, ExportScope, HostAbi, Import, Program,
    ProgramTypes, Stmt, SOURCE_MAIN,
};

/// Derive the host ABI of `program` from its resolved types and the IR codegen
/// emitted for it. A generic function is inlined at each call site and has no
/// symbol, so it has no C signature either; every function that does have one
/// must have been defined.
///
/// # Errors
///
/// A scalar function codegen did not define is refused by name. Two functions
/// whose C names coincide (`app::x_y` and `app_x::y`), or an export whose name
/// an import already takes, would silently shadow one another at link time, so
/// the ABI refuses to be generated.
#[cfg(test)]
pub(super) fn host_abi(
    program: &Program,
    types: &ProgramTypes,
    ir: &str,
) -> Result<HostAbi, String> {
    host_abi_selected(program, types, ir, &ExportScope::EntrySource(None))
}

pub(super) fn host_abi_selected(
    program: &Program,
    types: &ProgramTypes,
    ir: &str,
    scope: &ExportScope,
) -> Result<HostAbi, String> {
    let (exports, undefined): (Vec<_>, Vec<_>) = program
        .statements
        .iter()
        .filter_map(|statement| function_export(statement, types, scope))
        .partition(|export| defines(ir, &export.symbol));
    reject_undefined(program, &undefined)?;
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
    let symbols: Vec<_> = abi.exports.iter().map(|e| e.symbol.as_str()).collect();
    let errors = export_effect_errors(program, &symbols);
    if errors.is_empty() {
        Ok(abi)
    } else {
        Err(errors.join("\n"))
    }
}

/// A scalar function codegen emitted no definition for would be missing from
/// the archive and header with no diagnostic, so the build fails and names it
/// -- with the effect check's reason when there is one (#281).
fn reject_undefined(program: &Program, undefined: &[Export]) -> Result<(), String> {
    if undefined.is_empty() {
        return Ok(());
    }
    let symbols: Vec<_> = undefined.iter().map(|e| e.symbol.as_str()).collect();
    let mut reasons = export_effect_errors(program, &symbols);
    for symbol in symbols {
        let named = format!("library export `{symbol}`");
        if !reasons.iter().any(|reason| reason.contains(&named)) {
            reasons.push(format!(
                "{named} has a C signature but codegen emitted no definition for it"
            ));
        }
    }
    Err(reasons.join("\n"))
}

fn export_effect_errors(program: &Program, symbols: &[&str]) -> Vec<String> {
    osprey_types::check_program_exports(program, symbols)
        .into_iter()
        .map(|error| error.message)
        .collect()
}

fn function_export(statement: &Stmt, types: &ProgramTypes, scope: &ExportScope) -> Option<Export> {
    let Stmt::Function {
        name,
        parameters,
        position,
        ..
    } = statement
    else {
        return None;
    };
    if !scope.publishes(*position) || name == SOURCE_MAIN {
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
        "extern `{name}` has an unsupported C ABI signature: parameters must be int, float, bool, string or Ptr; returns may also be Unit"
    ))?;
    Ok(Some(Import {
        symbol: name.clone(),
        params,
        ret,
    }))
}
