//! Spawning the external toolchain a cross-target backend drives (clang,
//! `wasm-ld`, `xcrun`). Shared by the wasm and iOS drivers so an overridable
//! tool name, a failed exit and a missing program are reported one way.

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Stdio;
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
    let runner = named_runner();
    if matches!(runner, Runner::Unnamed) {
        return command.status();
    }
    runner_alive()?;
    let mut child = command.spawn()?;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if runner.gone() {
            child.kill()?;
            let _ = child.wait()?;
            return Err(runner.died());
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(RUN_POLL_CEILING);
    }
}

/// The process `OSPREY_RUN_PARENT` named when this one started.
#[derive(Clone, Copy)]
enum Runner {
    /// No runner to watch: none was named, or the name was inherited from a
    /// live process further up that did not start this one.
    Unnamed,
    /// The parent of this process named itself.
    Parent(u32),
    /// The named process had already exited when this one first looked.
    Dead(u32),
}

impl Runner {
    /// Whether the runner is no longer there to end this process's programs.
    fn gone(self) -> bool {
        match self {
            Self::Unnamed => false,
            Self::Parent(runner) => !is_parent(runner),
            Self::Dead(_) => true,
        }
    }

    fn died(self) -> std::io::Error {
        let (Self::Parent(runner) | Self::Dead(runner)) = self else {
            return std::io::Error::other("no runner was named");
        };
        std::io::Error::other(format!(
            "the runner named in {RUN_PARENT} (process {runner}) is gone"
        ))
    }
}

/// The runner this process ends with, decided once: a later look could not tell
/// a parent that died from a name that was only inherited.
fn named_runner() -> Runner {
    static RUNNER: std::sync::OnceLock<Runner> = std::sync::OnceLock::new();
    *RUNNER.get_or_init(|| {
        let named = std::env::var(RUN_PARENT)
            .ok()
            .and_then(|pid| pid.parse().ok());
        match named {
            None => Runner::Unnamed,
            Some(runner) if is_parent(runner) => Runner::Parent(runner),
            Some(runner) if process_exists(runner) => Runner::Unnamed,
            Some(runner) => Runner::Dead(runner),
        }
    })
}

/// [RUN-PARENT-DEATH] Fails once the runner this process was started under is
/// gone: from then on it starts nothing more.
pub(crate) fn runner_alive() -> std::io::Result<()> {
    let runner = named_runner();
    if runner.gone() {
        return Err(runner.died());
    }
    Ok(())
}

/// Whether `runner` is the process that started this one.
#[cfg(unix)]
fn is_parent(runner: u32) -> bool {
    std::os::unix::process::parent_id() == runner
}

/// No parent is reported here, so no runner is ever the parent.
#[cfg(not(unix))]
fn is_parent(_runner: u32) -> bool {
    false
}

/// Whether a process with this id exists. A probe that cannot run says it does,
/// so a host without `kill` watches nothing rather than refusing every run.
#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    let probe = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    probe.map_or(true, |run| {
        run.status.success() || !String::from_utf8_lossy(&run.stderr).contains("No such process")
    })
}

/// The runner watch is POSIX-only. An MSYS `kill` probes its emulated PID
/// namespace, while the Windows compiler passes native process IDs.
#[cfg(not(unix))]
const fn process_exists(_pid: u32) -> bool {
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
