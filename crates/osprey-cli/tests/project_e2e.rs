//! End-to-end coverage for mixed-flavor project CLI entry points.
#![expect(
    clippy::expect_used,
    reason = "subprocess and fixture setup failures must fail these end-to-end tests immediately"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn fixture() -> PathBuf {
    repo_root().join("examples/projects/modules")
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

fn copy_fixture(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("osprey_project_e2e_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for relative in FIXTURE_SOURCES {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).expect("create project directories");
        }
        copy(relative, &destination);
    }
    root
}

fn expected_output() -> String {
    std::fs::read_to_string(fixture().join("expectedoutput")).expect("read expectedoutput fixture")
}

/// The Talon Bank demo binds one fixed HTTP port and one `SQLite` file, so
/// the tests that actually execute it must not overlap in time.
fn demo_run_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let _ = std::fs::remove_file("/tmp/talon_bank.hold");
    match LOCK.get_or_init(std::sync::Mutex::default).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn copy(relative: &str, destination: &Path) {
    let _ = std::fs::copy(fixture().join(relative), destination).expect("copy project fixture");
}

#[test]
fn directory_and_manifest_inputs_share_the_flat_project_pipeline() {
    let project = fixture();
    let manifest = project.join("osprey.toml");
    for path in [&project, &manifest] {
        let output = run(&[arg(path), "--check".to_string(), "--quiet".to_string()]);
        assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    }

    let ast = run(&[arg(&project), "--ast".to_string()]);
    assert_eq!(ast.code, Some(0), "stderr={}", ast.stderr);
    assert!(!ast.stdout.contains("Import("), "{}", ast.stdout);
    assert!(!ast.stdout.contains("Namespace {"), "{}", ast.stdout);
    assert!(!ast.stdout.contains("Module {"), "{}", ast.stdout);

    let llvm = run(&[arg(&project), "--llvm".to_string()]);
    assert_eq!(llvm.code, Some(0), "stderr={}", llvm.stderr);
    assert!(llvm.stdout.contains("define"), "{}", llvm.stdout);

    let symbols = run(&[arg(&project), "--symbols".to_string()]);
    assert_eq!(symbols.code, Some(0), "stderr={}", symbols.stderr);
    assert!(
        symbols.stdout.contains("bank/web::Pages::render"),
        "{}",
        symbols.stdout
    );
    assert!(!symbols.stdout.contains("__osp_"), "{}", symbols.stdout);
    assert_project_main_symbol_is_local(&symbols.stdout, &project.join("src/main.ospml"));
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
    assert_eq!(
        main.and_then(|entry| entry.get("line"))
            .and_then(serde_json::Value::as_u64),
        expected_line
    );
    assert_eq!(
        main.and_then(|entry| entry.get("path"))
            .and_then(serde_json::Value::as_str),
        std::fs::canonicalize(source)
            .ok()
            .as_deref()
            .and_then(Path::to_str)
    );
}

#[test]
fn bank_project_runs_byte_exact_against_expectedoutput() {
    let _serialized = demo_run_lock();
    let project = fixture();
    let output = run(&[arg(&project), "--run".to_string()]);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    assert_eq!(output.stdout, expected_output());
}

#[test]
fn module_aware_single_file_uses_assembly_without_loading_siblings() {
    let directory =
        std::env::temp_dir().join(format!("osprey_single_module_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create single-source directory");
    let source = directory.join("main.osp");
    let namespace = directory
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temporary directory has an identifier name");
    std::fs::write(
        &source,
        format!(
            concat!(
                "module Answers {{\n",
                "    export fn answer() -> int = 42\n",
                "}}\n",
                "import {}::Answers::{{answer}}\n",
                "fn main() = print(\"answer=${{answer()}}\")\n",
            ),
            namespace
        ),
    )
    .expect("write single module source");
    std::fs::write(directory.join("broken.osp"), "fn = = =\n").expect("write ignored sibling");

    let output = run(&[arg(&source), "--run".to_string()]);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    assert_eq!(output.stdout, "answer=42\n");
    let relative = run_in(&directory, &["main.osp", "--check", "--quiet"]);
    assert_eq!(relative.code, Some(0), "stderr={}", relative.stderr);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn manifest_free_project_identity_is_path_spelling_independent() {
    let directory =
        std::env::temp_dir().join(format!("osprey_manifest_free_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let sources = directory.join("src");
    std::fs::create_dir_all(&sources).expect("create manifest-free source root");
    let namespace = directory
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temporary directory has an identifier name");
    std::fs::write(
        sources.join("values.osp"),
        "module Answers {\n    export fn answer() -> int = 42\n}\n",
    )
    .expect("write module source");
    std::fs::write(
        sources.join("main.osp"),
        format!("import {namespace}::Answers::{{answer}}\nfn main() = print(\"${{answer()}}\")\n"),
    )
    .expect("write entry source");

    let absolute = run(&[
        arg(&directory),
        "--check".to_string(),
        "--quiet".to_string(),
    ]);
    assert_eq!(absolute.code, Some(0), "stderr={}", absolute.stderr);
    let relative = run_in(&directory, &[".", "--check", "--quiet"]);
    assert_eq!(relative.code, Some(0), "stderr={}", relative.stderr);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn ordinary_script_bypasses_project_name_mangling() {
    let source = std::env::temp_dir().join(format!(
        "osprey_plain_script_e2e_{}.osp",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "fn historicalName(value: int) -> int = value\nprint(historicalName(7))\n",
    )
    .expect("write ordinary script");
    let output = run(&[arg(&source), "--llvm".to_string()]);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    assert!(
        output.stdout.contains("@historicalName("),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains("__osp_"), "{}", output.stdout);
    let _ = std::fs::remove_file(source);
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
    let output = finish(&mut command);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    let arguments = std::fs::read_to_string(log).expect("read recorded compiler args");
    assert!(arguments.lines().any(|arg| arg == "-lproof_from_non_entry"));
    assert!(arguments.lines().any(|arg| arg == "-L/proof/non-entry"));
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn build_subcommand_uses_project_name_inside_project_directory() {
    let _serialized = demo_run_lock();
    let project = copy_fixture("build");
    let artifact = project.join("talon-bank");
    let output = run_in(&project, &["build", "--quiet"]);
    assert_eq!(output.code, Some(0), "stderr={}", output.stderr);
    assert!(artifact.is_file(), "missing {}", artifact.display());

    let executed = Command::new(&artifact).output().expect("run built project");
    assert!(executed.status.success());
    assert_eq!(String::from_utf8_lossy(&executed.stdout), expected_output());
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn flattened_type_errors_map_back_to_physical_local_lines() {
    let project = copy_fixture("diagnostic");
    let money = project.join("src/domain/money.ospml");
    let source = std::fs::read_to_string(&money).expect("read ML source");
    let broken = source.replace("show : int -> string", "show : int -> int");
    assert_ne!(broken, source, "fixture mutation must change the ML source");
    let line = broken
        .lines()
        .position(|line| line.contains("show cents ="))
        .map_or(0, |line| line.saturating_add(1));
    std::fs::write(&money, broken).expect("write broken ML source");

    let output = run(&[arg(&project), "--check".to_string()]);
    assert_ne!(output.code, Some(0));
    let expected = format!("{}:{line}:", money.display());
    assert!(output.stderr.contains(&expected), "{}", output.stderr);
    let _ = std::fs::remove_dir_all(project);
}
