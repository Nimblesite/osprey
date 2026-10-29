//! `osprey <file> --signature` prints the ML export signature of a module as
//! an importing checker receives it ([TYPES-DISCIPLINE-PROTOCOL]): exported
//! functions only, type variables renumbered, and the text parses as ML.

#![expect(
    clippy::expect_used,
    reason = "test assertions: a failed scratch write is a test failure, not a production panic"
)]

use std::path::PathBuf;
use std::process::Command;

const MODULE: &str = "namespace demo

module Nat
    export label x = \"n=${toString x}\"
    export same x = x
    export twice (x, y) = x + y
    hidden x = x
";

const SIGNATURE: &str = "signature Nat
    label<t0> : (t0) -> string
    same<t0> : (t0) -> t0
    twice : (int, int) -> int
";

fn scratch(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("osprey-signature-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("nat.ospml");
    std::fs::write(&path, text).expect("write module");
    path
}

fn signature_of(path: &PathBuf) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(path)
        .arg("--signature")
        .output()
        .expect("run osprey");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn signature_mode_prints_exported_functions_with_renumbered_type_variables() {
    let path = scratch("ok", MODULE);
    let (code, stdout, stderr) = signature_of(&path);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, SIGNATURE);
    let parsed = osprey_syntax::parse_program_with_flavor(&stdout, osprey_syntax::Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "signature does not parse: {:?}",
        parsed.errors
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn signature_mode_reports_a_module_that_does_not_infer() {
    let path = scratch(
        "bad",
        "namespace demo\n\nmodule Nat\n    export broken () = listLength 1\n",
    );
    let (code, stdout, stderr) = signature_of(&path);
    assert_eq!(code, Some(1), "stdout: {stdout}");
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("could not be inferred"),
        "unexpected diagnostic: {stderr}"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}
