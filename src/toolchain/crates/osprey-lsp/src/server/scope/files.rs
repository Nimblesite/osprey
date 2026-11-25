//! The source files a diagnostics scope covers on disk [LSP-DIAGNOSTICS-SCOPE].

use crate::workspace::{file_path, uri_of};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

pub(super) fn repository(folder: &Path) -> PathBuf {
    folder
        .ancestors()
        .find(|path| path.join(".git").exists())
        .unwrap_or(folder)
        .to_path_buf()
}

pub(super) fn resolve_root(root: &str, folders: &[PathBuf]) -> Vec<PathBuf> {
    if let Some(path) = file_path(root) {
        return vec![path];
    }
    let path = Path::new(root);
    if path.is_absolute() {
        return vec![path.to_path_buf()];
    }
    folders.iter().map(|folder| folder.join(path)).collect()
}

/// The files `git` lists under `root`: tracked ones plus untracked files no
/// ignore rule matches — the same set the CLI's program gate reads with
/// `git ls-files` ([CI-PROGRAMS-COMPILE]). `None` when `root` is outside any
/// git work tree or git cannot answer, so a plain folder keeps every file.
pub(super) fn unignored(root: &Path) -> Option<HashSet<PathBuf>> {
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .ok()?;
    listed.status.success().then(|| {
        listed
            .stdout
            .split(|&byte| byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| root.join(String::from_utf8_lossy(path).into_owned()))
            .collect()
    })
}

pub(super) fn discover(
    root: &Path,
    unignored: Option<&HashSet<PathBuf>>,
    documents: &mut BTreeSet<String>,
) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("osprey lsp: cannot scan {}: {error}", root.display());
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => visit(&entry, unignored, documents),
            Err(error) => eprintln!("osprey lsp: cannot read {}: {error}", root.display()),
        }
    }
}

fn visit(
    entry: &std::fs::DirEntry,
    unignored: Option<&HashSet<PathBuf>>,
    documents: &mut BTreeSet<String>,
) {
    let path = entry.path();
    let kind = match entry.file_type() {
        Ok(kind) => kind,
        Err(error) => {
            eprintln!("osprey lsp: cannot inspect {}: {error}", path.display());
            return;
        }
    };
    if kind.is_dir() && !excluded(&path) {
        discover(&path, unignored, documents);
    }
    if kind.is_file() && source_file(&path) && unignored.is_none_or(|files| files.contains(&path)) {
        if let Some(uri) = uri_of(&path) {
            let _ = documents.insert(uri);
        }
    }
}

fn excluded(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with('.')
                || matches!(
                    name,
                    "target" | "node_modules" | "build" | "dist" | "__pycache__"
                )
        })
}

fn source_file(path: &Path) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("osp" | "ospml") => true,
        Some("py") => osprey_project::python_source(path),
        _ => false,
    }
}
