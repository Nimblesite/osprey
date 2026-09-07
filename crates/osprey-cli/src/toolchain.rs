//! Spawning the external toolchain a cross-target backend drives (clang,
//! `wasm-ld`, `xcrun`). Shared by the wasm and iOS drivers so an overridable
//! tool name, a failed exit and a missing program are reported one way.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The tool to invoke for `env`, defaulting to `default` when unset.
pub(crate) fn tool(env: &str, default: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| default.to_string())
}

/// Spawn `prog args`, mapping a non-zero exit or spawn failure to a CLI
/// failure. `hint` names the toolchain a spawn failure suggests installing.
pub(crate) fn run_tool(prog: &str, args: &[String], hint: &str) -> Result<(), ExitCode> {
    match Command::new(prog).args(args).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(fail(&format!("{prog} failed"))),
        Err(e) => Err(fail(&format!("could not invoke {prog}: {e} — {hint}"))),
    }
}

/// Print a backend build error and yield the failure exit code.
pub(crate) fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_falls_back_to_default_when_env_unset() {
        assert_eq!(
            tool("OSPREY_WASM_CC_DEFINITELY_UNSET_XYZ", "clang"),
            "clang"
        );
    }

    #[test]
    fn run_tool_reports_success_failure_and_a_missing_program() {
        // A program that exits 0 succeeds; a non-zero exit and a missing program
        // are both mapped to a CLI failure (exercising `run_tool` + `fail`).
        assert!(run_tool("true", &[], "install it").is_ok());
        assert!(run_tool("false", &[], "install it").is_err());
        assert!(run_tool("/no/such/tool/osprey_xyz", &[], "install it").is_err());
    }
}

pub(crate) fn write(path: &Path, contents: &str) -> Result<(), ExitCode> {
    std::fs::write(path, contents)
        .map_err(|e| fail(&format!("cannot write {}: {e}", path.display())))
}

pub(crate) fn publish(archive: &Path, out: &Path, header: &str) -> Result<(), ExitCode> {
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| fail(&format!("cannot create {}: {e}", parent.display())))?;
    }
    let _ = std::fs::copy(archive, out)
        .map_err(|e| fail(&format!("cannot write {}: {e}", out.display())))?;
    write(&out.with_extension("h"), header)
}

/// Remove intermediate IR and objects even when clang or libtool fails.
pub(crate) struct Scratch {
    pub(crate) path: PathBuf,
}

impl Scratch {
    pub(crate) fn new(source: &str, target: &str) -> Result<Self, ExitCode> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{}-{}-{sequence}",
            crate::scratch_stem(source),
            target
        ));
        std::fs::create_dir(&path)
            .map_err(|e| fail(&format!("cannot create {}: {e}", path.display())))?;
        Ok(Self { path })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
