//! Focused ownership regression for generic effect results under ARC.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "common/effect_execution.rs"]
mod effect_execution;

const ABANDONED_OWNERS: &str = r#"
effect Stop { control halt: fn(string) -> int }
type Payload = { items: List<string> }

fn abandoned(outer) = {
    let owned = mapList([1, 2, 3], fn(n) => "item:${n}")
    let owner = Payload { items: owned }
    let capture = fn() => join(owner.items, ",")
    let message = "${capture()}:${listLength(outer)}"
    let answer = perform Stop.halt(message)
    print("unreachable:${capture()}:${message}")
    answer
}

let outer = mapList(["a", "b"], fn(s) => "${s}!")
let stopping = handler Stop {
    halt message => length(message)
    finally => print("finalized:${join(outer, ",")}")
}
let answer = stopping(fn() => abandoned(outer))
print("${answer}:${join(outer, ",")}")
"#;

#[test]
fn abandoning_control_releases_body_owners_and_preserves_outer_captures() {
    effect_execution::assert_flavored_output(
        "abandoned_owners",
        "osp",
        ABANDONED_OWNERS,
        "finalized:a!,b!\n22:a!,b!\n",
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn generic_effect_result_promotion_retains_adapted_wrapper_under_arc() -> io::Result<()> {
    let source = r"
effect Source<T> {
    take: fn() -> Result<T, string>
}

fn checked() !Source<int> = perform Source.take()

let observed = {
    handle Source {
        take => 40 + 2
    }
    checked()
}

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
