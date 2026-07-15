//! WebAssembly (`wasm32-wasip1`) backend driver. [WASM-TARGET]
//!
//! Osprey codegen already emits target-agnostic textual LLVM IR (no triple or
//! datalayout; `int` is `i64`; pointers round-trip through `i64`, which is safe
//! on wasm32 since addresses fit in 32 bits). This module turns that IR into a
//! browser-ready `.wasm`:
//!
//!   1. append a `__main_void` entry thunk (see [`with_entry_thunk`]);
//!   2. clang lowers the IR to a wasm object — `-c` only, so clang never pulls
//!      in the `libclang_rt.builtins-wasm32.a` that a stock Homebrew/apt LLVM
//!      doesn't ship (a basic program needs no compiler-rt builtins);
//!   3. `wasm-ld` links `crt1-command.o` + that object + the portable wasm
//!      runtime archive (`libosprey_runtime_wasm.a`, `make wasm`) + libc.
//!
//! Driving `wasm-ld` directly — rather than the clang link driver — is what lets
//! step 3 control the exact link line and sidestep the missing builtins archive.
//! The result is a command module (`_start` → `__wasm_call_ctors` →
//! `__main_void`) that runs uniformly under `wasmtime`, Node's WASI and a browser
//! WASI shim. The non-portable runtime (fibers/socket HTTP/FFI) is out of scope:
//! a program that references those symbols fails at link with a clear undefined
//! symbol, not silently. Browser UI messaging uses the portable `osprey_web`
//! host ABI below.

use crate::{find_runtime_lib, scratch_stem};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The canonical wasm target triple (the modern spelling of `wasm32-wasi`).
const TRIPLE: &str = "wasm32-wasip1";
/// The browser-portable C-runtime archive, built by `make wasm`.
const RUNTIME_LIB: &str = "libosprey_runtime_wasm.a";
/// Candidate multiarch lib subdir names inside a WASI sysroot, newest first.
const LIB_SUBDIRS: [&str; 2] = ["wasm32-wasip1", "wasm32-wasi"];
/// Source-level browser event entry point. When present it is rooted and
/// exported from the otherwise command-oriented WASI module. [WASM-WEB-ABI]
const WEB_DISPATCH: &str = "osprey_web_dispatch";
/// A flattened project symbol ends in a length-prefixed, hex-encoded source
/// segment. This is the mangled terminal segment for [`WEB_DISPATCH`].
const WEB_DISPATCH_MANGLED_SUFFIX: &str = "_19x6f73707265795f7765625f6469737061746368";

/// Lower `program` to the `.wasm` at `out`. [WASM-TARGET]
///
/// # Errors
///
/// Returns the CLI failure code if codegen fails, the toolchain (sysroot /
/// runtime archive / `wasm-ld`) is missing, or clang/`wasm-ld` exits non-zero.
pub(crate) fn build(path: &str, program: &osprey_ast::Program, out: &Path) -> Result<(), ExitCode> {
    let web_dispatch = web_dispatch_export(program);
    let ir = match osprey_codegen::compile_program(program) {
        Ok(ir) => with_web_dispatch_thunk(&with_entry_thunk(&ir), web_dispatch),
        Err(e) => return Err(fail(&format!("{path}: {e}"))),
    };
    let sysroot = wasi_sysroot().map_err(|e| fail(&e))?;
    let libdir = lib_dir(&sysroot).map_err(|e| fail(&e))?;
    let archive = find_runtime_lib(RUNTIME_LIB).ok_or_else(|| {
        fail(&format!(
            "{RUNTIME_LIB} not found — run `make wasm` to build it"
        ))
    })?;
    let obj = compile_object(&scratch_stem(path), &ir)?;
    link(&obj, &archive, &libdir, out, web_dispatch)
}

/// `--run` for wasm: build to a temp `.wasm`, then execute it under a WASI host.
pub(crate) fn run(path: &str, program: &osprey_ast::Program) -> ExitCode {
    let wasm = std::env::temp_dir().join(format!("{}.wasm", scratch_stem(path)));
    if let Err(code) = build(path, program, &wasm) {
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

/// Lower the IR text to a wasm object with clang (`-c`, no link).
fn compile_object(stem: &str, ir: &str) -> Result<PathBuf, ExitCode> {
    let ll = std::env::temp_dir().join(format!("{stem}.wasm.ll"));
    let obj = std::env::temp_dir().join(format!("{stem}.wasm.o"));
    if let Err(e) = std::fs::write(&ll, ir.as_bytes()) {
        return Err(fail(&format!("cannot write IR to {}: {e}", ll.display())));
    }
    run_tool(&tool("OSPREY_WASM_CC", "clang"), &clang_argv(&ll, &obj))?;
    Ok(obj)
}

/// Link the wasm object + runtime archive + libc into a command module.
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

/// The tool to invoke for `env`, defaulting to `default` when unset.
fn tool(env: &str, default: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| default.to_string())
}

/// Spawn `prog args`, mapping a non-zero exit or spawn failure to a CLI failure.
fn run_tool(prog: &str, args: &[String]) -> Result<(), ExitCode> {
    match Command::new(prog).args(args).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(fail(&format!("{prog} failed"))),
        Err(e) => Err(fail(&format!(
            "could not invoke {prog}: {e} — is the wasm toolchain installed?"
        ))),
    }
}

/// Print a wasm build error and yield the failure exit code.
fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that read/write process-global toolchain env vars
    /// (`OSPREY_WASM_*`, `*_SYSROOT`) so they neither race each other nor the
    /// end-to-end build below — env is shared across the parallel test threads.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    const WEB_ABI_SOURCE: &str = "namespace app;\n\
         extern fn osprey_web_render(message: string) -> int\n\
         extern fn osprey_web_command(message: string) -> int\n\
         fn osprey_web_dispatch(message: string) -> int = 0\n\
         fn main() = {\n\
           let rendered = osprey_web_render(\"boot\")\n\
           osprey_web_command(\"ready\")\n\
         }\n";

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn assemble_web_program(source: &str, root: &str) -> osprey_ast::Program {
        osprey_project::assemble(
            &osprey_project::ProjectConfig::for_root(Path::new(root)),
            &[osprey_project::SourceFile {
                path: PathBuf::from("client.osp"),
                flavor: osprey_syntax::Flavor::Default,
                source: source.to_string(),
                program: osprey_syntax::parse_program(source).program,
            }],
        )
        .expect("assemble web project")
        .program
    }

    #[test]
    fn entry_thunk_wraps_main_for_the_wasi_start_path() {
        let out = with_entry_thunk("define i32 @main() {\n  ret i32 0\n}\n");
        assert!(out.contains("define i32 @main()"), "original main kept");
        assert!(out.contains("define i32 @__main_void()"), "thunk added");
        assert!(out.contains("call i32 @main()"), "thunk calls main");
    }

    #[test]
    fn web_dispatch_thunk_stabilizes_a_project_mangled_symbol() {
        let mangled = format!("__osp_3x617070{WEB_DISPATCH_MANGLED_SUFFIX}");
        let out = with_web_dispatch_thunk("; module", Some(&mangled));
        assert!(out.contains("define i64 @osprey_web_dispatch(i8* %message)"));
        assert!(out.contains(&format!("call i64 @{mangled}(i8* %message)")));
        assert!(out.contains("ret i64 %r"));

        let unchanged = with_web_dispatch_thunk("; module", Some(WEB_DISPATCH));
        assert_eq!(unchanged, "; module", "source spelling needs no thunk");
    }

    #[test]
    fn compile_object_reports_an_unwritable_ir_path() {
        // A stem pointing through a directory that does not exist makes the
        // temp `.ll` write fail, so compile_object returns the CLI failure code
        // before it ever needs a real clang — the toolchain-free error path.
        let stem = format!("osprey_missing_dir_{}/ir", std::process::id());
        assert!(compile_object(&stem, "; ir").is_err());
    }

    #[test]
    fn clang_argv_targets_wasm_and_compiles_only() {
        let argv = clang_argv(Path::new("/tmp/p.ll"), Path::new("/tmp/p.o"));
        assert!(argv.iter().any(|a| a == "--target=wasm32-wasip1"));
        assert!(argv.iter().any(|a| a == "-c"), "no link step");
        assert!(argv.iter().any(|a| a == "-Wno-override-module"));
        // output is the requested object path, after `-o`.
        let oi = argv.iter().position(|a| a == "-o").expect("has -o");
        assert_eq!(argv.get(oi + 1).map(String::as_str), Some("/tmp/p.o"));
    }

    #[test]
    fn link_argv_uses_crt1_runtime_archive_and_libc() {
        let argv = link_argv(
            Path::new("/tmp/p.o"),
            "/lib/libosprey_runtime_wasm.a",
            Path::new("/sys/lib/wasm32-wasip1"),
            Path::new("/out/p.wasm"),
            None,
        );
        assert!(argv.iter().any(|a| a.ends_with("crt1-command.o")));
        assert!(argv.iter().any(|a| a == "/lib/libosprey_runtime_wasm.a"));
        assert!(argv.iter().any(|a| a == "-L/sys/lib/wasm32-wasip1"));
        assert!(argv.iter().any(|a| a == "-lc"));
        assert!(argv.iter().any(|a| a == "--export=osp_alloc"));
        assert!(!argv.iter().any(|a| a.contains(WEB_DISPATCH)));
        let oi = argv.iter().position(|a| a == "-o").expect("has -o");
        assert_eq!(argv.get(oi + 1).map(String::as_str), Some("/out/p.wasm"));
    }

    #[test]
    fn link_argv_exports_the_discovered_web_dispatch_symbol() {
        let mangled = format!("__osp_3x617070{WEB_DISPATCH_MANGLED_SUFFIX}");
        let argv = link_argv(
            Path::new("/tmp/p.o"),
            "/lib/libosprey_runtime_wasm.a",
            Path::new("/sys/lib/wasm32-wasip1"),
            Path::new("/out/p.wasm"),
            Some(&mangled),
        );
        assert!(argv.iter().any(|a| a == "--export=osprey_web_dispatch"));
        assert!(!argv.iter().any(|a| a == &format!("--export={mangled}")));
    }

    #[test]
    fn finds_source_and_project_mangled_web_dispatch_functions() {
        let source =
            osprey_syntax::parse_program("fn osprey_web_dispatch(message: string) -> int = 0\n")
                .program;
        assert_eq!(web_dispatch_export(&source), Some(WEB_DISPATCH));

        let project_source = "namespace app;\nfn osprey_web_dispatch(message: string) -> int = 0\n";
        let project = assemble_web_program(project_source, "/tmp/web-dispatch-project");
        let mangled = web_dispatch_export(&project).expect("find flattened dispatcher");
        assert_ne!(mangled, WEB_DISPATCH);
        assert!(mangled.starts_with("__osp_"));
        assert!(mangled.ends_with(WEB_DISPATCH_MANGLED_SUFFIX));

        let ordinary =
            osprey_syntax::parse_program("fn dispatch(message: string) = message\n").program;
        assert_eq!(web_dispatch_export(&ordinary), None);
    }

    #[test]
    fn tool_falls_back_to_default_when_env_unset() {
        assert_eq!(
            tool("OSPREY_WASM_CC_DEFINITELY_UNSET_XYZ", "clang"),
            "clang"
        );
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("osprey_wasm_test_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&p).expect("mk test dir");
        p
    }

    #[test]
    fn pick_sysroot_prefers_existing_override_then_candidates() {
        let real = unique_dir("sysroot");
        // explicit override that exists wins.
        assert_eq!(
            pick_sysroot(Some(real.display().to_string()), &[]).expect("override ok"),
            real
        );
        // a non-directory override is an error.
        assert!(pick_sysroot(
            Some("/no/such/dir/xyz".to_string()),
            std::slice::from_ref(&real)
        )
        .is_err());
        // no override: first existing candidate is chosen.
        let missing = PathBuf::from("/no/such/dir/abc");
        assert_eq!(
            pick_sysroot(None, &[missing, real.clone()]).expect("candidate ok"),
            real
        );
        // nothing exists: error.
        assert!(pick_sysroot(None, &[PathBuf::from("/no/such/dir/q")]).is_err());
    }

    #[test]
    fn lib_dir_picks_wasip1_then_wasi_else_errors() {
        let sysroot = unique_dir("libdir");
        assert!(lib_dir(&sysroot).is_err(), "no lib subdir yet");
        let want = sysroot.join("lib").join("wasm32-wasip1");
        std::fs::create_dir_all(&want).expect("mk wasip1");
        assert_eq!(lib_dir(&sysroot).expect("found"), want);
    }

    #[test]
    fn run_tool_reports_success_failure_and_a_missing_program() {
        // A program that exits 0 succeeds; a non-zero exit and a missing program
        // are both mapped to a CLI failure (exercising `run_tool` + `fail`).
        assert!(run_tool("true", &[]).is_ok());
        assert!(run_tool("false", &[]).is_err());
        assert!(run_tool("/no/such/tool/osprey_xyz", &[]).is_err());
    }

    #[test]
    fn compile_object_lowers_textual_ir_with_clang() {
        // Requires clang (present wherever `make test` runs); a tiny valid module
        // exercises the write-IR + `clang -c` lowering path end to end.
        if Command::new(tool("OSPREY_WASM_CC", "clang"))
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping compile_object test: clang absent");
            return;
        }
        let obj = compile_object("osprey_cli_unit", "define i32 @main() {\n  ret i32 0\n}\n")
            .expect("clang lowers trivial IR to a wasm object");
        assert!(obj.exists(), "object emitted at {}", obj.display());
    }

    #[test]
    fn link_invokes_the_linker_and_maps_its_exit_code() {
        let _g = lock_env();
        let prev = std::env::var("OSPREY_WASM_LD").ok();
        // A stand-in "linker" that ignores its args and exits 0 → link succeeds.
        std::env::set_var("OSPREY_WASM_LD", "true");
        let ok = link(
            Path::new("/tmp/o.o"),
            "/tmp/rt.a",
            Path::new("/tmp/libdir"),
            Path::new("/tmp/out.wasm"),
            None,
        );
        // A stand-in that exits non-zero → link reports a CLI failure.
        std::env::set_var("OSPREY_WASM_LD", "false");
        let err = link(
            Path::new("/tmp/o.o"),
            "/tmp/rt.a",
            Path::new("/tmp/libdir"),
            Path::new("/tmp/out.wasm"),
            None,
        );
        match prev {
            Some(v) => std::env::set_var("OSPREY_WASM_LD", v),
            None => std::env::remove_var("OSPREY_WASM_LD"),
        }
        assert!(ok.is_ok(), "linker exit 0 => ok");
        assert!(err.is_err(), "linker non-zero => failure");
    }

    #[test]
    fn run_host_uses_the_configured_runner_and_handles_a_missing_one() {
        let _g = lock_env();
        let prev = std::env::var("OSPREY_WASM_RUN").ok();
        let wasm = std::env::temp_dir().join("osprey_run_host_unit.wasm");
        let _ = std::fs::write(&wasm, b"\0asm");
        // A present runner that ignores args and exits 0 → success-code path.
        std::env::set_var("OSPREY_WASM_RUN", "true");
        let _ok = run_host(&wasm);
        // A missing runner → the spawn-error hint path.
        std::env::set_var("OSPREY_WASM_RUN", "/no/such/runner/osprey_xyz");
        let _err = run_host(&wasm);
        match prev {
            Some(v) => std::env::set_var("OSPREY_WASM_RUN", v),
            None => std::env::remove_var("OSPREY_WASM_RUN"),
        }
    }

    #[test]
    fn sysroot_candidates_includes_wasi_sdk_path_when_set() {
        let _g = lock_env();
        let prev = std::env::var("WASI_SDK_PATH").ok();
        std::env::set_var("WASI_SDK_PATH", "/opt/osprey-test-wasi-sdk");
        let cands = sysroot_candidates();
        match prev {
            Some(v) => std::env::set_var("WASI_SDK_PATH", v),
            None => std::env::remove_var("WASI_SDK_PATH"),
        }
        assert!(
            cands.iter().any(|p| p.ends_with("wasi-sysroot")
                && p.to_string_lossy().contains("osprey-test-wasi-sdk")),
            "WASI_SDK_PATH contributes a sysroot candidate: {cands:?}"
        );
    }

    #[test]
    fn build_fails_cleanly_when_the_sysroot_override_is_bogus() {
        let _g = lock_env();
        let prev = std::env::var("OSPREY_WASI_SYSROOT").ok();
        std::env::set_var("OSPREY_WASI_SYSROOT", "/no/such/wasi/sysroot/xyz");
        let program = osprey_syntax::parse_program("let n = 1\nprint(\"${n}\")\n").program;
        let out = std::env::temp_dir().join("osprey_build_err_unit.wasm");
        let res = build("unit.osp", &program, &out);
        match prev {
            Some(v) => std::env::set_var("OSPREY_WASI_SYSROOT", v),
            None => std::env::remove_var("OSPREY_WASI_SYSROOT"),
        }
        assert!(res.is_err(), "a bogus sysroot override must fail the build");
    }

    /// Drive the whole build+run driver with the real tools stubbed by `true`
    /// (exit 0): codegen → entry thunk → sysroot → lib dir → runtime archive →
    /// `compile_object` → `link` → `run_host`. This exercises the orchestration
    /// deterministically on any host (no clang/wasm-ld/wasmtime needed); the
    /// genuine toolchain path is covered separately by the e2e test below and
    /// the CI `wasm` job.
    #[test]
    fn build_and_run_drive_the_full_driver_with_stub_tools() {
        let _g = lock_env();
        let prev = [
            (
                "OSPREY_WASI_SYSROOT",
                std::env::var("OSPREY_WASI_SYSROOT").ok(),
            ),
            ("OSPREY_WASM_CC", std::env::var("OSPREY_WASM_CC").ok()),
            ("OSPREY_WASM_LD", std::env::var("OSPREY_WASM_LD").ok()),
            ("OSPREY_WASM_RUN", std::env::var("OSPREY_WASM_RUN").ok()),
        ];

        // A fake sysroot whose lib dir exists so `lib_dir` resolves.
        let sysroot = unique_dir("full_sysroot");
        std::fs::create_dir_all(sysroot.join("lib").join("wasm32-wasip1")).expect("mk libdir");
        // A discoverable runtime archive next to the test binary (the current-exe
        // search root) so `find_runtime_lib` succeeds without `make wasm`.
        let archive = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join(RUNTIME_LIB)));
        if let Some(a) = &archive {
            std::fs::write(a, b"").expect("write stub archive");
        }
        std::env::set_var("OSPREY_WASI_SYSROOT", sysroot.display().to_string());
        for k in ["OSPREY_WASM_CC", "OSPREY_WASM_LD", "OSPREY_WASM_RUN"] {
            std::env::set_var(k, "true");
        }

        let program = osprey_syntax::parse_program("let n = 1\nprint(\"${n}\")\n").program;
        let out = std::env::temp_dir().join("osprey_full_driver_unit.wasm");
        let built = build("unit.osp", &program, &out);
        let _ran = run("unit.osp", &program);

        if let Some(a) = &archive {
            let _ = std::fs::remove_file(a);
        }
        for (k, v) in prev {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
        assert!(built.is_ok(), "stubbed toolchain drives a clean build");
    }

    // End-to-end build+run, exercised only where the wasm toolchain is present
    // (the dev machine and the CI `ci` job, which installs lld + a WASI sysroot
    // and `make wasm` before `make test`). Skipped elsewhere so the unit suite
    // stays toolchain-free.
    #[test]
    fn build_and_run_end_to_end_when_toolchain_present() {
        let _g = lock_env();
        let have_ld = std::env::var("OSPREY_WASM_LD").is_ok()
            || Command::new("wasm-ld").arg("--version").output().is_ok();
        let have_sysroot = wasi_sysroot().is_ok();
        let have_rt = find_runtime_lib(RUNTIME_LIB).is_some();
        if !(have_ld && have_sysroot && have_rt) {
            eprintln!("skipping wasm e2e: toolchain or runtime archive absent");
            return;
        }
        let src = "let n = 21\nprint(\"answer=${n + n}\")\n";
        let program = osprey_syntax::parse_program(src).program;
        let out = std::env::temp_dir().join(format!("osprey_wasm_e2e_{}.wasm", std::process::id()));
        build("e2e.osp", &program, &out).expect("wasm build");
        let bytes = std::fs::read(&out).expect("read wasm");
        assert!(bytes.starts_with(b"\0asm"), "wasm magic header");
        build_web_abi_fixture();
    }

    fn build_web_abi_fixture() {
        let web = assemble_web_program(WEB_ABI_SOURCE, "/tmp/web-abi-e2e");
        let actual = web_dispatch_export(&web).expect("flattened web dispatcher");
        assert_ne!(actual, WEB_DISPATCH);
        let web_out =
            std::env::temp_dir().join(format!("osprey_wasm_web_e2e_{}.wasm", std::process::id()));
        build("web-e2e.osp", &web, &web_out).expect("web ABI wasm build");
        let web_bytes = std::fs::read(&web_out).expect("read web wasm");
        assert_web_abi_names(&web_bytes);
    }

    fn assert_web_abi_names(web_bytes: &[u8]) {
        for expected in [
            "osprey_web_dispatch",
            "osp_alloc",
            "osprey_web",
            "render",
            "command",
        ] {
            assert!(
                web_bytes
                    .windows(expected.len())
                    .any(|window| window == expected.as_bytes()),
                "web module contains import/export name {expected}"
            );
        }
    }
}
