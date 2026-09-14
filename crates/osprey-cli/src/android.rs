//! Android app-logic archives, lowered with the NDK. [ANDROID-TARGET]

use crate::toolchain::{fail, publish, run_tool, write, Scratch};
use crate::{find_runtime_lib, ios_abi, Cli};
use osprey_ast::Program;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Architecture and minimum platform API are inseparable. [ANDROID-TARGET-TRIPLE]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Arm64,
    X64,
}

impl Target {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "android-arm64" => Some(Self::Arm64),
            "android-x64" => Some(Self::X64),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Arm64 => "android-arm64",
            Self::X64 => "android-x64",
        }
    }
    fn triple(self) -> &'static str {
        match self {
            Self::Arm64 => "aarch64-linux-android26",
            Self::X64 => "x86_64-linux-android26",
        }
    }
    fn runtime(self) -> &'static str {
        match self {
            Self::Arm64 => "libosprey_runtime_android_arm64.a",
            Self::X64 => "libosprey_runtime_android_x64.a",
        }
    }
}

/// Unsupported options fail even when only checking a program. [ANDROID-TARGET-OPTIONS]
pub(crate) fn validate(cli: &Cli) -> Result<(), ExitCode> {
    crate::reject_cross_target_options(
        cli,
        "Android",
        Some("an Android host (see examples/mobile/android/)"),
    )
}

pub(crate) fn source(
    program: &Program,
    path: &str,
    target: Target,
) -> Result<(String, String), String> {
    // AAPCS64 leaves bool extension to the callee; x86-64 requires zeroext.
    ios_abi::source(program, path, target.name(), target == Target::X64)
}

/// Bundle generated PIC code and the matching runtime. [ANDROID-TARGET-LINK]
pub(crate) fn build(
    path: &str,
    program: &Program,
    out: &Path,
    target: Target,
) -> Result<(), ExitCode> {
    crate::toolchain::validate_archive_output(out, "Android").map_err(|e| fail(&e))?;
    let (ir, header) = source(program, path, target).map_err(|e| fail(&e))?;
    let bin = ndk_bin().map_err(|e| fail(&e))?;
    let runtime = find_runtime_lib(target.runtime()).ok_or_else(|| {
        fail(&format!(
            "{} not found; run `make _runtime_android`",
            target.runtime()
        ))
    })?;
    let scratch = Scratch::new(path, target.name())?;
    let ll = scratch.path.join("app.ll");
    let obj = scratch.path.join("app.o");
    let archive = scratch.path.join("app.a");
    write(&ll, &ir)?;
    ndk_tool(&bin, "clang", &compile_args(target, &ll, &obj))?;
    ndk_tool(
        &bin,
        "llvm-ar",
        &[
            "qcsL".into(),
            archive.display().to_string(),
            obj.display().to_string(),
            runtime,
        ],
    )?;
    publish(&archive, out, &header)
}

fn compile_args(target: Target, ll: &Path, obj: &Path) -> Vec<String> {
    vec![
        format!("--target={}", target.triple()),
        "-O2".into(),
        "-fPIC".into(),
        "-Wno-override-module".into(),
        "-c".into(),
        ll.display().to_string(),
        "-o".into(),
        obj.display().to_string(),
    ]
}

fn ndk_tool(bin: &Path, name: &str, args: &[String]) -> Result<(), ExitCode> {
    run_tool(
        &bin.join(name).display().to_string(),
        args,
        "install the Android NDK and set ANDROID_NDK_HOME",
    )
}

/// Explicit NDK selection wins; otherwise choose the newest SDK side-by-side installation.
pub(crate) fn ndk_bin() -> Result<PathBuf, String> {
    let ndk = std::env::var_os("ANDROID_NDK_HOME")
        .or_else(|| std::env::var_os("ANDROID_NDK_ROOT"))
        .map(PathBuf::from)
        .or_else(|| sdk_root().and_then(|sdk| newest_directory(&sdk.join("ndk"))))
        .ok_or("Android NDK not found; install it with Android Studio and set ANDROID_NDK_HOME")?;
    let host = match std::env::consts::OS {
        "macos" => "darwin-x86_64",
        "linux" => "linux-x86_64",
        other => return Err(format!("Android NDK host `{other}` is not supported")),
    };
    let bin = ndk.join("toolchains/llvm/prebuilt").join(host).join("bin");
    if !bin.join("clang").is_file() {
        return Err(format!("Android NDK clang not found in {}", bin.display()));
    }
    Ok(bin)
}

fn sdk_root() -> Option<PathBuf> {
    std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home).join(if cfg!(target_os = "macos") {
                    "Library/Android/sdk"
                } else {
                    "Android/Sdk"
                })
            })
        })
}

fn newest_directory(root: &Path) -> Option<PathBuf> {
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .max_by_key(|path| {
            path.file_name().map(|name| {
                name.to_string_lossy()
                    .split('.')
                    .map(|part| part.parse::<u32>().unwrap_or(0))
                    .collect::<Vec<_>>()
            })
        })
}

#[cfg(test)]
#[path = "android_tests.rs"]
mod tests;
