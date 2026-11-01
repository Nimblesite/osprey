//! WebAssembly (`wasm32-wasip1`) backend driver. [WASM-TARGET]
//!
//! Osprey codegen already emits target-agnostic textual LLVM IR
//! [WASM-TARGET-IR] (no triple or datalayout; `int` is `i64`; pointers
//! round-trip through `i64`, which is safe
//! on wasm32 since addresses fit in 32 bits). This module turns that IR into a
//! browser-ready `.wasm`:
//!
//!   1. append a `__main_void` entry thunk (see [`with_entry_thunk`]);
//!   2. clang lowers the IR to a wasm object — `-c` only, because stock
//!      Homebrew/apt LLVM does not ship a wasm compiler-rt archive;
//!   3. `wasm-ld` links `crt1-command.o` + that object + the portable wasm
//!      runtime archive (`libosprey_runtime_wasm.a`, `make -C website wasm`) + libc. The
//!      runtime supplies the standard `__multi3` helper used when LLVM lowers
//!      checked i64 multiplication through a widened intermediate.
//!
//! Driving `wasm-ld` directly — rather than the clang link driver — is what lets
//! step 3 control the exact link line and sidestep the missing builtins archive.
//! The result is a command module (`_start` → `__wasm_call_ctors` →
//! `__main_void`) that runs uniformly under `wasmtime`, Node's WASI and a browser
//! WASI shim. The non-portable runtime (fibers/socket HTTP/FFI) is out of scope:
//! a program that references those symbols fails at link with a clear undefined
//! symbol, not silently. Browser UI messaging uses the portable `osprey_web`
//! host ABI below.

use crate::toolchain::{fail, run_tool, tool};
use crate::{find_runtime_lib, scratch_stem};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The canonical wasm target triple (the modern spelling of `wasm32-wasi`).
/// Implements [WASM-TARGET-TRIPLE].
const TRIPLE: &str = "wasm32-wasip1";
/// The `--memory=none` malloc-passthrough C runtime, built by `make -C website wasm`.
/// Implements [WASM-TARGET-RUNTIME].
const RUNTIME_LIB_NONE: &str = "libosprey_runtime_wasm.a";
/// The ARC runtime twin `--memory=arc` links: same portable unit set with the
/// managed allocator and the ARC ownership shims. It is the wasm32 default, so
/// this is the archive a plain `--target=wasm32` build takes. [WASM-TARGET-MEMORY]
const RUNTIME_LIB_ARC: &str = "libosprey_runtime_wasm_arc.a";
/// The managed backend a wasm32 build links when the user left `--memory` off.
const DEFAULT_MEMORY: &str = "arc";
/// Candidate multiarch lib subdir names inside a WASI sysroot, newest first.
const LIB_SUBDIRS: [&str; 2] = ["wasm32-wasip1", "wasm32-wasi"];
/// Source-level browser event entry point. When present it is rooted and
/// exported from the otherwise command-oriented WASI module. [WASM-WEB-ABI]
const WEB_DISPATCH: &str = "osprey_web_dispatch";
/// A flattened project symbol ends in a length-prefixed, hex-encoded source
/// segment. This is the mangled terminal segment for [`WEB_DISPATCH`].
const WEB_DISPATCH_MANGLED_SUFFIX: &str = "_19x6f73707265795f7765625f6469737061746368";
/// What a failed tool spawn suggests installing.
const TOOLCHAIN_HINT: &str = "is the wasm toolchain installed?";

/// Lower `program` to the `.wasm` at `out` with the target's default backend.
/// [WASM-TARGET]
pub(crate) fn build(path: &str, program: &osprey_ast::Program, out: &Path) -> Result<(), ExitCode> {
    build_with_memory(path, program, out, DEFAULT_MEMORY)
}

/// Lower `program` to the `.wasm` at `out` with an explicit memory backend:
/// `arc` links the managed runtime, anything else the malloc passthrough, which
/// is what an explicit `--memory=none` means. [WASM-TARGET-MEMORY]
///
/// # Errors
///
/// Returns the CLI failure code if codegen fails, the toolchain (sysroot /
/// runtime archive / `wasm-ld`) is missing, or clang/`wasm-ld` exits non-zero.
pub(crate) fn build_with_memory(
    path: &str,
    program: &osprey_ast::Program,
    out: &Path,
    memory: &str,
) -> Result<(), ExitCode> {
    crate::target_capabilities::validate(program, "wasm32")
        .map_err(|error| fail(&format!("{path}: {error}")))?;
    let web_dispatch = web_dispatch_export(program);
    let ir = match program_ir(program) {
        Ok(ir) => with_web_dispatch_thunk(&with_entry_thunk(&ir), web_dispatch),
        Err(e) => return Err(fail(&format!("{path}: {e}"))),
    };
    let sysroot = wasi_sysroot().map_err(|e| fail(&e))?;
    let libdir = lib_dir(&sysroot).map_err(|e| fail(&e))?;
    let lib = runtime_lib(memory);
    let archive = find_runtime_lib(lib)
        .ok_or_else(|| fail(&format!("{lib} not found — run `make -C website wasm` to build it")))?;
    let obj = compile_object(&scratch_stem(path), &ir)?;
    link(&obj, &archive, &libdir, out, web_dispatch)
}

/// The runtime archive a memory backend links. [WASM-TARGET-MEMORY]
fn runtime_lib(memory: &str) -> &'static str {
    if memory == "arc" {
        RUNTIME_LIB_ARC
    } else {
        RUNTIME_LIB_NONE
    }
}

/// Browser exports continue using module globals after the WASI entry returns.
pub(crate) fn program_ir(program: &osprey_ast::Program) -> osprey_codegen::Result<String> {
    if web_dispatch_export(program).is_some() {
        osprey_codegen::compile_wasm_library(program)
    } else {
        osprey_codegen::compile_wasm_program(program)
    }
}

/// `--run` for wasm: build to a temp `.wasm`, then execute it under a WASI host.
pub(crate) fn run(path: &str, program: &osprey_ast::Program, memory: &str) -> ExitCode {
    let wasm = std::env::temp_dir().join(format!("{}.wasm", scratch_stem(path)));
    if let Err(code) = build_with_memory(path, program, &wasm, memory) {
        return code;
    }
    run_host(&wasm)
}

/// Append a `__main_void` thunk that calls Osprey's `i32 @main()`. wasi-libc's
/// `crt1-command.o` `_start` calls `__main_void`; defining it here sidesteps the
/// clang/wasi-libc `main`-mangling skew, so the command module runs identically
/// under wasmtime, Node's WASI and a browser shim. [WASM-ENTRY]
fn with_entry_thunk(ir: &str) -> String {
    format!("{ir}\ndefine i32 @__main_void() {{\n  %r = call i32 @main()\n  ret i32 %r\n}}\n")
}

/// Lower target-neutral IR to a wasm object with clang (`-c`, no link).
/// Implements [WASM-TARGET-IR] and the compile half of [WASM-TARGET-LINK].
fn compile_object(stem: &str, ir: &str) -> Result<PathBuf, ExitCode> {
    let ll = std::env::temp_dir().join(format!("{stem}.wasm.ll"));
    let obj = std::env::temp_dir().join(format!("{stem}.wasm.o"));
    if let Err(e) = std::fs::write(&ll, ir.as_bytes()) {
        return Err(fail(&format!("cannot write IR to {}: {e}", ll.display())));
    }
    run_tool(
        &tool("OSPREY_WASM_CC", "clang"),
        &clang_argv(&ll, &obj),
        TOOLCHAIN_HINT,
    )?;
    Ok(obj)
}

/// Link the wasm object + runtime archive + libc into a command module.
/// Implements [WASM-TARGET-LINK].
fn link(
    obj: &Path,
    archive: &str,
    libdir: &Path,
    out: &Path,
    web_dispatch: Option<&str>,
) -> Result<(), ExitCode> {
    run_tool(
        &tool("OSPREY_WASM_LD", "wasm-ld"),
        &link_argv(obj, archive, libdir, out, web_dispatch),
        TOOLCHAIN_HINT,
    )
}

/// clang argv to lower textual IR to a wasm object: target the wasm triple,
/// optimize, suppress the module-triple-override note, compile only.
fn clang_argv(ll: &Path, obj: &Path) -> Vec<String> {
    vec![
        format!("--target={TRIPLE}"),
        "-O2".to_string(),
        "-Wno-override-module".to_string(),
        "-c".to_string(),
        ll.display().to_string(),
        "-o".to_string(),
        obj.display().to_string(),
    ]
}

/// `wasm-ld` argv: the sysroot's `crt1-command.o` `_start`, the program object,
/// the runtime archive (on-demand), and libc from the sysroot lib dir.
/// Exporting `osp_alloc` also roots its on-demand archive member so browser
/// hosts can copy UTF-8 request JSON into wasm memory. [WASM-WEB-ABI]
fn link_argv(
    obj: &Path,
    archive: &str,
    libdir: &Path,
    out: &Path,
    web_dispatch: Option<&str>,
) -> Vec<String> {
    let mut argv = base_link_argv(obj, archive, libdir, out);
    argv.extend(
        web_dispatch
            .is_some()
            .then(|| format!("--export={WEB_DISPATCH}")),
    );
    argv
}

fn base_link_argv(obj: &Path, archive: &str, libdir: &Path, out: &Path) -> Vec<String> {
    vec![
        libdir.join("crt1-command.o").display().to_string(),
        obj.display().to_string(),
        archive.to_string(),
        format!("-L{}", libdir.display()),
        "-lc".to_string(),
        "--export=osp_alloc".to_string(),
        "-o".to_string(),
        out.display().to_string(),
    ]
}

/// Return the actual LLVM/linkage name of the browser dispatcher, if the
/// flattened AST defines it. Single-file programs retain the source spelling;
/// project assembly uses `__osp_<len>x<hex>...` and preserves the source name
/// in the terminal segment. [WASM-WEB-ABI]
fn web_dispatch_export(program: &osprey_ast::Program) -> Option<&str> {
    program.statements.iter().find_map(|statement| {
        let osprey_ast::Stmt::Function { name, .. } = statement else {
            return None;
        };
        (name == WEB_DISPATCH || is_mangled_web_dispatch(name)).then_some(name.as_str())
    })
}

fn is_mangled_web_dispatch(name: &str) -> bool {
    name.starts_with("__osp_") && name.ends_with(WEB_DISPATCH_MANGLED_SUFFIX)
}

/// Add a stable browser ABI symbol in front of a project-mangled dispatcher.
/// The Osprey convention is `fn osprey_web_dispatch(message: string) -> int`;
/// strings are `i8*` and `int` is `i64` in the LLVM ABI. A single-file source
/// already emits the stable name and needs no duplicate definition.
fn with_web_dispatch_thunk(ir: &str, actual: Option<&str>) -> String {
    match actual {
        Some(symbol) if symbol != WEB_DISPATCH => format!(
            "{ir}\ndefine i64 @{WEB_DISPATCH}(i8* %message) {{\n  %r = call i64 @{symbol}(i8* %message)\n  ret i64 %r\n}}\n"
        ),
        _ => ir.to_string(),
    }
}

/// Locate a WASI sysroot: `OSPREY_WASI_SYSROOT` if set, else the first existing
/// of the conventional Homebrew / wasi-sdk / Linux locations.
fn wasi_sysroot() -> Result<PathBuf, String> {
    pick_sysroot(
        std::env::var("OSPREY_WASI_SYSROOT").ok(),
        &sysroot_candidates(),
    )
}

/// The default sysroot search path, in priority order.
fn sysroot_candidates() -> Vec<PathBuf> {
    let mut out = vec![
        PathBuf::from("/opt/homebrew/opt/wasi-libc/share/wasi-sysroot"),
        PathBuf::from("/usr/local/opt/wasi-libc/share/wasi-sysroot"),
        PathBuf::from("/opt/wasi-sdk/share/wasi-sysroot"),
        PathBuf::from("/usr/share/wasi-sysroot"),
    ];
    if let Ok(sdk) = std::env::var("WASI_SDK_PATH") {
        out.push(PathBuf::from(sdk).join("share/wasi-sysroot"));
    }
    if Path::new("/usr/lib/wasm32-wasip1/crt1-command.o").is_file() {
        out.push(PathBuf::from("/usr"));
    }
    out
}

/// Resolve the sysroot: an explicit override (which must exist) wins, else the
/// first existing candidate. Pure given its inputs, so it is unit-tested.
fn pick_sysroot(override_dir: Option<String>, candidates: &[PathBuf]) -> Result<PathBuf, String> {
    if let Some(dir) = override_dir {
        let p = PathBuf::from(dir);
        return if p.is_dir() {
            Ok(p)
        } else {
            Err(format!(
                "OSPREY_WASI_SYSROOT={} is not a directory",
                p.display()
            ))
        };
    }
    candidates
        .iter()
        .find(|p| p.is_dir())
        .cloned()
        .ok_or_else(|| {
            "no WASI sysroot found — install it (e.g. `brew install wasi-libc`) or set \
         OSPREY_WASI_SYSROOT=/path/to/wasi-sysroot"
                .to_string()
        })
}

/// The sysroot's wasm lib dir (holds `crt1-command.o` + `libc.a`), preferring
/// the modern `wasm32-wasip1` over the legacy `wasm32-wasi` name.
fn lib_dir(sysroot: &Path) -> Result<PathBuf, String> {
    LIB_SUBDIRS
        .iter()
        .map(|s| sysroot.join("lib").join(s))
        .find(|p| p.is_dir())
        .ok_or_else(|| format!("no wasm lib dir under {}", sysroot.display()))
}

/// Run a compiled `.wasm` under the first available WASI host (`wasmtime`, or
/// `OSPREY_WASM_RUN`), propagating its exit code. Without one, point the user at
/// a runtime or the browser loader.
fn run_host(wasm: &Path) -> ExitCode {
    let runner = tool("OSPREY_WASM_RUN", "wasmtime");
    match Command::new(&runner).arg(wasm).status() {
        Ok(s) => ExitCode::from(crate::child_exit_code(s)),
        Err(e) => {
            eprintln!("error: could not run {} with {runner}: {e}", wasm.display());
            eprintln!(
                "hint: install wasmtime, or load {} in a browser (see examples/wasm/)",
                wasm.display()
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
