//! iPhone app-logic archives with a generated C interface. [IOS-TARGET]

use crate::ios_abi;
use crate::toolchain::{fail, publish, run_tool, tool, write, Scratch};
use crate::{find_runtime_lib, Cli};
use osprey_ast::Program;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const TOOLCHAIN_HINT: &str = "install Xcode and select it with xcode-select";

/// The SDK, LLVM triple and runtime archive must describe the same platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Device,
    Simulator,
}

impl Target {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "ios" => Some(Self::Device),
            "ios-sim" => Some(Self::Simulator),
            _ => None,
        }
    }

    /// The `--target` value that selects this slice. A generated header and
    /// every diagnostic must name it, so following either one rebuilds the
    /// slice the reader already chose rather than the other one.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Device => "ios",
            Self::Simulator => "ios-sim",
        }
    }

    fn sdk(self) -> &'static str {
        match self {
            Self::Device => "iphoneos",
            Self::Simulator => "iphonesimulator",
        }
    }

    fn triple(self) -> &'static str {
        match self {
            Self::Device => "arm64-apple-ios15.0",
            Self::Simulator => "arm64-apple-ios15.0-simulator",
        }
    }

    fn runtime(self) -> &'static str {
        match self {
            Self::Device => "libosprey_runtime_ios.a",
            Self::Simulator => "libosprey_runtime_ios_sim.a",
        }
    }
}

/// Refuse native-only settings before invoking the cross compiler.
/// Implements [IOS-TARGET-OPTIONS].
pub(crate) fn validate(cli: &Cli) -> Result<(), ExitCode> {
    if let Some(code) = crate::reject_debug_cross_target(cli) {
        return Err(code);
    }
    if cli.memory != "default" {
        return Err(fail(
            "iOS supports --memory=default; other runtime archives are not available",
        ));
    }
    if cli.mode == "--run" {
        return Err(fail("iOS produces an app-logic library; use --compile and call it from a Swift host (see examples/ios/)"));
    }
    Ok(())
}

/// Emit the app's C ABI and LLVM implementation before any toolchain work.
/// Implements [IOS-HOST-ABI] and [IOS-TARGET-ENTRY].
pub(crate) fn source(
    program: &Program,
    path: &str,
    target: Target,
) -> Result<(String, String), String> {
    // Apple ARM64 zero-extends a C bool in both slices.
    ios_abi::source(program, path, target.name(), true)
}

/// Compile an ARM64 static archive, bundling the matching C runtime, plus its
/// sibling C header. The Swift app supplies the executable entry and imports.
/// Implements [IOS-TARGET-LINK].
pub(crate) fn build(
    path: &str,
    program: &Program,
    out: &Path,
    target: Target,
) -> Result<(), ExitCode> {
    validate_output(out).map_err(|e| fail(&e))?;
    let (ir, header) = source(program, path, target).map_err(|e| fail(&e))?;
    let sdk = sdk_path(target)?;
    let runtime = find_runtime_lib(target.runtime()).ok_or_else(|| {
        fail(&format!(
            "{} not found — run `make ios` to build the iOS runtime",
            target.runtime()
        ))
    })?;
    let scratch = Scratch::new(path, target.sdk())?;
    write(&scratch.path.join("app.ll"), &ir)?;
    let obj = scratch.path.join("app.o");
    xcrun(
        target,
        &compile_args(&scratch.path.join("app.ll"), &obj, &sdk, target),
    )?;
    let archive = scratch.path.join("app.a");
    xcrun(target, &archive_args(&obj, &runtime, &archive))?;
    publish(&archive, out, &header)
}

fn validate_output(out: &Path) -> Result<(), String> {
    if out.extension().and_then(|e| e.to_str()) != Some("a") {
        return Err("iOS output must end in .a; a matching .h is generated beside it".to_string());
    }
    Ok(())
}

fn sdk_path(target: Target) -> Result<PathBuf, ExitCode> {
    let runner = tool("OSPREY_XCRUN", "xcrun");
    let output = Command::new(&runner)
        .args(["--sdk", target.sdk(), "--show-sdk-path"])
        .output()
        .map_err(|e| {
            fail(&format!(
                "could not invoke {runner}: {e} — {TOOLCHAIN_HINT}"
            ))
        })?;
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if !output.status.success() || !path.is_dir() {
        return Err(fail(&format!(
            "{} SDK unavailable: {} — {TOOLCHAIN_HINT}",
            target.sdk(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(path)
}

fn xcrun(target: Target, args: &[String]) -> Result<(), ExitCode> {
    let mut invocation = vec!["--sdk".to_string(), target.sdk().to_string()];
    invocation.extend_from_slice(args);
    run_tool(&tool("OSPREY_XCRUN", "xcrun"), &invocation, TOOLCHAIN_HINT)
}

fn compile_args(ll: &Path, obj: &Path, sdk: &Path, target: Target) -> Vec<String> {
    vec![
        "clang".to_string(),
        format!("--target={}", target.triple()),
        "-isysroot".to_string(),
        sdk.display().to_string(),
        "-O2".to_string(),
        "-Wno-override-module".to_string(),
        "-c".to_string(),
        ll.display().to_string(),
        "-o".to_string(),
        obj.display().to_string(),
    ]
}

fn archive_args(obj: &Path, runtime: &str, out: &Path) -> Vec<String> {
    vec![
        "libtool".to_string(),
        "-static".to_string(),
        "-o".to_string(),
        out.display().to_string(),
        obj.display().to_string(),
        runtime.to_string(),
    ]
}

#[cfg(test)]
#[path = "ios_tests.rs"]
mod tests;
