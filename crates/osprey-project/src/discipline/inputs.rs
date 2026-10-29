//! Fingerprints of a checker package's build inputs ([TYPES-DISCIPLINE-REGISTRY]):
//! the binary name and the result-cache key must change when any file the
//! build reads changes — the entry module, the manifest, every source module
//! resolution can reach, and the compiler — not only the entry module.

use super::Discipline;
use crate::ProjectConfig;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

/// The manifest naming a package's source roots.
const MANIFEST: &str = "osprey.toml";
/// The extensions the compiler reads out of a source root.
const SOURCES: [&str; 3] = ["osp", "ospml", "py"];

/// A digest over every file the checker build reads, each as its
/// package-relative path, its length and its bytes. A checker is either one
/// program file — the entry, the package manifest and every source beside it
/// or under the package's source roots — or a project directory with its own
/// `osprey.toml`, in which case that manifest and every source under the
/// roots it names are the inputs.
///
/// # Errors
/// A file that cannot be read is named in the message instead of standing in
/// as an empty digest, which would silently reuse a stale checker binary.
pub(super) fn build_digest(discipline: &Discipline) -> Result<u64, String> {
    let mut hasher = DefaultHasher::new();
    if discipline.checker.is_dir() {
        project_inputs(&mut hasher, &discipline.checker)?;
    } else {
        file_inputs(&mut hasher, discipline)?;
    }
    Ok(hasher.finish())
}

/// The inputs of a one-file checker.
fn file_inputs(hasher: &mut DefaultHasher, discipline: &Discipline) -> Result<(), String> {
    let entry = std::fs::read(&discipline.checker).map_err(|error| {
        format!(
            "has no readable checker at {}: {error}",
            discipline.checker.display()
        )
    })?;
    fold(hasher, &discipline.root, &discipline.checker, &entry);
    let manifest = discipline.root.join(MANIFEST);
    if manifest.is_file() {
        fold_file(hasher, &discipline.root, &manifest)?;
    }
    let beside = discipline.checker.parent().map(Path::to_path_buf);
    for file in sources(&discipline.root, &manifest, beside)? {
        fold_file(hasher, &discipline.root, &file)?;
    }
    Ok(())
}

/// The inputs of a checker that is a project directory: its manifest, which
/// must exist, and every source under the roots the manifest names.
fn project_inputs(hasher: &mut DefaultHasher, root: &Path) -> Result<(), String> {
    let manifest = root.join(MANIFEST);
    if !manifest.is_file() {
        return Err(format!(
            "has a checker directory without a manifest: {} is missing",
            manifest.display()
        ));
    }
    fold_file(hasher, root, &manifest)?;
    for file in sources(root, &manifest, None)? {
        fold_file(hasher, root, &file)?;
    }
    Ok(())
}

/// A digest of the compiler a checker is built with, so a compiler upgrade
/// rebuilds the checker instead of reusing a binary it did not produce.
///
/// Measured against a 15 MB `target/release/osprey`: 4–8 ms per call for the
/// read plus the hash, well inside the 50 ms budget, so the whole binary is
/// hashed. Truncating to the first and last MiB would only buy microseconds
/// here and would weaken the identity, so it is not done.
///
/// # Errors
/// A compiler that cannot be read is named in the message.
pub(super) fn compiler_digest(compiler: &Path) -> Result<u64, String> {
    let bytes = std::fs::read(compiler).map_err(|error| {
        format!(
            "cannot read the compiler at {}: {error}",
            compiler.display()
        )
    })?;
    Ok(super::cache::digest(&bytes))
}

/// Every source file the checker build can read: those under the roots the
/// manifest at `manifest` names (or the defaults when there is none), plus
/// those under `extra` — beside a one-file checker's entry, which is where
/// module resolution looks. Sorted and deduplicated, so the order is a
/// function of the tree and one file is never counted twice.
fn sources(root: &Path, manifest: &Path, extra: Option<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let mut roots = roots(root, manifest)?;
    roots.extend(extra);
    let mut files = Vec::new();
    for found in roots {
        visit(&root.join(found), &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// The source roots the compiler would read the package from.
fn roots(root: &Path, manifest: &Path) -> Result<Vec<PathBuf>, String> {
    if !manifest.is_file() {
        return Ok(ProjectConfig::for_root(root).source_roots);
    }
    let text = std::fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    ProjectConfig::parse(&text, manifest)
        .map(|config| config.source_roots)
        .map_err(|errors| {
            format!(
                "cannot read the source roots of {}: {}",
                manifest.display(),
                errors.first().map_or("", |error| error.message.as_str())
            )
        })
}

fn visit(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_source(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    let unreadable = |error: std::io::Error| format!("cannot list {}: {error}", path.display());
    let entries = std::fs::read_dir(path).map_err(unreadable)?;
    for entry in entries {
        let found = entry.map_err(unreadable)?.path();
        if !hidden(&found) {
            visit(&found, out)?;
        }
    }
    Ok(())
}

fn is_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| SOURCES.contains(&extension))
}

fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn fold_file(hasher: &mut DefaultHasher, base: &Path, file: &Path) -> Result<(), String> {
    let bytes =
        std::fs::read(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    fold(hasher, base, file, &bytes);
    Ok(())
}

/// Path, length and bytes, so neither a rename nor a re-chunk can collide.
fn fold(hasher: &mut DefaultHasher, base: &Path, file: &Path, bytes: &[u8]) {
    file.strip_prefix(base).unwrap_or(file).hash(hasher);
    bytes.len().hash(hasher);
    bytes.hash(hasher);
}
