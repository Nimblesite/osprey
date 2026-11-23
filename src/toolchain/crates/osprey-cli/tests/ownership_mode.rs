//! `--memory=ownership` [MEM-OWNERSHIP-STATIC]
//! (docs/specs/compiler/0018-MemoryManagement.md): the compiler proves one
//! owner for every managed value or refuses the program. An accepted program
//! prints what it prints under the collector and leaves nothing allocated; a
//! refused one names the value that needs a second owner and where.

use std::process::{Command, Output};

const CLEAN_EXIT: &str = "[osp-arc] exit: 0 live objects, ";

fn run(name: &str, source: &str, memory: &str) -> std::io::Result<Output> {
    let directory = std::env::temp_dir().join(format!("osprey_ownership_{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{name}_{memory}.osp"));
    std::fs::write(&path, source)?;
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&path)
        .args(["--run", &format!("--memory={memory}")])
        .env("OSPREY_ARC_DEBUG", "1")
        .output();
    let _ = std::fs::remove_file(&path);
    output
}

/// Runs `source` under the collector and under ownership: same output, and the
/// ownership run ends with no block left allocated.
fn assert_accepted(name: &str, source: &str) -> std::io::Result<()> {
    let collected = run(name, source, "gc")?;
    let owned = run(name, source, "ownership")?;
    let stderr = String::from_utf8_lossy(&owned.stderr);
    assert!(collected.status.success(), "{name} under gc");
    assert!(owned.status.success(), "{name}: {stderr}");
    assert!(!collected.stdout.is_empty(), "{name} prints nothing");
    assert_eq!(
        String::from_utf8_lossy(&owned.stdout),
        String::from_utf8_lossy(&collected.stdout),
        "{name}"
    );
    assert!(stderr.contains(CLEAN_EXIT), "{name} left blocks: {stderr}");
    Ok(())
}

/// Ownership refuses `source`, saying each of `wanted`; the collector runs it.
fn assert_refused(name: &str, source: &str, wanted: &[&str]) -> std::io::Result<()> {
    let collected = run(name, source, "gc")?;
    assert!(
        collected.status.success(),
        "{name} under gc: {}",
        String::from_utf8_lossy(&collected.stderr)
    );
    let owned = run(name, source, "ownership")?;
    let stderr = String::from_utf8_lossy(&owned.stderr);
    assert!(!owned.status.success(), "{name} was accepted");
    assert!(owned.stdout.is_empty(), "{name} ran: {stderr}");
    for needle in wanted {
        assert!(stderr.contains(needle), "{name} lacks `{needle}`: {stderr}");
    }
    assert!(
        stderr.contains("[MEM-OWNERSHIP-STATIC]"),
        "{name}: {stderr}"
    );
    Ok(())
}

const FRESH_VALUES: &str = r#"type Point = { x: int, y: int }
type Shape = Dot { at: Point } | Line { from: Point, to: Point }

fn span(shape) = match shape {
    Dot { at } => at.x
    Line { from, to } => wrapSub(to.x, from.x)
}

fn label(shape) = "span ${span(shape)}"

fn main() = {
    let line = Line { from: Point { x: 1, y: 2 }, to: Point { x: 9, y: 4 } }
    let dot = Dot { at: Point { x: 5, y: 5 } }
    print("${label(line)}, ${label(dot)}, ${toString(wrapAdd(span(line), span(dot)))}")
}
"#;

// A fresh value moves into the list or record that takes it.
const FRESH_ELEMENTS: &str = r#"type Point = { x: int, y: int }

fn first(points) = match points {
    [head, ..._] => head.x
    [] => 0
}

fn main() = {
    let points = [Point { x: 1, y: 2 }, Point { x: 7, y: 4 }, Point { x: 3, y: 9 }]
    let names = ["a${toString(1)}", "b${toString(2)}"]
    print("${toString(first(points))} ${toString(listLength(names))}")
}
"#;

// A variant with no fields is one static value: storing it counts nothing.
const STATIC_VARIANTS: &str = r#"type Fault = DivZero | Overflow { by: int }

fn divide(a, b) = match b {
    0 => Error { message: DivZero }
    _ => Success { value: wrapAdd(a, b) }
}

fn show(outcome) = match outcome {
    Success { value } => "ok ${toString(value)}"
    Error { message } => "bad"
}

fn main() = print("${show(divide(6, 0))} ${show(divide(6, 1))}")
"#;

// A closure takes what it captures when the capture is that binding's last use.
const CAPTURED_AT_LAST_USE: &str = r#"type Box = { text: string }
type Named = { label: (int) -> string }

fn apply(f, n) = f(n)

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    let suffix = "z${toString(2)}"
    let named = Named { label: |n| => "${made.text}${toString(n)}" }
    print("${named.label(3)}${apply(|n| => "${suffix}${toString(n)}", 4)}")
}
"#;

// An Error owns its message once and joins a Success without a copy.
const FRESH_ERRORS: &str = r#"fn find(n) = match n {
    0 => Error { message: "no ${toString(n)}" }
    _ => Success { value: n }
}

fn show(found) = match found {
    Success { value } => "found ${toString(value)}"
    Error { message } => "missing: ${message}"
}

fn main() = print("${show(find(0))}, ${show(find(4))}")
"#;

// A binding read once, and never again, gives its value to what takes it.
const LAST_USE_MOVES: &str = r#"type Box = { text: string }
type Pair = { left: Box, right: Box }

fn build(n) = {
    let left = Box { text: "l${toString(n)}" }
    let right = Box { text: "r${toString(n)}" }
    Pair { left: left, right: right }
}

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    let again = made
    let boxes = [again, Box { text: "b" }]
    let pair = build(7)
    print("${pair.left.text}${pair.right.text}${toString(listLength(boxes))}")
}
"#;

const SCALARS_AND_TEXT: &str = r#"fn twice(n) = wrapMul(n, 2)
fn greet(name) = "hello ${name}"

fn main() = {
    let n = twice(21)
    print("${greet("osprey")} ${n}")
}
"#;

#[test]
fn a_program_whose_values_each_have_one_owner_runs_and_frees_everything() -> std::io::Result<()> {
    assert_accepted("scalars", SCALARS_AND_TEXT)?;
    assert_accepted("elements", FRESH_ELEMENTS)?;
    assert_accepted("last_use", LAST_USE_MOVES)?;
    assert_accepted("errors", FRESH_ERRORS)?;
    assert_accepted("captured", CAPTURED_AT_LAST_USE)?;
    assert_accepted("variants", STATIC_VARIANTS)?;
    assert_accepted("fresh", FRESH_VALUES)
}

const SECOND_NAME: &str = r#"type Box = { text: string }

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    let again = made
    print("${made.text}${again.text}")
}
"#;

const STORED_TWICE: &str = r#"type Box = { text: string }
type Pair = { left: Box, right: Box }

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    let pair = Pair { left: made, right: made }
    print("${pair.left.text}${pair.right.text}")
}
"#;

// Read again after it is stored: the store is not the last use.
const STORED_THEN_READ: &str = r#"type Box = { text: string }
type Wrap = { inner: Box }

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    let wrap = Wrap { inner: made }
    print("${wrap.inner.text}${made.text}")
}
"#;

// Stored on one path only: the other path still owns it.
const STORED_ON_ONE_PATH: &str = r#"type Box = { text: string }
type Wrap = { inner: Box }

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    let wrap = match made.text {
        "a1" => Wrap { inner: made }
        _ => Wrap { inner: Box { text: "other" } }
    }
    print(wrap.inner.text)
}
"#;

const RETURNED_BORROW: &str = r#"type Box = { text: string }

fn pick(box, n) = match n {
    0 => box
    _ => Box { text: "none${toString(n)}" }
}

fn main() = {
    let made = Box { text: "a${toString(1)}" }
    print("${pick(made, 0).text}${pick(made, 3).text}")
}
"#;

#[test]
fn a_value_that_needs_a_second_owner_is_refused_by_name_and_place() -> std::io::Result<()> {
    assert_refused(
        "second_name",
        SECOND_NAME,
        &["`again`", " 5:4: ownership", "second owner"],
    )?;
    assert_refused(
        "stored_twice",
        STORED_TWICE,
        &["`made`", " 6:4: ownership", "second owner"],
    )?;
    assert_refused(
        "stored_then_read",
        STORED_THEN_READ,
        &["`made`", " 6:4: ownership"],
    )?;
    assert_refused(
        "stored_on_one_path",
        STORED_ON_ONE_PATH,
        &["`made`", "second owner"],
    )?;
    assert_refused(
        "returned_borrow",
        RETURNED_BORROW,
        &[
            "`box` needs a second owner in `pick`",
            "one arm of a `match`",
        ],
    )
}

#[test]
fn an_unknown_backend_names_ownership_among_the_choices() -> std::io::Result<()> {
    let output = run("unknown", SCALARS_AND_TEXT, "bogus")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("available: gc, arc, ownership, none"),
        "{stderr}"
    );
    Ok(())
}
