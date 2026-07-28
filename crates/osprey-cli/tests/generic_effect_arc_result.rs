//! Focused ownership regression for generic effect results under ARC.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn generic_effect_result_promotion_retains_adapted_wrapper_under_arc() -> io::Result<()> {
    let source = r"
effect Source<T> {
    take: fn() -> Result<T, MathError>
}

fn checked() -> Result<int, MathError> !Source<int> = perform Source.take()

let observed = handle Source
    take => (40 + 2) ?: 0
in checked()

print(toString(observed))
";

    let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(repo_root())
        .args(["/dev/stdin", "--run", "--quiet", "--memory=arc"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::other("osprey child did not expose piped stdin"))?
        .write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Success(42)\n");
    Ok(())
}
