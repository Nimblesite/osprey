//! End-to-end coverage for mixed-flavor project CLI entry points.
#![expect(
    clippy::expect_used,
    reason = "subprocess and fixture setup failures must fail these end-to-end tests immediately"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixture setup and cleanup failures end these tests through `?`.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A rewritten fixture copy: its directory, the original text and the new text.
type Rewritten = (PathBuf, String, String);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

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

    let ast = checked(run(&[arg(&project), "--ast".to_string()])).stdout;
    assert!(!ast.contains("Import("), "{ast}");
    assert!(!ast.contains("Namespace {"), "{ast}");
    assert!(!ast.contains("Module {"), "{ast}");

    let llvm = checked(run(&[arg(&project), "--llvm".to_string()])).stdout;
    assert!(llvm.contains("define"), "{llvm}");

    let symbols = checked(run(&[arg(&project), "--symbols".to_string()])).stdout;
    assert!(symbols.contains("bank/web::Pages::render"), "{symbols}");
    assert!(!symbols.contains("__osp_"), "{symbols}");
    assert_project_main_symbol_is_local(&symbols, &project.join("src/main.ospml"));

    // An ordinary script keeps its own names: no project name mangling.
    let source = temp_project("plain_script").0.with_extension("osp");
    std::fs::write(
        &source,
        "fn historicalName(value: int) -> int = value\nprint(historicalName(7))\n",
    )
    .expect("write ordinary script");
    let llvm = checked(run(&[arg(&source), "--llvm".to_string()])).stdout;
    assert!(llvm.contains("@historicalName("), "{llvm}");
    assert!(!llvm.contains("__osp_"), "{llvm}");
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

/// A namespaced declaration reaches the type checker under its flat linkage
/// name, so a diagnostic about it must be translated back before a human sees
/// it — the same rule `--symbols` and `--llvm` already hold to. The reported
/// position is the declaring module's physical line, never a flattened one.
#[test]
fn project_type_diagnostics_map_back_to_physical_source_lines() {
    let (project, _, _) = rewritten_fixture(
        "diagnostic_names",
        "src/main.ospml",
        "serve : Ptr -> int ! [Ledger::Store, Metrics::MetricsFx, Api::Audit]",
        "serve : Ptr -> int ! [Ledger::Store, Metrics::MetricsFx]",
    );
    let output = run(&[arg(&project), "--check".to_string()]);
    assert_ne!(output.code, Some(0), "stdout={}", output.stdout);
    let rendered = format!("{}{}", output.stdout, output.stderr);
    assert!(rendered.contains("`bank::serve`"), "{rendered}");
    assert!(rendered.contains("bank::Api::Audit.log"), "{rendered}");
    assert!(!rendered.contains("__osp_"), "{rendered}");
    let _ = std::fs::remove_dir_all(project);

    let (project, original, broken) = rewritten_fixture(
        "diagnostic",
        "src/domain/money.ospml",
        "show : Amount -> string",
        "show : Amount -> int",
    );
    assert_ne!(
        broken, original,
        "fixture mutation must change the ML source"
    );
    let line = broken
        .lines()
        .position(|line| line.contains("show amount ="))
        .map_or(0, |line| line.saturating_add(1));
    let money = project.join("src/domain/money.ospml");
    let output = run(&[arg(&project), "--check".to_string()]);
    assert_ne!(output.code, Some(0));
    let expected = format!("{}:{line}:", money.display());
    assert!(output.stderr.contains(&expected), "{}", output.stderr);
    let _ = std::fs::remove_dir_all(project);
}

#[cfg(unix)]
#[test]
fn link_directives_from_a_non_entry_source_reach_the_native_driver() {
    use std::os::unix::fs::PermissionsExt;

    let project = copy_fixture("link_directives");
    let module = project.join("src/store/ledger.ospml");
    let source = std::fs::read_to_string(&module).expect("read non-entry source");
    std::fs::write(
        &module,
        format!("// @link: proof_from_non_entry\n// @linkdir: /proof/non-entry\n{source}"),
    )
    .expect("write link directives");
    let driver = project.join("record-cc.sh");
    std::fs::write(
        &driver,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$OSPREY_CC_LOG\"\n",
    )
    .expect("write recording compiler");
    let mut permissions = std::fs::metadata(&driver)
        .expect("read driver metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&driver, permissions).expect("make driver executable");
    let log = project.join("cc-args.txt");
    let output_path = project.join("ignored-output");
    let mut command = osprey();
    let _ = command
        .arg(&project)
        .arg("--compile")
        .arg("-o")
        .arg(&output_path)
        .env("OSPREY_CC", &driver)
        .env("OSPREY_CC_LOG", &log);
    let _ = checked(finish(&mut command));
    let arguments = std::fs::read_to_string(log).expect("read recorded compiler args");
    assert!(arguments.lines().any(|arg| arg == "-lproof_from_non_entry"));
    assert!(arguments.lines().any(|arg| arg == "-L/proof/non-entry"));
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn numeric_operator_errors_map_back_to_the_declaring_module() {
    let project = copy_fixture("numeric_positions");
    for (extension, source, column) in [
        (
            "osp",
            "\nfn scale(x) = x * 1.0\nfn probe() = scale(\"bad\")\n",
            16,
        ),
        (
            "ospml",
            "\nscale x = x * 1.0\nprobe () = scale \"bad\"\n",
            12,
        ),
    ] {
        let file = project.join(format!("src/numeric.{extension}"));
        std::fs::write(&file, source).expect("write numeric source");
        let output = run(&[arg(&project), "--check".to_string(), "--quiet".to_string()]);
        assert_eq!(output.code, Some(1), "{}", output.stderr);
        let expected = format!(
            "{}:2:{column}: operator `*` requires int or float; got string",
            file.display()
        );
        assert!(output.stderr.contains(&expected), "{}", output.stderr);
        std::fs::remove_file(file).expect("remove numeric source");
    }
    std::fs::remove_dir_all(project).expect("remove project fixture");
}

/// A recursively specialised function needs one return slot to emit, so the
/// backend reads it from the inferred signature. `Result`'s ERROR channel is
/// phantom — the block's slot is a fixed `i8*` ([ERR-PAYLOAD]) — so inference
/// leaves it open as the hole [TYPE-RENDER-HOLES] calls its honest spelling,
/// and a recursive `-> Result<T, _>` still has a slot to emit. Asking the whole
/// type instead refused this group and told the author to annotate the error
/// channel, which no annotation of it can change.
const RECURSIVE_RESULT_PROJECT: &str = concat!(
    "namespace rec\n",
    "\n",
    "type Tree = Leaf int | Node Tree Tree\n",
    "\n",
    "(** Rendering through a recursive group, with no written signatures. *)\n",
    "module Trees\n",
    "    (** Render `tree`. *)\n",
    "    export label tree = match tree\n",
    "        Leaf n => Success(value = toString n)\n",
    "        Node left right => join (label left, label right)\n",
    "\n",
    "    (** The other half of the group: `join` calls `label` back. *)\n",
    "    join (left, right) = match left\n",
    "        Success a => wrap (a, right)\n",
    "        Error message => Error(message = message)\n",
    "\n",
    "    wrap (a, right) = match right\n",
    "        Success b => Success(value = \"(${a} ${b})\")\n",
    "        Error message => Error(message = message)\n",
    "\n",
    "main () = match Trees::label (Node (Leaf 1) (Leaf 2))\n",
    "    Success text => print text\n",
    "    Error message => print \"error: ${message}\"\n",
);

/// The retained refusal: here the SUCCESS payload is the open slot, so the
/// emitted `Result` block has no value representation and the annotation really
/// is the answer.
const RECURSIVE_OPEN_PAYLOAD_PROJECT: &str = concat!(
    "namespace rec\n",
    "\n",
    "type N = Zero | Succ N\n",
    "\n",
    "collect (n) = match n\n",
    "    Zero => Success(value = [])\n",
    "    Succ k => collect k\n",
    "\n",
    "render (outcome) = match outcome\n",
    "    Success xs => print \"ok\"\n",
    "    Error message => print message\n",
    "\n",
    "main () = render (collect Zero)\n",
);

fn recursive_project(name: &str, source: &str) -> std::io::Result<PathBuf> {
    let project = temp_project(&format!("recursive_result_{name}")).0;
    std::fs::create_dir_all(project.join("src"))?;
    std::fs::write(
        project.join("osprey.toml"),
        "[project]\nname = \"rec\"\nsource_roots = [\"src\"]\ndefault_namespace = \"rec\"\nentry = \"src/main.ospml\"\n",
    )?;
    std::fs::write(project.join("src/main.ospml"), source)?;
    Ok(project)
}

/// The diagnostic a recursive group without an inferred return type reports.
const RECURSIVE_NEEDS_SIGNATURE: &str = "is recursive and its return type is not inferred";

#[test]
fn recursive_result_groups_need_an_annotation_only_for_an_open_payload() -> TestResult {
    let project = recursive_project("annotated", RECURSIVE_RESULT_PROJECT)?;
    for stderr in run_under_each_backend(&project, "(1 2)\n") {
        assert!(!stderr.contains(RECURSIVE_NEEDS_SIGNATURE), "{stderr}");
    }
    std::fs::remove_dir_all(project)?;

    let project = recursive_project("open", RECURSIVE_OPEN_PAYLOAD_PROJECT)?;
    let output = run(&[arg(&project), "--run".into(), "--quiet".into()]);
    assert_eq!(output.code, Some(1), "stdout={}", output.stdout);
    let stderr = &output.stderr;
    assert!(stderr.contains(RECURSIVE_NEEDS_SIGNATURE), "{stderr}");
    std::fs::remove_dir_all(project)?;
    Ok(())
}
