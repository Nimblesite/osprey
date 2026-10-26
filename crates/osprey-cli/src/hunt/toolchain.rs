//! Finding KLEE and the symbolic runtime bitcode [SYMEX-CLI] [SYMEX-INSTALL]
//! (docs/specs/tooling/SymbolicExecution.md). KLEE runs natively on the host;
//! there is no container in this pipeline.

use crate::linking::find_runtime_lib;
use std::path::{Path, PathBuf};

/// The runtime module `make build` links from `compiler/runtime/klee/`
/// [SYMEX-RUNTIME-BITCODE].
/// Points at the `klee` executable; `PATH` and the Homebrew prefixes are the
/// fallbacks.
pub(crate) const KLEE_ENV: &str = "OSPREY_KLEE";
/// Points at the runtime bitcode when it is not beside the compiler.
pub(crate) const RUNTIME_ENV: &str = "OSPREY_KLEE_RUNTIME";
pub(crate) const INSTALL_HINT: &str =
    "install KLEE 3.3 with `brew install klee` (macOS, Linux) or set OSPREY_KLEE to the klee binary";

/// Everything a hunt invokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Toolchain {
    pub(crate) klee: PathBuf,
    pub(crate) runtime: PathBuf,
}

pub(crate) fn locate(memory: &str) -> Result<Toolchain, String> {
    let runtime_bitcode = match memory {
        "gc" => "libosprey_runtime_klee_gc.bc",
        "arc" => "libosprey_runtime_klee_arc.bc",
        _ => return Err("Hunt requires the gc or arc memory backend".into()),
    };
    let klee = find_klee().ok_or_else(|| format!("klee was not found; {INSTALL_HINT}"))?;
    let runtime = find_runtime(runtime_bitcode)?;
    Ok(Toolchain {
        klee,
        runtime: PathBuf::from(runtime),
    })
}

fn find_runtime(expected: &str) -> Result<String, String> {
    if let Ok(explicit) = std::env::var(RUNTIME_ENV) {
        return validate_runtime(&explicit, expected).map(|()| explicit);
    }
    find_runtime_lib(expected)
        .ok_or_else(|| format!("{expected} was not found; build the symbolic runtime"))
}

fn validate_runtime(path: &str, expected: &str) -> Result<(), String> {
    if Path::new(path)
        .file_name()
        .is_none_or(|name| name != expected)
    {
        return Err(format!(
            "{RUNTIME_ENV} must name {expected} for the selected memory backend"
        ));
    }
    if !Path::new(path).is_file() {
        return Err(format!("symbolic runtime does not exist: {path}"));
    }
    Ok(())
}

fn find_klee() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let directories = std::env::split_paths(&path)
        .chain(brew_prefixes().iter().map(PathBuf::from))
        .collect::<Vec<_>>();
    find_klee_in(std::env::var(KLEE_ENV).ok(), &directories)
}

fn find_klee_in(explicit: Option<String>, directories: &[PathBuf]) -> Option<PathBuf> {
    if let Some(explicit) = explicit {
        return Some(PathBuf::from(explicit)).filter(|path| path.is_file());
    }
    directories
        .iter()
        .map(|directory| directory.join("klee"))
        .find(|candidate| candidate.is_file())
}

/// Homebrew is often absent from a GUI editor's `PATH`, so the usual prefixes
/// are probed after it.
fn brew_prefixes() -> [&'static str; 3] {
    [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_runtime_cannot_silently_select_another_backend() {
        let error = validate_runtime(
            "libosprey_runtime_klee_gc.bc",
            "libosprey_runtime_klee_arc.bc",
        )
        .expect_err("GC cannot stand in for ARC");
        assert!(error.contains("selected memory backend"));
        assert!(validate_runtime(
            "/missing/libosprey_runtime_klee_gc.bc",
            "libosprey_runtime_klee_gc.bc"
        )
        .is_err());
    }

    #[test]
    fn an_explicit_klee_must_exist() {
        let missing = std::env::temp_dir().join("osprey-hunt-no-such-klee");
        assert_eq!(find_klee_in(Some(missing.display().to_string()), &[]), None);
        let manifest = env!("CARGO_MANIFEST_DIR").to_string() + "/Cargo.toml";
        assert!(find_klee_in(Some(manifest), &[]).is_some_and(|klee| klee.ends_with("Cargo.toml")));
    }

    #[test]
    fn path_entries_are_searched_for_klee() {
        let dir = std::env::temp_dir().join(format!("osprey-hunt-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("klee"), "#!/bin/sh\n").expect("fake klee");
        let directories = [PathBuf::from("/nonexistent-dir"), dir.clone()];
        assert_eq!(find_klee_in(None, &directories), Some(dir.join("klee")));
        assert_eq!(find_klee_in(None, &directories[..1]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
