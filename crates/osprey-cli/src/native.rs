//! Native support for the compiler driver.

use crate::executable_cache::{ensure_cached_executable, test_cache_path};
use crate::linking::link_args;
use crate::project::CompilationInput;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Compile `input` natively to a temp binary and execute it inheriting stdio;
/// the child's exit code. Shared by `--run` and the `osprey test` runner
/// [TESTING-CLI-RUN].
pub(crate) fn execute_native(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<u8, ExitCode> {
    let (exe, temporary) = native_executable(input, memory, kind)?;
    let status = Command::new(&exe).status();
    if temporary {
        let _ = std::fs::remove_file(&exe);
    }
    match status {
        Ok(s) => Ok(child_exit_code(s)),
        Err(e) => {
            eprintln!("error: could not run {}: {e}", exe.display());
            Err(ExitCode::FAILURE)
        }
    }
}

pub(super) fn native_executable(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<(PathBuf, bool), ExitCode> {
    if let Some(cached) = test_cache_path(input, memory, kind) {
        ensure_cached_executable(input, memory, kind, &cached)?;
        return Ok((cached, false));
    }
    let exe = std::env::temp_dir().join(format!("{}.out", scratch_stem(input.display_path())));
    build_input(input, &exe, memory, kind)?;
    Ok((exe, true))
}

pub(super) fn build_input(
    input: &CompilationInput,
    exe: &Path,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<(), ExitCode> {
    build_executable(
        input.debug_path(),
        input.program(),
        input.source(),
        exe,
        memory,
        kind,
    )
}

/// Lower to LLVM IR and hand it to clang together with the prebuilt C runtime,
/// producing `exe`.
pub(super) fn build_executable(
    path: &str,
    program: &osprey_ast::Program,
    source: &str,
    exe: &Path,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<(), ExitCode> {
    let ir = compile_ir(path, program, kind).map_err(|error| {
        eprintln!("{path}: {error}");
        ExitCode::FAILURE
    })?;
    let ll = write_ir(path, &ir)?;
    let result = if kind == osprey_debug::BuildKind::Profile {
        build_profile_executable(&ll, &ir, source, exe, memory)
    } else {
        let mut command = ir_driver(&ll, exe, kind);
        let _ = command.args(link_args(&ir, source, memory));
        run_build_step(command, &ll)
    };
    let _ = std::fs::remove_file(&ll);
    result
}

fn write_ir(path: &str, ir: &str) -> Result<PathBuf, ExitCode> {
    let ll = std::env::temp_dir().join(format!("{}.ll", scratch_stem(path)));
    std::fs::write(&ll, ir.as_bytes()).map_err(|error| {
        eprintln!("error: cannot write IR to {}: {error}", ll.display());
        ExitCode::FAILURE
    })?;
    Ok(ll)
}

fn ir_driver(input: &Path, output: &Path, kind: osprey_debug::BuildKind) -> Command {
    let mut command = Command::new(c_compiler());
    let _ = command
        .arg(input)
        .arg("-o")
        .arg(output)
        .arg("-Wno-override-module")
        .arg(opt_flag(kind))
        .args(kind.native_driver_flags());
    command
}

/// Profile builds go `.ll -> .o -> link -> dsymutil` [PROF-BUILD-MODE]: the
/// single-step clang pipeline deletes the temp object that holds the DWARF on
/// macOS, making line-level attribution unrecoverable.
pub(super) fn build_profile_executable(
    ll: &Path,
    ir: &str,
    source: &str,
    exe: &Path,
    memory: &str,
) -> Result<(), ExitCode> {
    let obj = ll.with_extension("o");
    let mut compile = ir_driver(ll, &obj, osprey_debug::BuildKind::Profile);
    let _ = compile.arg("-c");
    let result =
        run_build_step(compile, ll).and_then(|()| link_profile(&obj, ir, source, exe, memory));
    let _ = std::fs::remove_file(&obj);
    result
}

fn link_profile(
    obj: &Path,
    ir: &str,
    source: &str,
    exe: &Path,
    memory: &str,
) -> Result<(), ExitCode> {
    let mut link = Command::new(c_compiler());
    let _ = link
        .arg(obj)
        .arg("-o")
        .arg(exe)
        .args(osprey_debug::BuildKind::Profile.native_driver_flags())
        .args(link_args(ir, source, memory));
    let result = run_build_step(link, obj);
    if result.is_ok() && cfg!(target_os = "macos") {
        // Best-effort: symbols remain available without source lines.
        let _ = Command::new("dsymutil").arg(exe).status();
    }
    result
}

/// Run one compiler/linker step, mapping failure onto the CLI exit contract.
pub(super) fn run_build_step(mut cmd: Command, input: &Path) -> Result<(), ExitCode> {
    let cc = c_compiler();
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => {
            eprintln!("error: {cc} failed to compile {}", input.display());
            Err(ExitCode::FAILURE)
        }
        Err(e) => {
            eprintln!("error: could not invoke {cc}: {e}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// The LLVM optimization level handed to clang when lowering the emitted IR.
/// Defaults to `-O2`; `OSPREY_OPT` overrides it (e.g. `-O0` for fast debug
/// builds, `-O3` for a more aggressive release build). At `-O2`, LLVM can
/// eliminate non-escaping per-operation `Result` allocations. Allocation
/// reclamation is selected independently by `--memory` ([MEM-BACKENDS]).
pub(super) fn compile_ir(
    path: &str,
    program: &osprey_ast::Program,
    kind: osprey_debug::BuildKind,
) -> osprey_codegen::Result<String> {
    if kind.wants_debug_info() {
        return osprey_codegen::compile_program_debug(
            program,
            osprey_codegen::DebugSource::from_path(path),
        );
    }
    if kind == osprey_debug::BuildKind::Coverage {
        return osprey_codegen::compile_program_coverage(program);
    }
    osprey_codegen::compile_program(program)
}

pub(super) fn opt_flag(kind: osprey_debug::BuildKind) -> String {
    kind.opt_flag(
        std::env::var("OSPREY_OPT").unwrap_or_else(|_| "-O2".to_string()),
        std::env::var("OSPREY_DEBUG_OPT").ok(),
    )
}

/// The C compiler/linker driver used to lower the emitted LLVM IR. Defaults to
/// `clang` (the only driver that consumes textual `.ll`); `OSPREY_CC` overrides
/// it — needed where several clangs coexist and the IR/runtime must link with a
/// matching toolchain (e.g. forcing the MinGW clang on Windows so it links the
/// MinGW-built C runtime archive rather than the system MSVC clang).
pub(super) fn c_compiler() -> String {
    std::env::var("OSPREY_CC").unwrap_or_else(|_| "clang".to_string())
}

/// The source file's stem (`demo` for `examples/demo.osp`).
pub(crate) fn stem_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("osprey_out")
        .to_string()
}

/// A process-unique scratch stem, preventing concurrent CLI builds of files
/// named `main` from overwriting each other's temporary IR and executables.
pub(crate) fn scratch_stem(path: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!(
        "{}-{}-{:x}",
        stem_of(path),
        std::process::id(),
        hasher.finish()
    )
}

/// The exit code to propagate for a finished child: its own code when it exited
/// normally, else (Unix) `128 + signal` for a signal death — so a segfaulting
/// program is NOT masked as success (`status.code()` is `None` for a signal).
pub(crate) fn child_exit_code(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128u8.saturating_add(u8::try_from(sig).unwrap_or(0));
        }
    }
    1
}
