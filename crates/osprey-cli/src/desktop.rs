//! Desktop app-logic archives for hosts that link an Osprey application into their own process,
//! such as the GPUI host on Linux and Windows. Each is built on its own operating system with the
//! host's `clang` (`OSPREY_CC`) and `llvm-ar` (`OSPREY_AR`) and bundles the host's native runtime,
//! with the same export selection and generated header as the Apple and Android slices; the Mac
//! slice is the `macos` target. [GPUI-HOST-TARGETS]

use crate::toolchain::{fail, publish, run_tool, write, Scratch};
use crate::{find_runtime_lib, ios_abi, Cli};
use osprey_ast::Program;
use std::path::Path;
use std::process::ExitCode;

/// A desktop operating system and architecture, named as `--target` spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    LinuxX64,
    LinuxArm64,
    WindowsX64,
}

impl Target {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "linux-x64" => Some(Self::LinuxX64),
            "linux-arm64" => Some(Self::LinuxArm64),
            "windows-x64" => Some(Self::WindowsX64),
            _ => None,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
            Self::WindowsX64 => "windows-x64",
        }
    }

    /// The operating system and architecture as Rust's `std::env::consts` spell them.
    fn host(self) -> (&'static str, &'static str) {
        match self {
            Self::LinuxX64 => ("linux", "x86_64"),
            Self::LinuxArm64 => ("linux", "aarch64"),
            Self::WindowsX64 => ("windows", "x86_64"),
        }
    }

    fn platform(self) -> &'static str {
        match self {
            Self::LinuxX64 | Self::LinuxArm64 => "Linux",
            Self::WindowsX64 => "Windows",
        }
    }

    /// x86-64 requires a C `bool` zero-extended by the caller; `AArch64` leaves it to the callee.
    fn zero_extends_bool(self) -> bool {
        matches!(self, Self::LinuxX64 | Self::WindowsX64)
    }
}

/// Native-only options fail before any work, and a slice is built only on its own system, whose
/// native runtime it bundles.
pub(crate) fn validate(cli: &Cli, target: Target) -> Result<(), ExitCode> {
    crate::reject_cross_target_options(
        cli,
        target.platform(),
        Some("a desktop host such as hosts/gpui"),
    )?;
    if cli.mode == "--check" {
        return Ok(());
    }
    on_host(target, std::env::consts::OS, std::env::consts::ARCH).map_err(|error| fail(&error))
}

/// Whether `target` can be built on the system `os`/`arch`.
pub(crate) fn on_host(target: Target, os: &str, arch: &str) -> Result<(), String> {
    if target.host() == (os, arch) {
        Ok(())
    } else {
        Err(format!(
            "--target={} builds on {} {}, and this is {os} {arch}; build it there, where its native runtime is",
            target.name(),
            target.platform(),
            target.host().1
        ))
    }
}

/// The application's C ABI and LLVM implementation.
pub(crate) fn source(
    program: &Program,
    path: &str,
    target: Target,
    scope: &ios_abi::ExportScope,
) -> Result<(String, String), String> {
    ios_abi::source(
        program,
        path,
        target.name(),
        target.zero_extends_bool(),
        scope,
    )
}

/// Compiles position-independent code and bundles it with the native runtime into one archive,
/// with its C header beside it.
pub(crate) fn build(
    cli: &Cli,
    path: &str,
    program: &Program,
    out: &Path,
    target: Target,
    scope: &ios_abi::ExportScope,
) -> Result<(), ExitCode> {
    validate(cli, target)?;
    if target == Target::WindowsX64 && is_dll(out) {
        return build_dll(cli, path, program, out, scope);
    }
    crate::toolchain::validate_archive_output(out, target.platform()).map_err(|e| fail(&e))?;
    let (ir, header) = source(program, path, target, scope).map_err(|e| fail(&e))?;
    let runtime = runtime_archive(cli, target)?;
    let scratch = Scratch::new(path, target.name())?;
    let ll = scratch.path.join("app.ll");
    let obj = scratch.path.join("app.o");
    let archive = scratch.path.join("app.a");
    write(&ll, &ir)?;
    run_tool(&tool("OSPREY_CC", "clang"), &compile_args(&ll, &obj), HINT)?;
    run_tool(
        &tool("OSPREY_AR", "llvm-ar"),
        &[
            "qcsL".into(),
            archive.display().to_string(),
            obj.display().to_string(),
            runtime,
        ],
        HINT,
    )?;
    publish(&archive, out, &header)
}

/// The native runtime this system's slice bundles, for the chosen memory backend.
fn runtime_archive(cli: &Cli, target: Target) -> Result<String, ExitCode> {
    let runtime_name = crate::linking::runtime_name("libfiber_runtime.a", &cli.memory);
    let runtime = find_runtime_lib(&runtime_name).ok_or_else(|| {
        fail(&format!(
            "{runtime_name} not found; run `make` to build the {} runtime",
            target.platform()
        ))
    })?;
    crate::linking::fresh_runtime(&runtime).map_err(|e| fail(&e))
}

/// Whether `out` names a Windows DLL rather than an archive. [WINUI-HOST-DLL]
fn is_dll(out: &Path) -> bool {
    out.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
}

/// Links the application and the native runtime into one DLL a host loads at run time, with its
/// C header beside it. Every import becomes a slot the host binds. [WINUI-HOST-DLL]
fn build_dll(
    cli: &Cli,
    path: &str,
    program: &Program,
    out: &Path,
    scope: &ios_abi::ExportScope,
) -> Result<(), ExitCode> {
    let target = Target::WindowsX64;
    let (ir, abi) = ios_abi::source_with_abi(program, path, target.name(), true, scope)
        .map_err(|e| fail(&e))?;
    let (ir, slotted) = ios_abi::windows_dll::adapt(&ir, &abi, true).map_err(|e| fail(&e))?;
    let header = ios_abi::windows_dll::header(&abi.exports, &slotted, path, target.name());
    let runtime = runtime_archive(cli, target)?;
    let scratch = Scratch::new(path, target.name())?;
    let ll = scratch.path.join("app.ll");
    let dll = scratch.path.join("app.dll");
    write(&ll, &ir)?;
    run_tool(
        &tool("OSPREY_CC", "clang"),
        &dll_args(&ll, &runtime, &dll),
        HINT,
    )?;
    publish(&dll, out, &header)
}

/// One self-contained DLL: the runtime and winpthreads are linked statically, so it needs only
/// what every Windows system has. [WINUI-HOST-DEPLOY]
pub(crate) fn dll_args(ll: &Path, runtime: &str, dll: &Path) -> Vec<String> {
    let mut args: Vec<String> = ["-shared", "-O2", "-Wno-override-module"]
        .map(String::from)
        .to_vec();
    args.extend([ll.display().to_string(), runtime.to_owned()]);
    args.extend(["-o".to_owned(), dll.display().to_string()]);
    args.extend(["-static", "-lpthread", "-lws2_32"].map(String::from));
    args
}

const HINT: &str = "install clang and llvm-ar (LLVM), or set OSPREY_CC and OSPREY_AR";

fn tool(variable: &str, default: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| default.to_owned())
}

pub(crate) fn compile_args(ll: &Path, obj: &Path) -> Vec<String> {
    vec![
        "-O2".into(),
        "-fPIC".into(),
        "-Wno-override-module".into(),
        "-c".into(),
        ll.display().to_string(),
        "-o".into(),
        obj.display().to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{on_host, Target};

    /// Every desktop target spelling round-trips, and other targets are not desktop slices.
    #[test]
    fn desktop_targets_parse_by_name() {
        for name in ["linux-x64", "linux-arm64", "windows-x64"] {
            assert_eq!(Target::parse(name).map(Target::name), Some(name));
        }
        for other in ["macos", "native", "android-x64", "linux", "windows"] {
            assert_eq!(Target::parse(other), None, "{other}");
        }
    }

    /// A slice builds only on its own system, and the refusal names where to build it.
    #[test]
    fn a_slice_builds_only_on_its_own_system() {
        assert!(on_host(Target::LinuxX64, "linux", "x86_64").is_ok());
        assert!(on_host(Target::LinuxArm64, "linux", "aarch64").is_ok());
        assert!(on_host(Target::WindowsX64, "windows", "x86_64").is_ok());
        let refused = on_host(Target::WindowsX64, "macos", "aarch64").unwrap_err();
        assert!(refused.contains("builds on Windows x86_64"), "{refused}");
        assert!(on_host(Target::LinuxX64, "linux", "aarch64").is_err());
    }

    /// x86-64 hosts need the caller to zero-extend a C bool; `AArch64` does not.
    #[test]
    fn bool_extension_follows_the_architecture() {
        assert!(Target::LinuxX64.zero_extends_bool());
        assert!(Target::WindowsX64.zero_extends_bool());
        assert!(!Target::LinuxArm64.zero_extends_bool());
    }
}
