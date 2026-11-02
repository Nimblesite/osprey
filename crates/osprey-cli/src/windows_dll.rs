//! The Windows DLL form of a desktop app library, for a host that loads the application at run
//! time, such as the `WinUI` host. [WINUI-HOST-DLL]
//!
//! A DLL cannot leave a function for the executable that loads it to supply, so each `extern fn`
//! the host answers becomes an import slot [WINUI-HOST-IMPORTS]: a pointer the host fills through
//! `osprey_import_<name>` before it calls `osprey_main`, which returns [`UNBOUND`] while any slot
//! is still empty. An `extern fn` the linked runtime archive defines, such as the TLS session, is
//! no slot: the archive answers it inside the DLL. Only the boundary is exported: the entry, the
//! C exports and the binders.

use super::{c_header, defines, llvm_params, rename_symbol, CType, Export, HostAbi, Import, ENTRY};
use std::fmt::Write as _;

/// What `osprey_main` returns while an import slot is still empty.
pub(crate) const UNBOUND: i32 = -2;
/// The prefix of the export that fills one import's slot.
pub(crate) const BINDER_PREFIX: &str = "osprey_import_";
/// Codegen's entry, renamed so the slot check runs before it.
const SLOTTED_ENTRY: &str = "__osprey_dll_entry";
/// The closing guard every generated header ends with.
const HEADER_CLOSE: &str = "\n#ifdef __cplusplus\n}\n#endif\n";

/// The library's IR with its boundary exported from the DLL and every import it calls turned
/// into a slot, and the imports that became slots.
pub(crate) fn adapt(
    ir: &str,
    abi: &HostAbi,
    extend_bool: bool,
) -> Result<(String, Vec<Import>), String> {
    let slotted: Vec<Import> = abi
        .imports
        .iter()
        .filter(|import| declaration(ir, import).is_some() && !in_runtime(import))
        .cloned()
        .collect();
    reject_binder_clashes(ir, &slotted)?;
    let exported = dllexported(&rename_symbol(ir, ENTRY, SLOTTED_ENTRY), &abi.exports);
    let forwarded = slotted
        .iter()
        .fold(exported, |out, import| slot(&out, import, extend_bool));
    Ok((forwarded + &boundary(&slotted), slotted))
}

/// Whether the runtime archive linked into the DLL defines `import` itself.
fn in_runtime(import: &Import) -> bool {
    crate::linking::HTTP_RUNTIME_EXTERNS
        .iter()
        .any(|prefix| import.symbol.starts_with(prefix))
}

/// The line codegen declares `import` with, if the library calls it.
fn declaration<'a>(ir: &'a str, import: &Import) -> Option<&'a str> {
    let needle = format!("@{}(", import.symbol);
    ir.lines()
        .find(|line| line.starts_with("declare ") && line.contains(&needle))
}

fn binder_name(import: &Import) -> String {
    format!("{BINDER_PREFIX}{}", import.symbol)
}

fn slot_name(import: &Import) -> String {
    format!("__osprey_slot_{}", import.symbol)
}

/// A binder must not take the name of a function the library already defines.
fn reject_binder_clashes(ir: &str, slotted: &[Import]) -> Result<(), String> {
    match slotted
        .iter()
        .map(binder_name)
        .find(|name| defines(ir, name))
    {
        Some(name) => Err(format!(
            "`{name}` is already defined, and the Windows DLL needs that name to bind an import \
             [WINUI-HOST-IMPORTS]; rename the function"
        )),
        None => Ok(()),
    }
}

/// Marks the C exports `dllexport`, so the DLL exports them and nothing else of the program.
fn dllexported(ir: &str, exports: &[Export]) -> String {
    let needles: Vec<String> = exports
        .iter()
        .map(|export| format!(" @{}(", export.c_name))
        .collect();
    ir.split_inclusive('\n')
        .map(|line| match line.strip_prefix("define ") {
            Some(rest) if needles.iter().any(|needle| line.contains(needle)) => {
                format!("define dllexport {rest}")
            }
            _ => line.to_owned(),
        })
        .collect()
}

/// Replaces the import's declaration with a slot and an internal function that calls through it.
fn slot(ir: &str, import: &Import, extend_bool: bool) -> String {
    match declaration(ir, import) {
        Some(line) => ir.replacen(line, &forwarder(import, extend_bool), 1),
        None => ir.to_owned(),
    }
}

fn forwarder(import: &Import, extend_bool: bool) -> String {
    let ret = import.ret;
    let signature = format!("{}{}", ret.zeroext(extend_bool), ret.llvm());
    let params = llvm_params(&import.params, extend_bool);
    let call = format!("call {signature} %fn({params})");
    let body = match ret {
        CType::Unit => format!("  {call}\n  ret void"),
        _ => format!("  %r = {call}\n  ret {} %r", ret.llvm()),
    };
    format!(
        "@{slot} = internal global ptr null\n\
         define internal {signature} @{symbol}({params}) {{\nentry:\n  %fn = load ptr, ptr @{slot}\n{body}\n}}",
        slot = slot_name(import),
        symbol = import.symbol,
    )
}

/// The binders and the exported entry, which checks every slot before running the program.
fn boundary(slotted: &[Import]) -> String {
    let binders: String = slotted.iter().map(binder).collect();
    let checks: String = slotted
        .iter()
        .enumerate()
        .map(|(index, import)| check(index, import))
        .collect();
    format!(
        "{binders}\ndefine dllexport i32 @{ENTRY}() {{\nentry:\n  br label %check0\n{checks}\
         check{last}:\n  %status = call i32 @{SLOTTED_ENTRY}()\n  ret i32 %status\n\
         unbound:\n  ret i32 {UNBOUND}\n}}\n",
        last = slotted.len()
    )
}

fn binder(import: &Import) -> String {
    format!(
        "\ndefine dllexport void @{}(ptr %fn) {{\nentry:\n  store ptr %fn, ptr @{}\n  ret void\n}}\n",
        binder_name(import),
        slot_name(import)
    )
}

fn check(index: usize, import: &Import) -> String {
    format!(
        "check{index}:\n  %slot{index} = load ptr, ptr @{slot}\n  %empty{index} = icmp eq ptr %slot{index}, null\n  \
         br i1 %empty{index}, label %unbound, label %check{next}\n",
        slot = slot_name(import),
        next = index + 1
    )
}

/// The C header for the DLL: the entry and exports as for an archive, then one binder per slot.
pub(crate) fn header(exports: &[Export], slotted: &[Import], source: &str, target: &str) -> String {
    let abi = HostAbi {
        exports: exports.to_vec(),
        imports: Vec::new(),
    };
    let base = c_header::header_for_target(&abi, source, target);
    let binders: String = slotted.iter().map(binder_prototype).collect();
    let section = format!(
        "\n// Import slots [WINUI-HOST-IMPORTS]: the host passes each function to its binder\n\
         // before {ENTRY}, which returns {UNBOUND} while any slot is still empty.\n{binders}"
    );
    base.replacen(HEADER_CLOSE, &format!("{section}{HEADER_CLOSE}"), 1)
}

fn binder_prototype(import: &Import) -> String {
    let params = if import.params.is_empty() {
        "void".to_owned()
    } else {
        import
            .params
            .iter()
            .map(|(_, c)| c.c().to_owned())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut line = String::new();
    let _ = writeln!(line, "// extern fn {}", import.symbol);
    let _ = writeln!(
        line,
        "void {}({} (*fn)({params}));",
        binder_name(import),
        import.ret.c().trim_end()
    );
    line
}

#[cfg(test)]
#[path = "windows_dll_tests.rs"]
mod tests;
