//! Focused ownership regressions for generic values under ARC.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "common/effect_execution.rs"]
mod effect_execution;

#[path = "common/workbench_fixture.rs"]
mod workbench_fixture;

#[test]
fn workbench_retains_record_identity_through_shared_proof_helpers() -> io::Result<()> {
    compile_workbench("apps/workbench", "wasm32")
}

#[test]
fn workbench_apple_host_includes_shared_proof_dependencies() -> io::Result<()> {
    compile_workbench("apps/workbench/hosts/apple/app", "macos")
}

fn compile_workbench(project: &str, target: &str) -> io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .current_dir(root)
        .arg(project)
        .args(["--llvm", "--quiet", "--no-warnings"])
        .arg(format!("--target={target}"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("define"));
    Ok(())
}

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
    let original = Payload { items: outer }
    let owner = match listLength(owned) == 3 { true => Payload { items: owned } false => original }
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

const MIXED_MATCH_OWNERS: &str = r#"
type Node = Node { text: string }
fn choose(fresh, old) = match fresh {
    true => Node { text: "fresh:${42}" }
    false => old
}
fn nested(fresh, old) = match fresh { true => match fresh { true => old false => choose(true, old) } false => choose(false, old) }
fn wrappedChoice(fresh, old) = match fresh { true => Success { value: choose(true, Node { text: "unused" }) } false => old }
fn resultText(result) = match result { Success { value } => value.text Error { message } => message }
type Viewed = { text: string }
fn erasedChoice(fresh, old: any) = match fresh { true => Viewed { text: "fresh:${42}" } false => old }
fn erasedText(value: any) = match value { { text } => text _ => "unexpected erased shape" }
fn repeated(which, old) = match which { 0 => old 1 => old _ => Node { text: "fresh:${42}" } }
fn discard(old) = match repeated(1, old) { Node { text } => byteLength(text) }
fn repeatedLifetime(n) = {
    let base = Node { text: "base:${n}" }
    let size = discard(base)
    let replacement = Node { text: "replacement:${n}" }
    "${base.text}|${replacement.text}|${size}"
}
fn exercise() = {
    let old = Node { text: "outer:${7}" }
    let alias = old
    let first = nested(false, old)
    let second = choose(true, old)
    let wrapped = nested(true, old)
    let last = fold(range(0, 10000), old, fn(current, index) => choose(index % 2 == 0, current))
    let kept = Success { value: old }
    let boxed = Viewed { text: old.text }
    "${first.text}|${second.text}|${wrapped.text}|${last.text}|${alias.text}|${resultText(wrappedChoice(true, kept))}|${resultText(wrappedChoice(false, kept))}|${erasedText(erasedChoice(true, boxed))}|${erasedText(erasedChoice(false, boxed))}"
}
print("${exercise()}|${repeatedLifetime(7)}")
"#;

#[test]
fn mixed_match_owners_preserve_borrows_aliases_and_loop_accumulators() {
    effect_execution::assert_flavored_output(
        "mixed_match_owners",
        "osp",
        MIXED_MATCH_OWNERS,
        "outer:7|fresh:42|outer:7|fresh:42|outer:7|fresh:42|outer:7|fresh:42|outer:7|base:7|replacement:7|6\n",
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

/// One run of `root` with the shared launcher flags. `extra` carries only the
/// flags a caller genuinely needs, so every existing caller keeps its exact
/// behaviour, warnings included.
fn run_project(root: &Path, memory: &str, extra: &[&str]) -> io::Result<std::process::Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command
        .current_dir(repo_root())
        .arg(root)
        .args(["--run", "--quiet", &format!("--memory={memory}")])
        .args(extra);
    effect_execution::runtime_environment(&mut command, memory);
    command.output()
}

fn run_port_identities(root: &Path, memory: &str) -> io::Result<std::process::Output> {
    run_project(root, memory, &[])
}

/// The staged fixture compiles the whole workbench root set; warnings are not
/// what it asserts and the redundant-annotation oracle dominates that build.
fn compile_goal_fixture(root: &Path, memory: &str) -> io::Result<PathBuf> {
    let binary = root.join(format!("fixture-{memory}{}", std::env::consts::EXE_SUFFIX));
    let output = goal_compile_command(root, &binary, memory).output()?;
    assert!(
        output.status.success(),
        "{memory} compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(binary)
}

fn goal_compile_command(root: &Path, binary: &Path, memory: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command
        .current_dir(repo_root())
        .arg(root)
        .args(["--compile", "--quiet", "--no-warnings"])
        .arg(format!("--memory={memory}"))
        .arg("-o")
        .arg(binary);
    command
}

fn run_goal_fault(root: &Path, binary: &Path, memory: &str) -> io::Result<std::process::Output> {
    let mut command = Command::new(binary);
    let _ = command.current_dir(root);
    effect_execution::runtime_environment(&mut command, memory);
    let output = command.output()?;
    if memory == "arc" {
        effect_execution::assert_arc_clean(&String::from_utf8_lossy(&output.stderr));
    }
    Ok(output)
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
    if memory == "arc" {
        effect_execution::assert_arc_clean(&String::from_utf8_lossy(&output.stderr));
    }
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
    let output = child.wait_with_output()?;

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Success(42)\n");
    effect_execution::assert_arc_clean(&String::from_utf8_lossy(&output.stderr));
    Ok(())
}

fn phase(label: &str) {
    eprintln!("phase {label} at {:?}", std::time::SystemTime::now());
}

/// [WORKBENCH-GOALS] A goal formula the reader refuses is a broken fixture,
/// never `⊥`: the shared typed helper fails the active test normally. The
/// valid formula in the same pipeline still prints itself. Both controls run
/// under GC and ARC, and no exit path is skipped on either OS family.
#[test]
fn a_faulted_workbench_goal_formula_fails_its_case_without_fabricating_bottom() -> io::Result<()> {
    let root =
        TempProject(std::env::temp_dir().join(format!("osprey_goal_fault_{}", std::process::id())));
    workbench_fixture::stage(&root.0, &repo_root())?;
    for memory in ["gc", "arc"] {
        check_goal_backend(&root.0, memory)?;
    }
    Ok(())
}

fn check_goal_backend(root: &Path, memory: &str) -> io::Result<()> {
    phase(&format!("{memory} compile once"));
    let binary = compile_goal_fixture(root, memory)?;
    std::fs::write(root.join("formula.txt"), "p ∧")?;
    workbench_fixture::assert_faulted(&run_goal_fault(root, &binary, memory)?, memory);
    std::fs::write(root.join("formula.txt"), workbench_fixture::VALID_FORMULA)?;
    assert_valid_goal(&run_goal_fault(root, &binary, memory)?, memory);
    std::fs::remove_file(root.join("formula.txt"))?;
    workbench_fixture::assert_missing(&run_goal_fault(root, &binary, memory)?, memory);
    Ok(())
}

fn assert_valid_goal(output: &std::process::Output, memory: &str) {
    assert!(
        output.status.success(),
        "{memory}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        workbench_fixture::VALID_STDOUT,
        "{memory} changed the goal pipeline's stdout"
    );
}
