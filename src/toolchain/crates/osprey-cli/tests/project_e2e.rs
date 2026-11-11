//! End-to-end coverage for mixed-flavor project CLI entry points.
#![expect(
    clippy::expect_used,
    reason = "subprocess and fixture setup failures must fail these end-to-end tests immediately"
)]

#[path = "common/repo_root.rs"]
mod repo_root;
use repo_root::repo_root;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixture setup and cleanup failures end these tests through `?`.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A rewritten fixture copy: its directory, the original text and the new text.
type Rewritten = (PathBuf, String, String);

fn fixture() -> PathBuf {
    repo_root().join("src/osprey/examples/projects/modules")
}

fn osprey() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command.current_dir(repo_root());
    command
}

struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(args: &[String]) -> Output {
    finish(osprey().args(args))
}

fn run_in(directory: &Path, args: &[&str]) -> Output {
    let mut command = osprey();
    let _ = command.current_dir(directory).args(args);
    finish(&mut command)
}

fn finish(command: &mut Command) -> Output {
    let result = command.output().expect("run osprey");
    Output {
        code: result.status.code(),
        stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    }
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Asserts a pipeline invocation succeeded, returning its output.
fn checked(output: Output) -> Output {
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    output
}

/// Asserts the `flag` pipeline stage on `path` succeeded, its stdout
/// containing every `wanted` and no `banned` fragment.
fn flag_stdout(path: &Path, flag: &str, wanted: &[&str], banned: &[&str]) -> String {
    let stdout = checked(run(&[arg(path), flag.to_string()])).stdout;
    for fragment in wanted {
        assert!(stdout.contains(fragment), "{stdout}");
    }
    for fragment in banned {
        assert!(!stdout.contains(fragment), "{stdout}");
    }
    stdout
}

/// Checks `project` quietly, asserting the flat project pipeline accepted it.
fn check_passes(project: &Path) {
    let output = run(&[arg(project), "--check".into(), "--quiet".into()]);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
}

/// A process-scoped temporary path plus the namespace its directory name spells.
fn temp_project(name: &str) -> (PathBuf, String) {
    let path = std::env::temp_dir().join(format!("osprey_{name}_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    let namespace = path.file_name().and_then(|name| name.to_str());
    let namespace = namespace
        .expect("temporary directory has an identifier name")
        .to_owned();
    (path, namespace)
}

const FIXTURE_SOURCES: [&str; 11] = [
    "osprey.toml",
    "src/main.ospml",
    "src/lib/sqlite.ospml",
    "src/domain/money.ospml",
    "src/domain/accounts.ospml",
    "src/domain/json.ospml",
    "src/store/ledger.ospml",
    "src/store/metrics.ospml",
    "src/api/routes.ospml",
    "src/web/bundle.ospml",
    "src/web/pages.ospml",
];

/// The module the project-layout tests import their `answer` from.
const ANSWERS_MODULE: &str = "module Answers {\n    export fn answer() -> int = 42\n}\n";

fn copy_fixture(name: &str) -> PathBuf {
    let root = temp_project(&format!("fixture_{name}")).0;
    for relative in FIXTURE_SOURCES {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).expect("create project directories");
        }
        let _ =
            std::fs::copy(fixture().join(relative), &destination).expect("copy project fixture");
    }
    root
}

fn expected_output() -> String {
    std::fs::read_to_string(fixture().join("expectedoutput")).expect("read expectedoutput fixture")
}

/// Copies the bank fixture under `name`, rewriting `relative` from `from` to `to`.
fn rewritten_fixture(name: &str, relative: &str, from: &str, to: &str) -> Rewritten {
    let project = copy_fixture(name);
    let file = project.join(relative);
    let original = std::fs::read_to_string(&file).expect("read fixture source");
    let rewritten = original.replace(from, to);
    std::fs::write(&file, &rewritten).expect("write rewritten fixture source");
    (project, original, rewritten)
}

/// Runs `project` under the GC and ARC backends, returning each run's stderr.
fn run_under_each_backend(project: &Path, expected: &str) -> Vec<String> {
    let mut stderrs = Vec::new();
    for backend in ["gc", "arc"] {
        let output = run(&[arg(project), "--run".into(), format!("--memory={backend}")]);
        assert_eq!(output.code, Some(0), "{backend}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "{backend}");
        stderrs.push(output.stderr);
    }
    stderrs
}

/// The Talon Bank demo binds one fixed HTTP port and one `SQLite` file, so
/// the tests that actually execute it must not overlap in time.
fn demo_run_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let guard = match LOCK.get_or_init(std::sync::Mutex::default).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    assert!(
        demo_available(Path::new("/tmp/talon_bank.hold")),
        "bank demo active"
    );
    guard
}

fn demo_available(marker: &Path) -> bool {
    !marker.exists()
}
#[test]
fn demo_preparation_preserves_another_sessions_hold_marker() {
    let directory = copy_fixture("foreign_hold");
    let marker = directory.join("hold");
    std::fs::write(&marker, "another session").expect("create foreign hold");
    let available = demo_available(&marker);
    let retained = std::fs::read_to_string(&marker);
    std::fs::remove_dir_all(directory).expect("clean fixture");
    assert!(!available, "an active demo must be refused, not stopped");
    assert_eq!(
        retained.expect("foreign hold must survive"),
        "another session"
    );
}

#[test]
fn flat_project_pipeline_covers_scripts_directories_and_manifests() {
    let project = fixture();
    let manifest = project.join("osprey.toml");
    for path in [&project, &manifest] {
        check_passes(path);
    }

    let _ = flag_stdout(
        &project,
        "--ast",
        &[],
        &["Import(", "Namespace {", "Module {"],
    );

    let _ = flag_stdout(&project, "--llvm", &["define"], &[]);

    let symbols = flag_stdout(
        &project,
        "--symbols",
        &["bank/web::Pages::render"],
        &["__osp_"],
    );
    assert_project_main_symbol_is_local(&symbols, &project.join("src/main.ospml"));

    // An ordinary script keeps its own names: no project name mangling.
    let source = temp_project("plain_script").0.with_extension("osp");
    std::fs::write(
        &source,
        "fn historicalName(value: int) -> int = value\nprint(historicalName(7))\n",
    )
    .expect("write ordinary script");
    let _ = flag_stdout(&source, "--llvm", &["@historicalName("], &["__osp_"]);
    let _ = std::fs::remove_file(source);
}

fn assert_project_main_symbol_is_local(json: &str, source: &Path) {
    let value = serde_json::from_str::<serde_json::Value>(json).expect("valid symbols JSON");
    let main = value.as_array().and_then(|entries| {
        entries.iter().find(|entry| {
            entry.get("name").and_then(serde_json::Value::as_str) == Some("bank::main")
        })
    });
    let expected_line = std::fs::read_to_string(source)
        .expect("read entry source")
        .lines()
        .position(|line| line.trim_start().starts_with("main () ="))
        .and_then(|line| u64::try_from(line.saturating_add(1)).ok());
    let line = main
        .and_then(|entry| entry.get("line"))
        .and_then(serde_json::Value::as_u64);
    assert_eq!(line, expected_line);
    let path = main
        .and_then(|entry| entry.get("path"))
        .and_then(serde_json::Value::as_str);
    let canonical = std::fs::canonicalize(source).ok();
    assert_eq!(path, canonical.as_deref().and_then(Path::to_str));
}

#[test]
fn bank_project_runs_and_builds_byte_exact_against_expectedoutput() {
    let _serialized = demo_run_lock();

    let project = fixture();
    let output = checked(run(&[arg(&project), "--run".to_string()]));
    assert_eq!(output.stdout, expected_output());

    let project = copy_fixture("build");
    let artifact = project.join("talon-bank");
    let _ = checked(run_in(&project, &["build", "--quiet"]));
    assert!(artifact.is_file(), "missing {}", artifact.display());

    let executed = Command::new(&artifact).output().expect("run built project");
    assert!(executed.status.success());
    assert_eq!(String::from_utf8_lossy(&executed.stdout), expected_output());
    let _ = std::fs::remove_dir_all(project);
}

// [MODULES-SIGNATURE], [MODULES-OPAQUE-TYPES]: exercise the shipped bank source.
#[test]
fn bank_money_signature_is_opaque_from_both_flavors() -> TestResult {
    for (extension, source) in [
        ("osp", "fn main() = { let amount = Money::fromCents(420005)\n print(\"${Money::show(amount)}|${wrapAdd(Money::toCents(amount), 1)}|${Money::positive(amount)}|${Money::show(Money::fromCents(-9223372036854775808))}\") }\n"),
        ("ospml", "main () =\n    amount = Money::fromCents 420005\n    print \"${Money::show amount}|${wrapAdd (Money::toCents amount) 1}|${Money::positive amount}|${Money::show (Money::fromCents (-9223372036854775808))}\"\n"),
    ] {
        let project = money_client_fixture(extension, source)?;
        let expected = "$4,200.05|420006|true|-$92,233,720,368,547,758.08\n";
        for stderr in run_under_each_backend(&project, expected) {
            assert!(!stderr.contains("redundant"), "{stderr}");
        }
        std::fs::remove_dir_all(project)?;
    }
    for &(extension, source, error) in MONEY_REJECTIONS {
        let project = money_client_fixture(extension, source)?;
        let output = run(&[arg(&project), "--check".into(), "--quiet".into()]);
        assert_eq!(output.code, Some(1), "{source}: {}", output.stderr);
        assert!(output.stderr.contains(error), "{source}: {}", output.stderr);
        std::fs::remove_dir_all(project)?;
    }
    Ok(())
}

const MONEY_REJECTIONS: &[(&str, &str, &str)] = &[
    (
        "osp",
        "fn main() = Money::Amount { cents: 1 }\n",
        "cannot be constructed outside module",
    ),
    (
        "ospml",
        "main () = Money::Amount(cents = 1)\n",
        "cannot be constructed outside module",
    ),
    (
        "osp",
        "fn main() = { let amount = Money::fromCents(1)\n amount.cents }\n",
        "is hidden outside module",
    ),
    (
        "ospml",
        "main () =\n    amount = Money::fromCents 1\n    amount.cents\n",
        "is hidden outside module",
    ),
    ("osp", "fn main() = Money::show(1)\n", "type mismatch"),
    ("ospml", "main () = Money::show 1\n", "type mismatch"),
];

fn money_client_fixture(
    extension: &str,
    source: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let project = temp_project(&format!("money_signature_{extension}_{}", source.len())).0;
    std::fs::create_dir_all(project.join("src"))?;
    let _ = std::fs::copy(
        fixture().join("src/domain/money.ospml"),
        project.join("src/money.ospml"),
    )?;
    std::fs::write(project.join(format!("src/main.{extension}")), source)?;
    std::fs::write(
        project.join("osprey.toml"),
        format!("[project]\nname = \"money-contract\"\nsource_roots = [\"src\"]\ndefault_namespace = \"bank\"\nentry = \"src/main.{extension}\"\n"),
    )?;
    Ok(project)
}

#[test]
fn answers_project_layouts_are_path_spelling_independent() {
    // A single file carrying its own module; the sibling stays unloaded.
    let (directory, namespace) = temp_project("single_module");
    std::fs::create_dir_all(&directory).expect("create single-source directory");
    let source = directory.join("main.osp");
    std::fs::write(
        &source,
        format!(
            "{ANSWERS_MODULE}import {namespace}::Answers::{{answer}}\nfn main() = print(\"answer=${{answer()}}\")\n"
        ),
    )
    .expect("write single module source");
    std::fs::write(directory.join("broken.osp"), "fn = = =\n").expect("write ignored sibling");
    let output = checked(run(&[arg(&source), "--run".to_string()]));
    assert_eq!(output.stdout, "answer=42\n");
    let _ = checked(run_in(&directory, &["main.osp", "--check", "--quiet"]));
    let _ = std::fs::remove_dir_all(directory);

    // A manifest-free project under `src`, addressed absolutely and relatively.
    let (directory, namespace) = temp_project("manifest_free");
    let sources = directory.join("src");
    std::fs::create_dir_all(&sources).expect("create manifest-free source root");
    std::fs::write(sources.join("values.osp"), ANSWERS_MODULE).expect("write module source");
    std::fs::write(
        sources.join("main.osp"),
        format!("import {namespace}::Answers::{{answer}}\nfn main() = print(\"${{answer()}}\")\n"),
    )
    .expect("write entry source");
    check_passes(&directory);
    let _ = checked(run_in(&directory, &[".", "--check", "--quiet"]));
    let _ = std::fs::remove_dir_all(directory);
}

#[path = "cases/project_diagnostics.rs"]
mod project_diagnostics;
