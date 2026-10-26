//! Apple app-logic archives with a generated C interface: iPhone, the iOS
//! Simulator and the Mac share one boundary. [IOS-TARGET] [IOS-TARGET-MACOS]

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
    Mac,
}

/// Everything that distinguishes one slice, stated once per slice.
struct Slice {
    /// The `--target` value. A generated header and every diagnostic name it,
    /// so following either one rebuilds the slice the reader already chose.
    name: &'static str,
    sdk: &'static str,
    triple: &'static str,
    runtime: &'static str,
    /// The platform a diagnostic names.
    platform: &'static str,
    /// The `make` goal that builds the runtime, and the example directory.
    goal: &'static str,
}

const SLICES: [(Target, Slice); 3] = [
    (
        Target::Device,
        Slice {
            name: "ios",
            sdk: "iphoneos",
            triple: "arm64-apple-ios15.0",
            runtime: "libosprey_runtime_ios.a",
            platform: "iOS",
            goal: "ios",
        },
    ),
    (
        Target::Simulator,
        Slice {
            name: "ios-sim",
            sdk: "iphonesimulator",
            triple: "arm64-apple-ios15.0-simulator",
            runtime: "libosprey_runtime_ios_sim.a",
            platform: "iOS",
            goal: "ios",
        },
    ),
    (
        Target::Mac,
        Slice {
            name: "macos",
            sdk: "macosx",
            triple: "arm64-apple-macos12.0",
            runtime: "libosprey_runtime_macos.a",
            platform: "macOS",
            goal: "macos",
        },
    ),
];

impl Target {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        SLICES
            .iter()
            .find(|(_, slice)| slice.name == value)
            .map(|(target, _)| *target)
    }

    fn slice(self) -> &'static Slice {
        match self {
            Self::Device => &SLICES[0].1,
            Self::Simulator => &SLICES[1].1,
            Self::Mac => &SLICES[2].1,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        self.slice().name
    }

    fn sdk(self) -> &'static str {
        self.slice().sdk
    }

    fn triple(self) -> &'static str {
        self.slice().triple
    }

    fn runtime(self, memory: &str) -> String {
        crate::linking::runtime_name(self.slice().runtime, memory)
    }

    /// The diagnostic for a slice whose runtime archive has not been built.
    fn missing_runtime(self, memory: &str) -> String {
        let slice = self.slice();
        format!(
            "{} not found — run `make {}` to build the {} runtime",
            self.runtime(memory),
            slice.goal,
            slice.platform
        )
    }
}

/// Refuse native-only settings before invoking the cross compiler.
/// Implements [IOS-TARGET-OPTIONS].
pub(crate) fn validate(cli: &Cli, target: Target) -> Result<(), ExitCode> {
    let slice = target.slice();
    crate::reject_cross_target_options(
        cli,
        slice.platform,
        Some(&format!("a Swift host (see examples/{}/)", slice.goal)),
    )
}

/// Emit the app's C ABI and LLVM implementation before any toolchain work.
/// Implements [IOS-HOST-ABI] and [IOS-TARGET-ENTRY].
pub(crate) fn source(
    program: &Program,
    path: &str,
    target: Target,
    scope: &ios_abi::ExportScope,
) -> Result<(String, String), String> {
    // Apple ARM64 zero-extends a C bool in both slices.
    ios_abi::source(program, path, target.name(), true, scope)
}

/// Compile an ARM64 static archive, bundling the matching C runtime, plus its
/// sibling C header. The Swift app supplies the executable entry and imports.
/// Implements [IOS-TARGET-LINK].
pub(crate) fn build(
    path: &str,
    program: &Program,
    out: &Path,
    target: Target,
    scope: &ios_abi::ExportScope,
    memory: &str,
) -> Result<(), ExitCode> {
    validate_output(out, target).map_err(|e| fail(&e))?;
    let (ir, header) = source(program, path, target, scope).map_err(|e| fail(&e))?;
    let sdk = sdk_path(target)?;
    let runtime = find_runtime_lib(&target.runtime(memory))
        .ok_or_else(|| fail(&target.missing_runtime(memory)))?;
    let runtime = crate::linking::fresh_runtime(&runtime).map_err(|e| fail(&e))?;
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

fn validate_output(out: &Path, target: Target) -> Result<(), String> {
    crate::toolchain::validate_archive_output(out, target.slice().platform)
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
