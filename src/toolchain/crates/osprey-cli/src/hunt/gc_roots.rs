//! Explicit KLEE roots for the real collector [SYMEX-GC-ROOTS].
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn instrument(module: &Path, runtime: &Path) -> Result<(), String> {
    let plugin = runtime.with_file_name("libosprey_gc_roots.so");
    if !plugin.is_file() {
        return Err(format!(
            "GC root pass not found: {}; rebuild the symbolic runtime",
            plugin.display()
        ));
    }
    let opt = optimizer().ok_or("LLVM 19 opt not found; set OSPREY_KLEE_LLVM")?;
    transform(&opt, &plugin, module)
}

fn optimizer() -> Option<PathBuf> {
    std::env::var_os("OSPREY_KLEE_LLVM")
        .map(PathBuf::from)
        .into_iter()
        .chain(
            [
                "/opt/homebrew/opt/llvm@19/bin",
                "/usr/local/opt/llvm@19/bin",
                "/home/linuxbrew/.linuxbrew/opt/llvm@19/bin",
                "/usr/lib/llvm-19/bin",
            ]
            .map(PathBuf::from),
        )
        .map(|dir| dir.join("opt"))
        .find(|path| path.is_file())
}

fn transform(opt: &Path, plugin: &Path, module: &Path) -> Result<(), String> {
    let output = module.with_extension("gc.ll");
    let result = Command::new(opt)
        .arg(format!("-load-pass-plugin={}", plugin.display()))
        .args(["-passes=osprey-gc-roots", "-verify-each", "-S"])
        .arg(module)
        .arg("-o")
        .arg(&output)
        .output()
        .map_err(|error| format!("cannot run GC root pass: {error}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&output);
        return Err(format!(
            "GC root instrumentation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    std::fs::rename(&output, module).map_err(|error| format!("cannot install GC roots: {error}"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::spec_replay_tests::write_executable_fixture;
    use super::*;

    #[test]
    fn failed_pass_preserves_original_module_and_reports_diagnostic() {
        let root = std::env::temp_dir().join(format!("osprey-root-pass-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("directory");
        let module = root.join("main.ll");
        let opt = root.join("opt");
        std::fs::write(&module, "original module").expect("module");
        write_executable_fixture(&opt, b"#!/bin/sh\necho unsupported-root >&2\nexit 1\n")
            .expect("script");
        let error = transform(&opt, &root.join("pass.so"), &module).expect_err("must stop");
        assert!(error.contains("unsupported-root"));
        assert_eq!(
            std::fs::read_to_string(&module).expect("module"),
            "original module"
        );
        assert!(!module.with_extension("gc.ll").exists());
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
