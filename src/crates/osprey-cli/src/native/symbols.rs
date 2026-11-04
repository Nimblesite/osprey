//! Keep native object files alive until the platform has collected DWARF.
//! Implements [DEBUGGER-BUILD-OPTIONS] and [PROF-BUILD-MODE].
use super::{c_compiler, ir_driver, link_args, run_build_step, NativeOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub(super) fn build(
    ll: &Path,
    ir: &str,
    source: &str,
    exe: &Path,
    options: NativeOptions<'_>,
) -> Result<(), ExitCode> {
    let object = sidecar(ll, ".o");
    let mut compile = ir_driver(ll, &object, options);
    let _ = compile.arg("-c");
    let result = run_build_step(compile, ll)
        .and_then(|()| link(&object, ir, source, exe, options))
        .and_then(|()| collect(exe));
    let _ = std::fs::remove_file(object);
    result
}

fn link(
    object: &Path,
    ir: &str,
    source: &str,
    exe: &Path,
    options: NativeOptions<'_>,
) -> Result<(), ExitCode> {
    let mut command = Command::new(c_compiler());
    let _ = command
        .arg(object)
        .arg("-o")
        .arg(exe)
        .args(options.kind.native_driver_flags())
        .args(link_args(ir, source, options.memory));
    run_build_step(command, object)
}

fn collect(exe: &Path) -> Result<(), ExitCode> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    match Command::new("dsymutil").arg(exe).status() {
        Ok(status) if status.success() => Ok(()),
        outcome => {
            eprintln!(
                "error: dsymutil failed to preserve debug symbols for {}: {outcome:?}",
                exe.display()
            );
            Err(ExitCode::FAILURE)
        }
    }
}

pub(super) fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

pub(crate) fn remove_temporary(exe: &Path) {
    let _ = std::fs::remove_file(exe);
    if cfg!(target_os = "macos") {
        let _ = std::fs::remove_dir_all(sidecar(exe, ".dSYM"));
    }
}
