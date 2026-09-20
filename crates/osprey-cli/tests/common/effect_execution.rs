//! Shared runtime and ownership assertions for effect-contract tests.

use std::process::Command;

fn execute(
    name: &str,
    extension: &str,
    source: &str,
    memory: &str,
) -> std::io::Result<std::process::Output> {
    let directory =
        std::env::temp_dir().join(format!("osprey_effect_execution_{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{name}_{memory}.{extension}"));
    std::fs::write(&path, source)?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_osprey"));
    let _ = command
        .arg(&path)
        .args(["--run", &format!("--memory={memory}")]);
    if memory == "arc" {
        let _ = command.env("OSPREY_ARC_DEBUG", "1");
    } else {
        let _ = command.env_remove("OSPREY_ARC_DEBUG");
    }
    let output = command.output();
    let _ = std::fs::remove_file(&path);
    output
}

pub(crate) fn assert_flavored_output(name: &str, extension: &str, source: &str, expected: &str) {
    for memory in ["default", "gc", "arc"] {
        let result = execute(name, extension, source, memory);
        assert!(result.is_ok(), "compiler execution failed: {result:?}");
        let Ok(output) = result else { return };
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{name}/{extension}/{memory}: {stderr}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{name}/{extension}/{memory}"
        );
        if memory == "arc" {
            assert_arc_clean(&stderr);
        }
    }
}

fn assert_arc_clean(stderr: &str) {
    let reports: Vec<&str> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("[osp-arc] exit: "))
        .collect();
    assert_eq!(
        reports.len(),
        1,
        "missing or duplicate ARC exit report: {stderr}"
    );
    assert!(
        reports
            .first()
            .is_some_and(|report| report.starts_with("0 live objects, ")),
        "ARC retained live objects: {stderr}"
    );
}
