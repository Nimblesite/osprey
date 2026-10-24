//! Finding KLEE and the symbolic runtime bitcode [SYMEX-CLI] [SYMEX-INSTALL]
//! (docs/specs/tooling/SymbolicExecution.md). KLEE runs natively on the host;
//! there is no container in this pipeline.

use crate::linking::find_runtime_lib;
use std::path::{Path, PathBuf};

/// The runtime module `make build` links from `compiler/runtime/klee/`
/// [SYMEX-RUNTIME-BITCODE].
pub(crate) const RUNTIME_BITCODE: &str = "libosprey_runtime_klee.bc";
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

pub(crate) fn locate() -> Result<Toolchain, String> {
    let klee = find_klee().ok_or_else(|| format!("klee was not found; {INSTALL_HINT}"))?;
    let runtime = std::env::var(RUNTIME_ENV)
        .ok()
        .filter(|path| Path::new(path).is_file())
        .or_else(|| find_runtime_lib(RUNTIME_BITCODE))
        .ok_or_else(|| {
            format!("{RUNTIME_BITCODE} was not found; `make build` links it when KLEE's clang is installed")
        })?;
    Ok(Toolchain {
        klee,
        runtime: PathBuf::from(runtime),
    })
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
