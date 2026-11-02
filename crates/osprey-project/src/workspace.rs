//! Workspace packages: a dependency's source beside the package naming it.
//! Implements [PACKAGE-WORKSPACE].

use crate::discipline::INSTALL_ROOT;
use crate::manifest::Dependency;
use crate::manifest_codec as codec;
use crate::ProjectConfig;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Adds the source of every dependency, transitively, that sits beside `root`
/// under the key its own manifest declares. A declared `path` is a discipline's
/// and an installed release is the package client's, so neither is touched.
pub(crate) fn adopt(root: &Path, config: &mut ProjectConfig) {
    let mut pending = config.dependencies.clone();
    let mut seen = BTreeSet::new();
    while let Some(dependency) = pending.pop() {
        if !seen.insert(dependency.name.clone()) || !is_workspace(root, &dependency) {
            continue;
        }
        let Some((directory, sibling)) = sibling(root, &dependency.name) else {
            continue;
        };
        let prefix = Path::new("..").join(directory);
        extend(&mut config.source_roots, &prefix, &sibling.source_roots);
        extend(&mut config.python_sources, &prefix, &sibling.python_sources);
        pending.extend(sibling.dependencies);
    }
}

fn is_workspace(root: &Path, dependency: &Dependency) -> bool {
    dependency.path.is_none() && !root.join(INSTALL_ROOT).join(&dependency.name).exists()
}

/// The directory named by the key's last segment beside `root`, when its
/// manifest declares exactly that key: a directory name alone proves nothing.
fn sibling(root: &Path, key: &str) -> Option<(String, ProjectConfig)> {
    let directory = key.rsplit('/').next()?;
    let package = root.join("..").join(directory);
    if package.canonicalize().ok()? == root.canonicalize().ok()? {
        return None;
    }
    let manifest = package.join("osprey.toml");
    let text = std::fs::read_to_string(&manifest).ok()?;
    (package_key(&text).as_deref() == Some(key))
        .then(|| ProjectConfig::parse(&text, &manifest).ok())
        .flatten()
        .map(|config| (directory.to_string(), config))
}

/// `[package].name`, which the compiler otherwise leaves to the package client.
fn package_key(manifest: &str) -> Option<String> {
    let mut section = "";
    for line in manifest.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim();
        } else if section == "package" {
            if let Some(("name", value)) =
                codec::split_assignment(line).map(|(key, value)| (key.trim(), value.trim()))
            {
                return codec::parse_string(value).ok();
            }
        }
    }
    None
}

fn extend(listed: &mut Vec<PathBuf>, prefix: &Path, adopted: &[PathBuf]) {
    for path in adopted.iter().map(|path| prefix.join(path)) {
        if !listed.contains(&path) {
            listed.push(path);
        }
    }
}
