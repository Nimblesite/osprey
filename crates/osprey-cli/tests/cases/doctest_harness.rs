//! The actual corpus gate must fail on drift, missing examples, and malformed CLI reports.
//! Implements [DOC-DOCTEST-HARNESS].

use super::{finish, repo_root, temp_dir, Out};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn harness(report: &str, status: i32, documented: bool, target: &str) -> (Out, std::path::PathBuf) {
    let root = temp_dir(&format!(
        "doctest_gate_{status}_{documented}_{target}_{}",
        report.len()
    ));
    let binary = root.join("fake-osprey");
    let script = "#!/bin/sh\nprintf '%s\\n' \"$DOCTEST_REPORT\"\nexit \"$DOCTEST_STATUS\"\n";
    assert!(std::fs::write(&binary, script).is_ok());
    assert!(std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).is_ok());
    let source = if documented {
        "/// ```osprey\n/// print(42)\n/// ```\n"
    } else {
        "fn value() = 42\n"
    };
    for name in ["default.osp", "ml.ospml"] {
        assert!(std::fs::write(root.join(name), source).is_ok());
    }
    let mut command = Command::new("zsh");
    let _ = command.args(["-c", "source \"$HARNESS\"; FILES=(\"$FIXTURES/default.osp\" \"$FIXTURES/ml.ospml\"); run_corpus_doctests"])
        .env("HARNESS", repo_root().join("crates/corpus_doctests.sh")).env("FIXTURES", &root)
        .env("BIN", binary).env("TARGET", target).env("MEMORY", "default").env("RESULTDIR", &root)
        .env("SMOKE", repo_root().join("scripts/wasm-smoke.mjs"))
        .env("DOCTEST_REPORT", report).env("DOCTEST_STATUS", status.to_string());
    (finish(command), root)
}

#[test]
fn corpus_doctest_gate_enforces_reports_output_status_and_discovery_floor() {
    for (report, status, documented, accepted) in [
        ("doctests: 3 passed, 0 failed", 0, true, true),
        ("doctests: 2 passed, 0 failed", 0, true, false),
        ("doctests: 3 passed, 0 failed", 1, true, false),
        ("doctests: 2 passed, 1 failed", 0, true, false),
        (
            "doctests: 3 passed, 0 failed\nunrelated output",
            0,
            true,
            false,
        ),
        ("malformed", 0, true, false),
        ("doctests: 3 passed, 0 failed", 0, false, false),
    ] {
        let (result, _) = harness(report, status, documented, "native");
        assert_eq!(
            result.code == Some(0),
            accepted,
            "{report:?}: {} {}",
            result.stdout,
            result.stderr
        );
        assert!(result.stdout.contains("TEST_CORPUS_DOCTEST_PASS="));
        assert!(result.stdout.contains("TEST_CORPUS_DOCTEST_FAIL="));
    }
}

#[test]
fn corpus_doctest_wasm_host_forwards_paths_without_shell_interpolation() {
    let (result, root) = harness("doctests: 3 passed, 0 failed", 0, true, "wasm32");
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    let wrapper = root.join("doctest-wasi-host");
    let script = std::fs::read_to_string(&wrapper).expect("WASM wrapper");
    assert!(script.contains("exec node \"$OSPREY_DOCTEST_WASI_SCRIPT\" \"$@\""));
    assert_ne!(
        std::fs::metadata(wrapper)
            .expect("wrapper mode")
            .permissions()
            .mode()
            & 0o111,
        0
    );
}
