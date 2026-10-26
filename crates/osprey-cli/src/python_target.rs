//! Python-hosted native libraries reuse the checked C ABI. [PYTHON-HOST-ABI]

use crate::ios_abi::{self, CType, HostAbi, Import};
use crate::project::CompilationInput;
use crate::toolchain::{fail, publish, write, Scratch};
use crate::Cli;
use osprey_ast::Program;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const CALL: &str = "osprey_python_call";
const OBJECT: &str = "osprey_python_object";
const INSTALL: &str = "osprey_python_install";
const ABI: &str = "osprey_python_abi";
const BRIDGE: &str = include_str!("../../../compiler/runtime/python_bridge.c");
const BRIDGE_HEADER: &str = include_str!("../../../compiler/runtime/python_bridge.h");
const MEMORY_HEADER: &str = include_str!("../../../compiler/runtime/memory_hooks.h");
const COMPILE_FLAGS: &[&str] = &[
    "-O2",
    "-fPIC",
    "-fstack-protector-strong",
    "-D_FORTIFY_SOURCE=2",
    "-Wno-override-module",
    "-Wall",
    "-Wextra",
    "-Werror",
    "-Wstrict-prototypes",
    "-Wmissing-prototypes",
];

pub(crate) fn validate(cli: &Cli) -> Result<(), ExitCode> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(fail("the Python target currently requires Linux or macOS"));
    }
    crate::reject_cross_target_options(cli, "Python", Some("Python (python -m osprey)"))
}

pub(crate) fn source(
    program: &Program,
    path: &str,
    scope: &ios_abi::ExportScope,
) -> Result<(String, HostAbi), String> {
    let extend_bool = !cfg!(all(target_os = "linux", target_arch = "aarch64"));
    let (ir, abi) = ios_abi::source_with_abi(program, path, "python", extend_bool, scope)?;
    validate_boundary(&abi, &ir)?;
    Ok((ir, abi))
}

fn validate_boundary(abi: &HostAbi, ir: &str) -> Result<(), String> {
    for symbol in [CALL, OBJECT, INSTALL, ABI] {
        if ios_abi::defines(ir, symbol) {
            return Err(format!("Python bridge symbol `{symbol}` is reserved"));
        }
    }
    validate_exports(abi)?;
    abi.imports.iter().try_for_each(validate_import)
}

fn validate_exports(abi: &HostAbi) -> Result<(), String> {
    for export in &abi.exports {
        if export.ret == CType::Ptr || export.params.iter().any(|(_, ty)| *ty == CType::Ptr) {
            return Err(format!(
                "Python export `{}` passes a Ptr; the Python host exchanges int, float, bool and string",
                export.source_name
            ));
        }
        if [CALL, OBJECT, INSTALL, ABI]
            .iter()
            .any(|name| *name == export.c_name)
        {
            return Err(format!(
                "Python bridge symbol `{}` is reserved",
                export.c_name
            ));
        }
    }
    Ok(())
}

fn validate_import(import: &Import) -> Result<(), String> {
    let expected = match import.symbol.as_str() {
        CALL => vec![CType::Str; 3],
        OBJECT => vec![CType::Str, CType::Int, CType::Str, CType::Str],
        _ => Vec::new(),
    };
    let actual: Vec<_> = import.params.iter().map(|(_, ty)| *ty).collect();
    if expected.is_empty() || actual != expected || import.ret != CType::Str {
        return Err(format!(
            "Python import `{}` has an unsupported signature; use {CALL}(string, string, string) -> string or {OBJECT}(string, int, string, string) -> string",
            import.symbol
        ));
    }
    Ok(())
}

fn manifest(abi: &HostAbi) -> Value {
    let exports: Vec<_> = abi
        .exports
        .iter()
        .map(|export| {
            json!({
                "name": export.source_name,
                "symbol": export.c_name,
                "parameters": export.params.iter().map(|(_, ty)| ty.osprey()).collect::<Vec<_>>(),
                "parameter_names": export.params.iter().map(|(name, _)| name).collect::<Vec<_>>(),
                "returns": export.ret.osprey(),
            })
        })
        .collect();
    json!({"version": 1, "exports": exports})
}

fn bridge_source(abi: &HostAbi) -> Result<String, ExitCode> {
    let literal = serde_json::to_string(&manifest(abi).to_string())
        .map_err(|error| fail(&format!("cannot encode Python ABI: {error}")))?;
    Ok(format!(
        "{BRIDGE}\nconst char *{ABI}(void);\nconst char *{ABI}(void) {{ return {literal}; }}\n"
    ))
}

pub(crate) fn build(cli: &Cli, input: &CompilationInput, out: &Path) -> Result<(), ExitCode> {
    validate(cli)?;
    validate_output(out)?;
    let (ir, abi) = source(
        input.backend_program(),
        input.debug_path(),
        &input.export_scope(false),
    )
    .map_err(|e| fail(&e))?;
    let scratch = Scratch::new(input.debug_path(), "python")?;
    let (ll, bridge) = write_sources(&scratch, &ir, &abi)?;
    let library = scratch.path.join("library.so");
    link(&ll, &bridge, &library, &ir, input.source(), &cli.memory)?;
    publish(
        &library,
        out,
        &ios_abi::header_for_target(&abi, input.display_path(), "python"),
    )
}

fn write_sources(
    scratch: &Scratch,
    ir: &str,
    abi: &HostAbi,
) -> Result<(PathBuf, PathBuf), ExitCode> {
    let ll = scratch.path.join("library.ll");
    let bridge = scratch.path.join("bridge.c");
    write(&ll, ir)?;
    write(&bridge, &bridge_source(abi)?)?;
    write(&scratch.path.join("python_bridge.h"), BRIDGE_HEADER)?;
    write(&scratch.path.join("memory_hooks.h"), MEMORY_HEADER)?;
    Ok((ll, bridge))
}

fn validate_output(out: &Path) -> Result<(), ExitCode> {
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    if out.extension().and_then(|value| value.to_str()) != Some(extension) {
        return Err(fail(&format!("Python output must end in .{extension}")));
    }
    Ok(())
}

fn link(
    ll: &Path,
    bridge: &Path,
    out: &Path,
    ir: &str,
    source: &str,
    memory: &str,
) -> Result<(), ExitCode> {
    let mut command = Command::new(crate::native::c_compiler());
    let _ = command
        .args([ll, bridge])
        .arg("-o")
        .arg(out)
        .args(COMPILE_FLAGS)
        .arg(if cfg!(target_os = "macos") {
            "-dynamiclib"
        } else {
            "-shared"
        });
    if cfg!(target_os = "linux") {
        let _ = command.arg("-Wl,-z,defs").arg("-Wl,-Bsymbolic");
    }
    let _ = command.args(crate::linking::link_args(ir, source, memory));
    crate::native::run_build_step(command, ll)
}
