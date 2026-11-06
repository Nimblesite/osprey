//! Share allocator-independent compilation, retaining each runtime's own link.
//! Implements [TESTING-NATIVE-CACHE].
use super::{compile_ir, ir_driver, run_build_step, NativeOptions};
use crate::executable_cache::{codegen_cache_path, is_nonempty_file, publish_cached_artifact};
use crate::project::CompilationInput;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn path(input: &CompilationInput, options: NativeOptions<'_>) -> Option<PathBuf> {
    options
        .cacheable()
        .then(|| codegen_cache_path(input, options.kind))
        .flatten()
}

pub(super) fn ir(input: &CompilationInput, options: NativeOptions<'_>) -> Result<String, ExitCode> {
    let cached = path(input, options);
    if let Some(file) = cached.as_deref().filter(|file| is_nonempty_file(file)) {
        return std::fs::read_to_string(file).map_err(|error| io_error(file, &error));
    }
    let program = input.codegen_program(options.kind == osprey_debug::BuildKind::Coverage);
    let ir = compile_ir(input.debug_path(), &program, options.kind).map_err(|error| {
        eprintln!("{}: {error}", input.display_path());
        ExitCode::FAILURE
    })?;
    if let Some(file) = cached {
        let staging = staging(&file);
        std::fs::write(&staging, &ir).map_err(|error| io_error(&staging, &error))?;
        publish_cached_artifact(&staging, &file)?;
    }
    Ok(ir)
}

pub(super) fn object(
    input: &CompilationInput,
    ll: &Path,
    options: NativeOptions<'_>,
) -> Result<Option<PathBuf>, ExitCode> {
    let Some(cached) = path(input, options) else {
        return Ok(None);
    };
    let object = cached.with_extension("o");
    if !is_nonempty_file(&object) {
        let staging = staging(&object);
        let mut command = ir_driver(ll, &staging, options);
        let _ = command.arg("-c");
        if let Err(code) = run_build_step(command, ll) {
            let _ = std::fs::remove_file(&staging);
            return Err(code);
        }
        publish_cached_artifact(&staging, &object)?;
    }
    Ok(Some(object))
}

fn staging(cached: &Path) -> PathBuf {
    cached.with_extension(format!("{}.tmp", std::process::id()))
}

fn io_error(path: &Path, error: &std::io::Error) -> ExitCode {
    eprintln!("error: cannot access cached IR {}: {error}", path.display());
    ExitCode::FAILURE
}
