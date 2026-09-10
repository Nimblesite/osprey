//! Helpers shared by the two gates over `docs/specs/0035-StagedEffects.md`.
//!
//! Stage and multiplicity are two axes of ONE declaration, so both suites
//! compile the same way and read the same diagnostics. One copy here, so a
//! change to how a staged program reaches the checker reaches both.

use osprey_codegen::compile_program;
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Parse — which discharges static handlers at the flavor boundary
/// ([STAGE-LOWER-ORDER-PHASE]) — and emit LLVM IR.
pub(crate) fn compile_staged(source: &str) -> String {
    let parsed = parse_program_with_flavor(source, Flavor::Default);
    assert!(
        parsed.errors.is_empty(),
        "parse errors: {:?}",
        parsed.errors
    );
    let compiled = compile_program(&parsed.program);
    assert!(compiled.is_ok(), "codegen failed: {:?}", compiled.err());
    compiled.unwrap_or_default()
}

/// Every diagnostic the frontend produces for `source`, joined for matching.
pub(crate) fn diagnostics(source: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(source, flavor);
    if !parsed.errors.is_empty() {
        return parsed
            .errors
            .iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join("\n");
    }
    osprey_types::check_program(&parsed.program)
        .iter()
        .map(|e| e.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compile `source` for `target` through the real CLI, returning stderr and
/// whether it succeeded — the only path that runs the per-target capability
/// gate [MULTI-WASM] and [STAGE-WASM] are checked by.
pub(crate) fn compile_for_target(source: &str, target: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!(
        "osprey_staged_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let Ok(()) = std::fs::create_dir_all(&dir) else {
        return (false, "could not create temp dir".to_string());
    };
    let input = dir.join("staged.osp");
    let _ = std::fs::write(&input, source);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args([input.as_os_str(), "--llvm".as_ref()])
        .arg(format!("--target={target}"))
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    match output {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(e) => (false, e.to_string()),
    }
}
