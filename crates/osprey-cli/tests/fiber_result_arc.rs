//! Focused ownership regression for Result values crossing a fiber boundary.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn run_arc(source: &str) -> io::Result<Output> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo_root())
        .args(["/dev/stdin", "--run", "--quiet", "--memory=arc"])
        .env("OSPREY_ARC_DEBUG", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::other("osprey child did not expose piped stdin"))?
        .write_all(source.as_bytes())?;
    child.wait_with_output()
}

#[test]
fn awaited_overflow_error_transfer_is_released_under_arc() -> io::Result<()> {
    let source = r"
fn checked(value: int) -> Result<int, MathError> = value + 1

let observed = await(spawn checked(9223372036854775807))
print(toString(observed))
";

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Error(integer overflow)\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn repeated_awaits_each_receive_a_live_result_under_arc() -> io::Result<()> {
    let source = r"
fn checked(value: int) -> Result<int, MathError> = value + 1

let task = spawn checked(41)
let first = await(task)
print(toString(first))
let second = await(task)
print(toString(second))
";

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Success(42)\nSuccess(42)\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn repeated_awaits_each_receive_a_live_string_under_arc() -> io::Result<()> {
    let source = r#"
fn greet(value: int) -> string = "hello ${value}"

let task = spawn greet(7)
let first = await(task)
print(first)
let second = await(task)
print(second)
"#;

    let output = run_arc(source)?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "hello 7\nhello 7\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[osp-arc] exit: 0 live objects"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
