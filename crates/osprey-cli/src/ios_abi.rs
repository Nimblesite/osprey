//! Shared checked mobile C boundary. [IOS-HOST-ABI] [ANDROID-HOST-ABI]
//!
//! Scalar functions receive stable C exports, and host-provided extern
//! functions become imports. Thunks adapt Apple's bool attributes and Unit
//! returns. Generic and aggregate function signatures stay internal; extern
//! signatures must be scalar. Every export must discharge its own effects.

use osprey_ast::{Program, Stmt};
use osprey_types::{names, ProgramTypes, Type};
use std::collections::BTreeSet;
#[path = "ios_abi_header.rs"]
mod c_header;
pub(crate) use c_header::{header, header_for_target};

/// Prefix on every exported C symbol.
const EXPORT_PREFIX: &str = "osprey_";
/// The C-linkage entry point that replaces codegen's `main`, so the host app
/// keeps its own. [IOS-TARGET-ENTRY]
pub(crate) const ENTRY: &str = "osprey_main";
const INIT_FUNCTION: &str = "__osprey_ios_initialize";
const INIT_STATE: &str = "__osprey_ios_init_state";
const INIT_STATUS: &str = "__osprey_ios_init_status";
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

    /// Apple ARM64 and Android x86-64 extend bool; Android AAPCS64 does not.
    /// The thunk must match clang for the chosen C ABI.
    fn zeroext(self, extend_bool: bool) -> &'static str {
        match self {
            Self::Bool if extend_bool => "zeroext ",
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
pub(crate) fn host_abi(
    program: &Program,
    types: &ProgramTypes,
    ir: &str,
) -> Result<HostAbi, String> {
    let mut abi = HostAbi::default();
    for statement in &program.statements {
        match statement {
            Stmt::Function {
                name, parameters, ..
            } if name != SOURCE_MAIN && defines(ir, name) => {
                let names = parameters.iter().map(|p| p.name.as_str());
                if let Some((params, ret)) = c_signature(types, name, names) {
                    abi.exports.push(export(name, params, ret));
                }
            }
            Stmt::Extern {
                name, parameters, ..
            } => {
                let names = parameters.iter().map(|p| p.name.as_str());
                let (params, ret) = c_signature(types, name, names).ok_or_else(|| format!(
                    "extern `{name}` has an unsupported C ABI signature: parameters must be int, float, bool or string; returns may also be Unit"
                ))?;
                abi.imports.push(Import {
                    symbol: name.clone(),
                    params,
                    ret,
                });
            }
            _ => {}
        }
    }
    reject_clashes(&abi, program, ir)?;
    let exports: Vec<_> = abi.exports.iter().map(|e| e.symbol.as_str()).collect();
    let errors = osprey_types::check_program_exports(program, &exports);
    if !errors.is_empty() {
        return Err(errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("\n"));
    }
    Ok(abi)
}

fn defines(ir: &str, symbol: &str) -> bool {
    let needle = format!(" @{symbol}(");
    ir.lines()
        .any(|line| line.starts_with("define ") && line.contains(&needle))
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

fn reject_clashes(abi: &HostAbi, program: &Program, ir: &str) -> Result<(), String> {
    let mut taken: BTreeSet<_> = ir
        .lines()
        .filter_map(defined_symbol)
        .map(str::to_owned)
        .collect();
    taken.extend(program.statements.iter().filter_map(|s| match s {
        Stmt::Extern { name, .. } => Some(name.clone()),
        _ => None,
    }));
    for symbol in [ENTRY, INIT_FUNCTION, INIT_STATE, INIT_STATUS] {
        reserve(&mut taken, symbol)?;
    }
    for e in &abi.exports {
        reserve(&mut taken, &e.c_name)?;
    }
    for i in &abi.imports {
        if !c_identifier(&i.symbol) || i.symbol == SOURCE_MAIN {
            return Err(format!(
                "import `{}` is not an available C function name",
                i.symbol
            ));
        }
        reserve(&mut taken, &import_adapter_name(i))?;
    }
    Ok(())
}

fn defined_symbol(line: &str) -> Option<&str> {
    (line.starts_with("define ") || line.starts_with("declare ") || line.starts_with('@'))
        .then(|| line.split_once('@'))??
        .1
        .split(['(', ' ', '='])
        .next()
}

fn reserve(taken: &mut BTreeSet<String>, symbol: &str) -> Result<(), String> {
    if !c_identifier(symbol) || !taken.insert(symbol.to_string()) {
        return Err(format!(
            "C ABI boundary symbol `{symbol}` is invalid or collides with another symbol; rename it"
        ));
    }
    Ok(())
}

/// Wrap codegen's entry with initialization tracking and adapt C ABI calls.
///
/// # Errors
///
/// The IR must contain exactly the entry definition codegen renders; anything
/// else means the backend changed underneath this driver, which must not be
/// papered over with a guess.
pub(crate) fn with_host_abi(ir: &str, abi: &HostAbi) -> Result<String, String> {
    with_host_abi_for_target(ir, abi, true)
}

pub(crate) fn with_host_abi_for_target(
    ir: &str,
    abi: &HostAbi,
    extend_bool: bool,
) -> Result<String, String> {
    if ir
        .lines()
        .filter(|line| line.starts_with(CODEGEN_ENTRY_DEFINE))
        .count()
        != 1
    {
        return Err(format!(
            "codegen did not emit the `{CODEGEN_ENTRY_DEFINE}` entry the mobile target renames"
        ));
    }
    let mut out = rename_symbol(ir, SOURCE_MAIN, INIT_FUNCTION);
    for import in &abi.imports {
        out = adapt_import(&out, import, extend_bool);
    }
    for export in &abi.exports {
        out.push_str(&thunk(export, extend_bool));
    }
    out.push_str(&initialization_thunk());
    Ok(out)
}

fn initialization_thunk() -> String {
    format!(
        "\n@{INIT_STATE} = internal global i8 0\n@{INIT_STATUS} = internal global i32 0\n\
         define i32 @{ENTRY}() {{\nentry:\n  %state = load i8, i8* @{INIT_STATE}\n\
           switch i8 %state, label %busy [ i8 0, label %initialize i8 2, label %complete ]\n\
         initialize:\n  store i8 1, i8* @{INIT_STATE}\n  %status = call i32 @{INIT_FUNCTION}()\n\
           store i32 %status, i32* @{INIT_STATUS}\n  store i8 2, i8* @{INIT_STATE}\n  ret i32 %status\n\
         complete:\n  %cached = load i32, i32* @{INIT_STATUS}\n  ret i32 %cached\n\
         busy:\n  ret i32 1\n}}\n"
    )
}

fn import_adapter_name(import: &Import) -> String {
    format!("__osprey_host_{}", import.symbol)
}

fn rename_symbol(ir: &str, from: &str, to: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    let mut chars = ir.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            quoted = !quoted;
        }
        out.push(c);
        if c == '@' && !quoted {
            let symbol: String = std::iter::from_fn(|| {
                chars.next_if(|c| c.is_ascii_alphanumeric() || "_.$-".contains(*c))
            })
            .collect();
            out.push_str(if symbol == from { to } else { &symbol });
        }
    }
    out
}

fn adapt_import(ir: &str, import: &Import, extend_bool: bool) -> String {
    let needle = format!("@{}(", import.symbol);
    let Some(declaration) = ir
        .lines()
        .find(|line| line.starts_with("declare ") && line.contains(&needle))
    else {
        return ir.to_string();
    };
    let adapter = import_adapter_name(import);
    let renamed = rename_symbol(&ir.replace(declaration, ""), &import.symbol, &adapter);
    let params = import
        .params
        .iter()
        .map(|(_, c)| {
            format!("{} {}", c.llvm(), c.zeroext(extend_bool))
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{renamed}\ndeclare {}{} @{}({params})\n{}",
        import.ret.zeroext(extend_bool),
        import.ret.llvm(),
        import.symbol,
        import_thunk(import, &adapter, extend_bool)
    )
}

fn import_thunk(import: &Import, adapter: &str, extend_bool: bool) -> String {
    let internal = llvm_params(&import.params, false);
    let boundary = llvm_params(&import.params, extend_bool);
    let ret = import.ret;
    let call = format!(
        "call {}{} @{}({boundary})",
        ret.zeroext(extend_bool),
        ret.llvm(),
        import.symbol
    );
    let body = if ret == CType::Unit {
        format!("  {call}\n  ret i64 0")
    } else {
        format!("  %r = {call}\n  ret {} %r", ret.llvm())
    };
    format!(
        "\ndefine internal {} @{adapter}({internal}) {{\n{body}\n}}\n",
        ret.llvm_internal()
    )
}

fn llvm_params(params: &[(String, CType)], boundary: bool) -> String {
    params
        .iter()
        .enumerate()
        .map(|(i, (_, c))| format!("{} {}%p{i}", c.llvm(), c.zeroext(boundary)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn thunk(export: &Export, extend_bool: bool) -> String {
    let boundary = llvm_params(&export.params, extend_bool);
    let forwarded = llvm_params(&export.params, false);
    let ret = export.ret;
    let call = format!(
        "call {} @{}({forwarded})",
        ret.llvm_internal(),
        export.symbol
    );
    let body = if ret == CType::Unit {
        format!("  {call}\n  ret void")
    } else {
        format!("  %r = {call}\n  ret {} %r", ret.llvm())
    };
    format!(
        "\ndefine {}{} @{}({boundary}) {{\n{body}\n}}\n",
        ret.zeroext(extend_bool),
        ret.llvm(),
        export.c_name
    )
}

fn c_identifier(name: &str) -> bool {
    let reserved = "alignas alignof and and_eq asm atomic_auto atomic_cancel atomic_commit atomic_noexcept auto bitand bitor bool break case catch char char8_t char16_t char32_t class compl concept const consteval constexpr constinit const_cast continue co_await co_return co_yield decltype default delete do double dynamic_cast else enum explicit export extern false float for friend goto if inline int int32_t int64_t long mutable namespace new noexcept not not_eq nullptr operator or or_eq private protected public register reinterpret_cast requires restrict return short signed sizeof size_t static static_assert static_cast struct switch template this thread_local throw true try typedef typeid typename union unsigned using virtual void volatile wchar_t while xor xor_eq _Alignas _Alignof _Atomic _Bool _Complex _Generic _Imaginary _Noreturn _Static_assert _Thread_local";
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !reserved.split_whitespace().any(|word| word == name)
        && !c_header::reserved_identifier(name)
}

/// Check the entire library boundary before discovering a platform toolchain.
/// Implements [IOS-HOST-ABI] and [ANDROID-HOST-ABI].
pub(crate) fn source(
    program: &Program,
    path: &str,
    target: &str,
    extend_bool: bool,
) -> Result<(String, String), String> {
    crate::target_capabilities::validate(program, target)?;
    let ir = osprey_codegen::compile_library(program).map_err(|e| format!("{path}: {e}"))?;
    let abi = host_abi(program, &osprey_types::infer_program(program), &ir)
        .map_err(|error| format!("{path}: target `{target}` C ABI: {error}"))?;
    let adapted = if extend_bool {
        with_host_abi(&ir, &abi)?
    } else {
        with_host_abi_for_target(&ir, &abi, false)?
    };
    let header = if target == "ios" {
        header(&abi, path)
    } else {
        header_for_target(&abi, path, target)
    };
    Ok((adapted, header))
}

#[cfg(test)]
#[path = "ios_abi_tests.rs"]
mod tests;
