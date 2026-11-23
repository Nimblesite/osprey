//! Spawning the external toolchain a cross-target backend drives (clang,
//! `wasm-ld`, `xcrun`). Shared by the wasm and iOS drivers so an overridable
//! tool name, a failed exit and a missing program are reported one way.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus};
use std::time::Duration;

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

/// The app-library targets that publish a C ABI static archive: every Apple
/// slice, Android ABI and desktop slice. [IOS-TARGET-TRIPLE] [ANDROID-TARGET-TRIPLE]
/// [GPUI-HOST-TARGETS]
pub(crate) fn host_archive_target(target: &str) -> bool {
    crate::ios::Target::parse(target).is_some()
        || crate::android::Target::parse(target).is_some()
        || crate::desktop::Target::parse(target).is_some()
}

/// An app-library target publishes a static archive plus a matching C header,
/// so `-o` must name the archive. `platform` opens the diagnostic.
pub(crate) fn validate_archive_output(out: &Path, platform: &str) -> Result<(), String> {
    if out.extension().and_then(|e| e.to_str()) != Some("a") {
        return Err(format!(
            "{platform} output must end in .a; a matching .h is generated beside it"
        ));
    }
    Ok(())
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

/// [RUN-PARENT-DEATH] Names the process whose death ends a run: a compiled
/// program reads it at start and ends when that process, its parent, is gone.
pub(crate) const RUN_PARENT: &str = "OSPREY_RUN_PARENT";

/// The longest this process waits before looking at its program and runner again.
const RUN_POLL_CEILING: Duration = Duration::from_millis(50);

/// Makes the program `command` starts end with this process.
pub(crate) fn ends_with_this_process(command: &mut Command) {
    let _ = command.env(RUN_PARENT, std::process::id().to_string());
}

/// [RUN-PARENT-DEATH] Runs `command` to completion as a program that ends with
/// this process. When the process that started this one named itself as its
/// runner, the program is ended as soon as that runner is gone.
pub(crate) fn run_watched(mut command: Command) -> std::io::Result<ExitStatus> {
    ends_with_this_process(&mut command);
    let Some(runner) = named_runner() else {
        return command.status();
    };
    let mut child = command.spawn()?;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if runner_gone(runner) {
            child.kill()?;
            return child.wait();
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(RUN_POLL_CEILING);
    }
}

/// The runner this process ends with: its parent, when the parent named itself.
fn named_runner() -> Option<u32> {
    let named = std::env::var(RUN_PARENT).ok()?.parse().ok()?;
    (!runner_gone(named)).then_some(named)
}

/// Whether `runner` is no longer the process that started this one.
#[cfg(unix)]
fn runner_gone(runner: u32) -> bool {
    std::os::unix::process::parent_id() != runner
}

/// No parent is reported here, so no runner is ever watched.
#[cfg(not(unix))]
fn runner_gone(_runner: u32) -> bool {
    true
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
