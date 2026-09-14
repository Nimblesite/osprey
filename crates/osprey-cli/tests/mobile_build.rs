//! Exercise mobile driver orchestration without requiring SDKs in Rust CI.
//! Actual object formats and execution are covered by test-ios/test-android.sh.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TOOL: &str = r#"#!/bin/sh
set -eu
name=${0##*/}
printf '%s\n' "$@" >> "$MOBILE_TOOL_LOG"
if [ "$name" = xcrun ]; then
    shift 2
    name=$1
    shift
    if [ "$name" = --show-sdk-path ]; then
        printf '%s\n' "$MOBILE_SDK"
        exit 0
    fi
fi
if [ "$name" = "${MOBILE_FAIL_TOOL:-}" ]; then exit 17; fi
if [ "$name" = llvm-ar ]; then
    printf 'mobile archive\n' > "$2"
else
    while [ "$#" -gt 0 ]; do
        case "$1" in
            *.ll) cp "$1" "$MOBILE_IR" ;;
            -o) shift; printf 'mobile archive\n' > "$1" ;;
        esac
        shift
    done
fi
"#;

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> std::io::Result<Self> {
        let root =
            std::env::temp_dir().join(format!("osprey_mobile_build_{name}_{}", std::process::id()));
        fs::create_dir_all(root.join("compiler/bin"))?;
        fs::create_dir_all(root.join("sdk"))?;
        for runtime in ["ios", "ios_sim", "android_arm64", "android_x64"] {
            fs::write(
                root.join(format!("compiler/bin/libosprey_runtime_{runtime}.a")),
                "runtime fixture",
            )?;
        }
        fs::write(root.join("app.osp"), "fn answer() = 42\n")?;
        let fixture = Self(root);
        Self::tool(&fixture.0.join("xcrun"))?;
        for version in ["9.9", "28.2"] {
            for tool in ["clang", "llvm-ar"] {
                Self::tool(&fixture.ndk_bin(version).join(tool))?;
            }
        }
        Ok(fixture)
    }

    fn ndk_bin(&self, version: &str) -> PathBuf {
        let host = if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else {
            "linux-x86_64"
        };
        self.0.join(format!(
            "sdk/ndk/{version}/toolchains/llvm/prebuilt/{host}/bin"
        ))
    }

    fn tool(path: &Path) -> std::io::Result<()> {
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| std::io::Error::other("tool parent"))?,
        )?;
        fs::write(path, TOOL)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }

    fn command(&self, target: &str, output: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_osprey"));
        let _ = cmd
            .current_dir(&self.0)
            .args(["app.osp", "--compile", "-o", output])
            .arg(format!("--target={target}"))
            .env("OSPREY_XCRUN", self.0.join("xcrun"))
            .env("ANDROID_HOME", self.0.join("sdk"))
            .env_remove("ANDROID_NDK_HOME")
            .env_remove("ANDROID_NDK_ROOT")
            .env_remove("MOBILE_FAIL_TOOL")
            .env("MOBILE_SDK", self.0.join("sdk"))
            .env("MOBILE_TOOL_LOG", self.0.join("tools.log"))
            .env("MOBILE_IR", self.0.join("emitted.ll"));
        cmd
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fails(output: &Output, diagnostic: &str) {
    assert!(!output.status.success(), "unexpected success: {output:?}");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains(diagnostic), "{error}");
}

#[test]
fn mobile_builds_publish_matching_headers_archives_and_clean_scratch() -> std::io::Result<()> {
    for (target, triple, runtime) in [
        ("ios", "arm64-apple-ios15.0", "ios"),
        ("ios-sim", "arm64-apple-ios15.0-simulator", "ios_sim"),
        ("android-arm64", "aarch64-linux-android26", "android_arm64"),
        ("android-x64", "x86_64-linux-android26", "android_x64"),
    ] {
        let fixture = Fixture::new(target)?;
        let result = fixture.command(target, "output/app.a").output()?;
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            fs::read_to_string(fixture.0.join("output/app.a"))?,
            "mobile archive\n"
        );
        let header = fs::read_to_string(fixture.0.join("output/app.h"))?;
        assert!(header.contains("int64_t osprey_answer(void)"), "{header}");
        let ir = fs::read_to_string(fixture.0.join("emitted.ll"))?;
        assert!(ir.contains("define i64 @osprey_answer()"), "{ir}");
        let log = fs::read_to_string(fixture.0.join("tools.log"))?;
        assert!(
            log.lines().any(|line| line == format!("--target={triple}")),
            "{log}"
        );
        assert!(
            log.contains(&format!("libosprey_runtime_{runtime}.a")),
            "{log}"
        );
        let scratch_ir = log
            .lines()
            .find(|line| line.ends_with("/app.ll"))
            .expect("IR argument");
        assert!(!Path::new(scratch_ir).exists(), "scratch leaked: {log}");
    }
    Ok(())
}

#[test]
fn mobile_tool_failure_does_not_replace_existing_artifacts() -> std::io::Result<()> {
    for (target, archiver) in [("ios-sim", "libtool"), ("android-arm64", "llvm-ar")] {
        let fixture = Fixture::new(&format!("failure-{target}"))?;
        fs::write(fixture.0.join("app.a"), "previous archive")?;
        fs::write(fixture.0.join("app.h"), "previous header")?;
        for tool in ["clang", archiver] {
            fails(
                &fixture
                    .command(target, "app.a")
                    .env("MOBILE_FAIL_TOOL", tool)
                    .output()?,
                "failed",
            );
            assert_eq!(
                fs::read_to_string(fixture.0.join("app.a"))?,
                "previous archive"
            );
            assert_eq!(
                fs::read_to_string(fixture.0.join("app.h"))?,
                "previous header"
            );
        }
        fails(
            &fixture.command(target, "app.o").output()?,
            "must end in .a",
        );
    }
    Ok(())
}

#[test]
fn mobile_sdk_errors_are_actionable_and_ndk_overrides_win() -> std::io::Result<()> {
    let fixture = Fixture::new("discovery")?;
    fails(
        &fixture
            .command("ios", "app.a")
            .env("OSPREY_XCRUN", fixture.0.join("missing"))
            .output()?,
        "install Xcode",
    );
    fails(
        &fixture
            .command("ios-sim", "app.a")
            .env("MOBILE_SDK", fixture.0.join("missing"))
            .output()?,
        "iphonesimulator SDK unavailable",
    );
    for key in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        fails(
            &fixture
                .command("android-arm64", "app.a")
                .env(key, fixture.0.join("missing"))
                .output()?,
            "Android NDK clang not found",
        );
    }
    // The numeric newest version wins (28.2 > 9.9), even when 9.9 has a broken compiler.
    fs::remove_file(fixture.ndk_bin("9.9").join("clang"))?;
    let result = fixture.command("android-arm64", "app.a").output()?;
    assert!(result.status.success(), "{result:?}");
    fails(
        &fixture
            .command("android-arm64", "app.a")
            .env("ANDROID_HOME", fixture.0.join("missing"))
            .output()?,
        "Android NDK not found",
    );
    Ok(())
}
