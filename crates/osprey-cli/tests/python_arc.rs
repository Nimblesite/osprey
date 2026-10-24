//! Python-flavor programs that pin ARC ownership bugs the port suites found
//! ([GC-ARC-PERCEUS]). The corpus harness walks only the Default and ML twins,
//! so a leak that only a Python-flavor lowering reaches is pinned here: each
//! fixture under `tests/regressions/python/` must print its expected output
//! and exit under ARC with no live objects.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `(stdout, stderr)` of `fixture` run under the ARC backend with the exit
/// report armed.
fn run_arc(fixture: &Path) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo())
        .arg(fixture)
        .args(["--run", "--quiet", "--memory=arc"])
        .env("OSPREY_ARC_DEBUG", "1")
        .output();
    output.map_or_else(
        |error| (String::new(), error.to_string()),
        |out| {
            (
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        },
    )
}

#[test]
fn a_record_captured_per_loop_iteration_is_released_each_iteration() {
    let fixture = repo().join("tests/regressions/python/closure_captures_record.py");
    let (stdout, stderr) = run_arc(&fixture);
    assert_eq!(stdout, "13\n", "unexpected output:\n{stdout}\n{stderr}");
    assert!(
        stderr.contains("[osp-arc] exit: 0 live objects"),
        "the ARC exit report must show no live objects:\n{stderr}"
    );
}
