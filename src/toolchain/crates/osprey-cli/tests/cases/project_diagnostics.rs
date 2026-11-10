//! Project diagnostics map back to the declaring module's physical lines,
//! link directives from any source reach the driver, and recursive result
//! groups need an annotation only for an open payload.
use std::path::PathBuf;

use super::{
    arg, checked, copy_fixture, finish, osprey, rewritten_fixture, run, run_under_each_backend,
    temp_project, TestResult,
};

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
