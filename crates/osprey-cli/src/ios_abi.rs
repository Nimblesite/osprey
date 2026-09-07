//! The C ABI an iOS archive exposes to its host application. [IOS-HOST-ABI]
//!
//! Osprey functions are already emitted as plain LLVM functions over C-shaped
//! scalars (`i64`, `double`, `i1`, `i8*`), so the host needs no runtime bridge
//! — only a stable name and a C signature for each one. This module derives
//! both from the checked program: every top-level function whose resolved
//! parameter and return types are `int`, `float`, `bool`, `string` or `Unit`
//! (return only) becomes an `osprey_<name>` export backed by a forwarding
//! thunk in the IR, and every `extern fn` over those types is listed as an
//! import the host must define. Everything else — generic functions, `Result`,
//! collections, records, effects — stays internal to the archive.

use osprey_ast::{Program, Stmt};
use osprey_types::{names, ProgramTypes, Type};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Prefix on every exported C symbol.
const EXPORT_PREFIX: &str = "osprey_";
/// The C-linkage entry point that replaces codegen's `main`, so the host app
/// keeps its own. [IOS-TARGET-ENTRY]
pub(crate) const ENTRY: &str = "osprey_main";
/// Codegen's entry definition, exactly as `lower.rs` renders it.
const CODEGEN_ENTRY_DEFINE: &str = "define i32 @main() ";
/// The source-level entry, never exported as a function.
const SOURCE_MAIN: &str = "main";

/// An Osprey type with a direct C representation at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CType {
    Int,
    Float,
    Bool,
    Str,
    Unit,
}

impl CType {
    fn from_type(ty: &Type) -> Option<Self> {
        let Type::Con { name, args } = ty else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        match name.as_str() {
            names::INT => Some(Self::Int),
            names::FLOAT => Some(Self::Float),
            names::BOOL => Some(Self::Bool),
            names::STRING => Some(Self::Str),
            names::UNIT => Some(Self::Unit),
            _ => None,
        }
    }

    fn c(self) -> &'static str {
        match self {
            Self::Int => "int64_t",
            Self::Float => "double",
            Self::Bool => "bool",
            Self::Str => "const char *",
            Self::Unit => "void",
        }
    }

    fn osprey(self) -> &'static str {
        match self {
            Self::Int => names::INT,
            Self::Float => names::FLOAT,
            Self::Bool => names::BOOL,
            Self::Str => names::STRING,
            Self::Unit => names::UNIT,
        }
    }

    /// The LLVM type at the C boundary (`void` for `Unit`).
    fn llvm(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Float => "double",
            Self::Bool => "i1",
            Self::Str => "i8*",
            Self::Unit => "void",
        }
    }

    /// The LLVM type codegen gives the same value inside the archive: a
    /// `Unit` function returns an `i64` there.
    fn llvm_internal(self) -> &'static str {
        match self {
            Self::Unit => "i64",
            other => other.llvm(),
        }
    }

    /// Apple's arm64 C ABI zero-extends `bool` in both directions; clang spells
    /// that `zeroext` on the boundary, and so must the thunk.
    fn zeroext(self) -> &'static str {
        match self {
            Self::Bool => "zeroext ",
            _ => "",
        }
    }
}

/// An Osprey function the host may call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Export {
    /// The C symbol (`osprey_greet`, `osprey_app_greet` for a project).
    pub(crate) c_name: String,
    /// The emitted LLVM function it forwards to.
    pub(crate) symbol: String,
    /// The source-level qualified name, for the header comment.
    pub(crate) source_name: String,
    pub(crate) params: Vec<(String, CType)>,
    pub(crate) ret: CType,
}

/// An `extern fn` the host must define with C linkage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) symbol: String,
    pub(crate) params: Vec<(String, CType)>,
    pub(crate) ret: CType,
}

/// Both directions of the boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HostAbi {
    pub(crate) exports: Vec<Export>,
    pub(crate) imports: Vec<Import>,
}

/// Derive the host ABI of `program` from its resolved types and the IR codegen
/// emitted for it. A function is exported only when codegen actually defined
/// it — a generic function is inlined at each call site and has no symbol.
///
/// # Errors
///
/// Two functions whose C names coincide (`app::x_y` and `app_x::y`), or an
/// export whose name an import already takes, would silently shadow one
/// another at link time, so the ABI refuses to be generated.
pub(crate) fn host_abi(program: &Program, types: &ProgramTypes, ir: &str) -> Result<HostAbi, String> {
    let mut abi = HostAbi::default();
    for statement in &program.statements {
        match statement {
            Stmt::Function { name, parameters, .. } if name != SOURCE_MAIN && defines(ir, name) => {
                let names = parameters.iter().map(|p| p.name.as_str());
                if let Some((params, ret)) = c_signature(types, name, names) {
                    abi.exports.push(export(name, params, ret));
                }
            }
            Stmt::Extern { name, parameters, .. } => {
                let names = parameters.iter().map(|p| p.name.as_str());
                if let Some((params, ret)) = c_signature(types, name, names) {
                    abi.imports.push(Import { symbol: name.clone(), params, ret });
                }
            }
            _ => {}
        }
    }
    reject_clashes(&abi)?;
    Ok(abi)
}

fn defines(ir: &str, symbol: &str) -> bool {
    let needle = format!(" @{symbol}(");
    ir.lines().any(|line| line.starts_with("define ") && line.contains(&needle))
}

/// The C signature of `symbol`, if every parameter and the return type map.
fn c_signature<'a>(
    types: &ProgramTypes,
    symbol: &str,
    param_names: impl Iterator<Item = &'a str>,
) -> Option<(Vec<(String, CType)>, CType)> {
    let (param_types, ret) = types.functions.get(symbol)?;
    let params = param_names
        .zip(param_types)
        .map(|(name, ty)| {
            CType::from_type(ty)
                .filter(|c| *c != CType::Unit)
                .map(|c| (name.to_string(), c))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((params, CType::from_type(ret)?))
}

fn export(symbol: &str, params: Vec<(String, CType)>, ret: CType) -> Export {
    let source_name = osprey_ast::symbol::demangle(symbol).unwrap_or_else(|| symbol.to_string());
    Export {
        c_name: format!("{EXPORT_PREFIX}{}", source_name.replace("::", "_")),
        symbol: symbol.to_string(),
        source_name,
        params,
        ret,
    }
}

fn reject_clashes(abi: &HostAbi) -> Result<(), String> {
    let mut taken: BTreeSet<&str> = abi.imports.iter().map(|i| i.symbol.as_str()).collect();
    let _ = taken.insert(ENTRY);
    for export in &abi.exports {
        if !taken.insert(&export.c_name) {
            return Err(format!(
                "iOS export {} for `{}` collides with another boundary symbol; rename one of them",
                export.c_name, export.source_name
            ));
        }
    }
    Ok(())
}

/// Rename codegen's entry to [`ENTRY`] and append one C-ABI thunk per export.
///
/// # Errors
///
/// The IR must contain exactly the entry definition codegen renders; anything
/// else means the backend changed underneath this driver, which must not be
/// papered over with a guess.
pub(crate) fn with_host_abi(ir: &str, abi: &HostAbi) -> Result<String, String> {
    if !ir.contains(CODEGEN_ENTRY_DEFINE) {
        return Err(format!("codegen did not emit the `{CODEGEN_ENTRY_DEFINE}` entry the iOS target renames"));
    }
    let mut out = ir.replacen(CODEGEN_ENTRY_DEFINE, &format!("define i32 @{ENTRY}() "), 1);
    for export in &abi.exports {
        out.push_str(&thunk(export));
    }
    Ok(out)
}

fn thunk(export: &Export) -> String {
    let boundary = export
        .params
        .iter()
        .enumerate()
        .map(|(i, (_, c))| format!("{} {}%p{i}", c.llvm(), c.zeroext()))
        .collect::<Vec<_>>()
        .join(", ");
    let forwarded = export
        .params
        .iter()
        .enumerate()
        .map(|(i, (_, c))| format!("{} %p{i}", c.llvm()))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = export.ret;
    let call = format!("call {} @{}({forwarded})", ret.llvm_internal(), export.symbol);
    let body = if ret == CType::Unit {
        format!("  {call}\n  ret void")
    } else {
        format!("  %r = {call}\n  ret {} %r", ret.llvm())
    };
    format!(
        "\ndefine {}{} @{}({boundary}) {{\n{body}\n}}\n",
        ret.zeroext(),
        ret.llvm(),
        export.c_name
    )
}

/// Render the C header a host compiles against. [IOS-HOST-ABI]
pub(crate) fn header(abi: &HostAbi, source: &str) -> String {
    let mut h = format!(
        "// Osprey iOS host ABI for {source}.\n\
         // Generated by `osprey --target=ios`; do not edit. [IOS-HOST-ABI]\n\
         #pragma once\n#include <stdbool.h>\n#include <stdint.h>\n\n\
         #ifdef __cplusplus\nextern \"C\" {{\n#endif\n\n\
         // Runs the program's top-level statements and `fn main`, returning its\n\
         // exit status. Call it once, before any other export. [IOS-TARGET-ENTRY]\n\
         int32_t {ENTRY}(void);\n"
    );
    if !abi.exports.is_empty() {
        h.push_str(
            "\n// Exports: Osprey functions the host may call. A returned string belongs\n\
             // to the Osprey runtime; copy it before the next call into the library.\n",
        );
    }
    for e in &abi.exports {
        let _ = writeln!(h, "// fn {}{}", e.source_name, osprey_signature(&e.params, e.ret));
        let _ = writeln!(h, "{}", prototype(&e.c_name, &e.params, e.ret));
    }
    if !abi.imports.is_empty() {
        h.push_str(
            "\n// Imports: functions the host must define with C linkage\n\
             // (Swift: `@_cdecl(\"name\")`). Osprey calls them through `extern fn`.\n",
        );
    }
    for i in &abi.imports {
        let _ = writeln!(h, "// extern fn {}{}", i.symbol, osprey_signature(&i.params, i.ret));
        let _ = writeln!(h, "{}", prototype(&i.symbol, &i.params, i.ret));
    }
    h.push_str("\n#ifdef __cplusplus\n}\n#endif\n");
    h
}

fn osprey_signature(params: &[(String, CType)], ret: CType) -> String {
    let params = params
        .iter()
        .map(|(name, c)| format!("{name}: {}", c.osprey()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({params}) -> {}", ret.osprey())
}

fn prototype(name: &str, params: &[(String, CType)], ret: CType) -> String {
    let params = if params.is_empty() {
        "void".to_string()
    } else {
        params
            .iter()
            .map(|(n, c)| format!("{}{n}", c.c()).replace("* ", "*"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret = ret.c();
    let space = if ret.ends_with('*') { "" } else { " " };
    format!("{ret}{space}{name}({params});")
}

#[cfg(test)]
#[path = "ios_abi_tests.rs"]
mod tests;
