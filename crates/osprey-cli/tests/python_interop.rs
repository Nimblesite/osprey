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
