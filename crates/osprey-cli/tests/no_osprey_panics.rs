//! Osprey has no panic, abort or exit ([FAILURE-EXPLICIT]): no package or example
//! may bind a native function that ends the program in place of returning an
//! error. Every Osprey source under `packages/` and `examples/` is scanned for an
//! `extern` (Default and ML) or `@extern` (Python flavor) declaration of one.

use std::path::{Path, PathBuf};

const ROOTS: [&str; 2] = ["packages", "examples"];
const SOURCE_EXTENSIONS: [&str; 3] = ["osp", "ospml", "py"];
const SKIPPED_DIRECTORIES: [&str; 6] = [
    ".osprey",
    ".deslop",
    "node_modules",
    "target",
    "__pycache__",
    ".git",
];
const ENDING_NAMES: [&str; 4] = ["abort", "exit", "_exit", "quick_exit"];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources(directory: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() && !SKIPPED_DIRECTORIES.contains(&name) {
            sources(&path, found)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SOURCE_EXTENSIONS.contains(&e))
        {
            found.push(path);
        }
    }
    Ok(())
}

/// The name an extern line declares: `extern fn name(`, `extern name ->` or, after
/// a Python `@extern` line, `def name(`.
fn declared_name(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let rest = rest
        .strip_prefix("extern fn ")
        .or_else(|| rest.strip_prefix("extern "))
        .or_else(|| rest.strip_prefix("def "))?;
    rest.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
}

fn ends_the_program(name: &str) -> bool {
    name.contains("panic") || ENDING_NAMES.contains(&name)
}

/// Every `file:line name` that binds a program-ending native function.
fn bindings(path: &Path, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(i, line)| {
            let python_extern = i
                .checked_sub(1)
                .and_then(|previous| lines.get(previous))
                .is_some_and(|previous| previous.trim() == "@extern");
            line.trim_start().starts_with("extern") || python_extern
        })
        .filter_map(|(i, line)| {
            declared_name(line)
                .filter(|n| ends_the_program(n))
                .map(|n| format!("{}:{} {n}", path.display(), i + 1))
        })
        .collect()
}

#[test]
fn no_package_or_example_binds_a_native_panic_abort_or_exit() -> std::io::Result<()> {
    let mut found = Vec::new();
    for root in ROOTS {
        sources(&repository().join(root), &mut found)?;
    }
    let mut violations = Vec::new();
    for path in &found {
        violations.extend(bindings(path, &std::fs::read_to_string(path)?));
    }
    assert!(found.len() > 100, "scanned only {} sources", found.len());
    assert!(
        violations.is_empty(),
        "Osprey code may not panic, abort or exit:\n{}",
        violations.join("\n")
    );
    Ok(())
}

#[test]
fn the_scan_recognises_every_flavor_of_binding() {
    let path = Path::new("probe");
    assert_eq!(
        bindings(path, "extern fn osprey_gpu_panic(message: string) -> int").len(),
        1
    );
    assert_eq!(bindings(path, "extern abort -> int").len(), 1);
    assert_eq!(
        bindings(
            path,
            "@extern\ndef osprey_tensor_panic(message: str) -> str: ..."
        )
        .len(),
        1
    );
    assert_eq!(
        bindings(
            path,
            "extern fn puts(text: string) -> int\ndef exit_code(x): ..."
        )
        .len(),
        0
    );
}
