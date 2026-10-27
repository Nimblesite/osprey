//! Execute the Python package against the compiler built by Cargo.

const COMPREHENSION_SCOPE: &str = r#"# osprey: flavor=python
def main():
    callbacks = map(range(1), lambda _: lambda xs: len(xs) + 5)
    a = [len([1, 2]) for len in callbacks if len([1]) > 0]
    b = [range(7) for range in map(range(1), lambda _: lambda x: x + 3)]
    c = [value for len in map(range(1), lambda _: lambda xs: range(4, 6)) for value in len([4, 5])]
    d = [x for x in map(range(1), lambda _: range(6, 8)) for x in x]
    return print(f"{unwrap_or(listGet(a, 0), -1)}|{unwrap_or(listGet(b, 0), -1)}|{len(toList(range(3)))}|{unwrap_or(listGet(c, 0), -1)}|{unwrap_or(listGet(d, 0), -1)}")
"#;

#[test]
fn comprehension_aliases_execute_in_their_lexical_scope() -> std::io::Result<()> {
    let path = std::env::temp_dir().join(format!("osprey_comprehension_{}.py", std::process::id()));
    std::fs::write(&path, COMPREHENSION_SCOPE)?;
    let result = ["gc", "arc"].into_iter().try_for_each(|memory| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
            .arg(&path)
            .args(["--run", "--quiet"])
            .arg(format!("--memory={memory}"))
            .output()?;
        assert!(
            output.status.success(),
            "{memory}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"7|10|3|4|6\n", "{memory}");
        Ok(())
    });
    std::fs::remove_file(path)?;
    result
}

fn quarantine_project() -> std::io::Result<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!("osprey_jinja_fatal_{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    for (source, destination) in [
        ("packages/jinja2/src/value.py", "value.py"),
        ("packages/testing/src/check.py", "check.py"),
        ("packages/jinja2/tests/fatal_quarantine.py", "main.py"),
    ] {
        let _ = std::fs::copy(root.join(source), directory.join(destination))?;
    }
    std::fs::write(directory.join("osprey.toml"), "[project]\nname = \"jinja-fatal\"\nsource_roots = [\".\"]\npython_sources = [\"value.py\", \"check.py\", \"main.py\"]\ndefault_namespace = \"fatal_test\"\nentry = \"main.py\"\n")?;
    Ok(directory)
}

#[test]
fn jinja_quarantine_aborts_instead_of_returning_a_value() -> std::io::Result<()> {
    let directory = quarantine_project()?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(&directory)
        .args(["--run", "--quiet"])
        .output()?;
    std::fs::remove_dir_all(directory)?;
    assert_quarantine_exit(&output);
    Ok(())
}

fn assert_quarantine_exit(output: &std::process::Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "fatal channel returned successfully"
    );
    assert!(
        stderr.contains("tensor load cannot continue: Jinja quarantine must abort"),
        "{stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "fatal channel returned a rendered value"
    );
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn python_interoperability() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("python3")
        .args(["-m", "unittest", "discover", "-s", "python/tests", "-v"])
        .current_dir(&root)
        .env("PYTHONPATH", root.join("python"))
        .env("OSPREY", env!("CARGO_BIN_EXE_osprey"))
        .output()
        .expect("run Python interop tests");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn python_target_rejects_unsupported_host() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .arg(root.join("examples/python/interop.osp"))
        .args(["--check", "--target=python"])
        .output()
        .expect("check Python target");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Linux or macOS"));
}
