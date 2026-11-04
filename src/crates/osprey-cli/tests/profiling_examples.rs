//! The profiling walkthrough's programs [PROF-VSCODE-SHARED] [GC-ARC-PERCEUS].
//!
//! `src/examples/profiling` is what the documentation and its tutorial film run:
//! `busy.osp` is the program the VS Code profiler attaches to, and `words.osp`
//! is the leak check. Each claim the walkthrough makes about them is held
//! here against this build's compiler, so the docs cannot drift from it.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/profiling")
        .join(name)
}

fn osprey(args: &[&str], env: &[(&str, &str)]) -> io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args(args)
        .envs(env.iter().copied())
        .output()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn the_leak_check_example_ends_with_no_live_objects_under_arc() -> io::Result<()> {
    let source = example("words.osp");
    let source = source.to_string_lossy();
    let output = osprey(
        &[&source, "--run", "--quiet", "--memory=arc"],
        &[("OSPREY_ARC_DEBUG", "1")],
    )?;
    let (stdout, stderr) = (text(&output.stdout), text(&output.stderr));
    assert!(
        output.status.success(),
        "words.osp runs under ARC: {stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "characters: 488894",
        "the program prints its total"
    );
    assert!(
        stderr.contains("[osp-arc] exit: 0 live objects, 0 KiB"),
        "the exit census the walkthrough shows reports nothing left alive: {stderr}"
    );
    Ok(())
}

#[test]
fn the_profiled_example_compiles_cleanly_with_debug_info() -> io::Result<()> {
    let source = example("busy.osp");
    let binary = std::env::temp_dir().join(format!("osprey-profiling-busy-{}", std::process::id()));
    let args = [
        &*source.to_string_lossy(),
        "--compile",
        "--profile",
        "-o",
        &*binary.to_string_lossy(),
    ];
    let output = osprey(&args, &[])?;
    let stderr = text(&output.stderr);
    let built = binary.exists();
    // A leftover binary in the temp dir is harmless; the build result is what is asserted.
    let _removed = std::fs::remove_file(&binary);
    assert!(
        output.status.success() && built,
        "busy.osp builds for the profiler: {stderr}"
    );
    assert!(
        !stderr.contains("warning"),
        "the example a tutorial shows carries no warnings: {stderr}"
    );
    Ok(())
}
