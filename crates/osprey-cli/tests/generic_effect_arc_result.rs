//! Focused ownership regressions for generic values under ARC.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "common/effect_execution.rs"]
mod effect_execution;

const STATIC_FINALIZERS: &str = r#"
static effect Config { number: fn() -> int }
effect Log { note: fn(string) -> Unit }
fn recording(work) = {
    mut trace = ""
    handle Log { note text => { trace = "${trace}${text}" } }
    let answer = work()
    "${answer}|${trace}"
}
fn exercise() = {
    let policy = handler static Config {
        number => 7
        return value => { perform Log.note("R")
wrapAdd(value, 1) }
        finally => perform Log.note("F")
    }
    let first = policy(fn() => { perform Log.note("B")
perform Config.number() })
    let second = policy(fn() => { perform Log.note("C")
99 })
    let plain = { handle static Config { number => 5
finally => perform Log.note("F") }
perform Log.note("D")
perform Config.number() }
    "${first}|${second}|${plain}"
}
fn main() = print(recording(exercise))
"#;

#[test]
fn static_handler_finalizers_run_after_return_transformations() {
    effect_execution::assert_flavored_output(
        "static_finalizers",
        "osp",
        STATIC_FINALIZERS,
        "8|100|5|BRFCRFDF\n",
    );
}

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

#[test]
fn abandoned_generic_constructors_release_previously_evaluated_fields() {
    let source = r#"
effect Stop { control halt: fn(int) -> int }
type Hold<T> = Holding { a: T, z: int }
fn make() = Holding { a: "owned:${42}", z: perform Stop.halt(7) }
let stopping = handler Stop { halt value => value }
let answer = stopping(fn() => match make() { Holding { a, z } => satAdd(length(a), z) })
print(answer)
"#;
    effect_execution::assert_flavored_output("abandoned_constructor", "osp", source, "7\n");
}

/// [GC-ARC-PERCEUS] A `Result` carrying a manifest record whose `Maybe`
/// publication holds a `Names::Key` read from a shipped `osprey.toml`.
/// The `Maybe::Present` payload is a generic variant slot: codegen boxes the
/// publication pointer into an `i64` field and leaves that raw block unmarked,
/// so the publication owner is released while the box still holds its address.
/// GC is the clean control; ARC prints the freed metadata instead of the key.
const PORT_IDENTITY_PROJECT: &str = r#"[project]
name = "port-identities"
source_roots = ["src"]
entry = "src/main.osp"
"#;

const PORT_IDENTITY_SOURCES: [(&str, &str); 5] = [
    (
        "names.osp",
        r#"namespace probe;
module Names {
    export type Key = { scope: string, name: string }

    fn split(body) = match indexOf(body, "/") {
        Success { value } => Success { value: Key { scope: take(body, value), name: drop(body, satAdd(value, 1)) } }
        Error => Error { message: "name.missing-slash" }
    }

    export fn parse(text) = match startsWith(text, "@") {
        false => Error { message: "name.missing-at" }
        true => split(drop(text, 1))
    }

    export fn display(key) = "@${key.scope}/${key.name}"
}
"#,
    ),
    (
        "maybe.osp",
        r"namespace probe;
module Maybe {
    export type Maybe<T> = Present { value: T } | Absent

    export fn some(value) = Present { value: value }

    export fn fold(maybe, absent, present) = match maybe {
        Present { value } => present(value)
        Absent => absent
    }
}
",
    ),
    (
        "model.osp",
        r"namespace probe;
import probe::Names
import probe::Maybe
module Model {
    export type Publication = { package: Names::Key }
    export type Manifest = { publication: Maybe::Maybe<Publication> }
}
",
    ),
    (
        "manifest.osp",
        r#"namespace probe;
import probe::Names
import probe::Maybe
import probe::Model
module Manifest {
    export type Problem = { code: string }
    export type Verdict = Accepted { manifest: Model::Manifest } | Rejected { problem: Problem }

    fn after(text, marker) = match indexOf(text, marker) {
        Success { value } => Success { value: drop(text, satAdd(value, byteLength(marker))) }
        Error => Error { message: "manifest.package.missing" }
    }

    fn quoted(text) = match indexOf(text, "\"") {
        Success { value } => take(text, value)
        Error => text
    }

    fn withKey(text) = match Names::parse(quoted(text)) {
        Success { value } => Accepted { manifest: Model::Manifest { publication: Maybe::some(Model::Publication { package: value }) } }
        Error { message } => Rejected { problem: Problem { code: message } }
    }

    export fn parse(text) = match after(text, "[package]\nname = \"") {
        Success { value } => withKey(value)
        Error { message } => Rejected { problem: Problem { code: message } }
    }

    export fn accepted(verdict) = match verdict {
        Accepted { manifest } => Success { value: manifest }
        Rejected { problem } => Error { message: problem.code }
    }
}
"#,
    ),
    (
        "main.osp",
        r#"namespace probe;
import probe::Manifest
import probe::Model
import probe::Names
import probe::Maybe

fn publicationKey(m) = Maybe::fold(m.publication, "none", fn(p) => Names::display(p.package))

fn identityOf(text) = match Manifest::accepted(Manifest::parse(text)) {
    Success { value } => publicationKey(value)
    Error { message } => message
}

fn main() = {
    print(identityOf(readFile("packages/flash_attn/osprey.toml") ?: ""))
    print(identityOf(readFile("packages/rouge_score/osprey.toml") ?: ""))
}
"#,
    ),
];

/// A project directory removed when the test ends, pass or fail.
struct TempProject(PathBuf);

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_port_identity_project(root: &Path) -> io::Result<()> {
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("osprey.toml"), PORT_IDENTITY_PROJECT)?;
    for (name, source) in PORT_IDENTITY_SOURCES {
        std::fs::write(root.join("src").join(name), source)?;
    }
    Ok(())
}

fn run_port_identities(root: &Path, memory: &str) -> io::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command.current_dir(repo_root()).arg(root).args([
        "--run",
        "--quiet",
        &format!("--memory={memory}"),
    ]);
    if memory == "arc" {
        let _ = command.env("OSPREY_ARC_DEBUG", "1");
    }
    command.output()
}

fn assert_port_identities(root: &Path, memory: &str, expected: &str) -> io::Result<()> {
    let output = run_port_identities(root, memory)?;
    assert!(
        output.status.success(),
        "{memory} run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "{memory} released a Maybe publication payload before Names::display read it"
    );
    Ok(())
}

#[test]
fn dynamically_read_port_manifests_keep_their_publication_keys_under_arc() -> io::Result<()> {
    let root = TempProject(
        std::env::temp_dir().join(format!("osprey_port_identities_{}", std::process::id())),
    );
    write_port_identity_project(&root.0)?;
    let expected = "@osprey/flash-attn\n@osprey/rouge-score\n";
    assert_port_identities(&root.0, "gc", expected)?;
    assert_port_identities(&root.0, "arc", expected)
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
