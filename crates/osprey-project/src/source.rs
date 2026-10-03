//! Deterministic source-root discovery for mixed-flavor projects.

use crate::{ProjectConfig, ProjectError};
use std::path::{Path, PathBuf};

/// Discover every `.osp` and `.ospml` source under configured roots.
pub(crate) fn discover(root: &Path, config: &ProjectConfig) -> Result<Vec<PathBuf>, ProjectError> {
    let mut paths = Vec::new();
    for source_root in &config.source_roots {
        visit(&root.join(source_root), &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn visit(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), ProjectError> {
    if path.is_file() {
        if is_source(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    let entries = std::fs::read_dir(path).map_err(|error| ProjectError::io(path, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| ProjectError::io(path, &error))?;
        if !hidden(&entry.path()) {
            visit(&entry.path(), out)?;
        }
    }
    Ok(())
}

fn is_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "osp" || extension == "ospml")
}

pub(crate) fn contains(root: &Path, config: &ProjectConfig, path: &Path) -> bool {
    let path = normalize(path);
    is_source(&path)
        && config.source_roots.iter().any(|source_root| {
            let source_root = normalize(&root.join(source_root));
            path == source_root
                || path.strip_prefix(&source_root).is_ok_and(|relative| {
                    relative
                        .components()
                        .all(|part| !hidden(Path::new(part.as_os_str())))
                })
        })
}

fn normalize(path: &Path) -> PathBuf {
    path.components().fold(PathBuf::new(), |mut result, part| {
        if part == std::path::Component::ParentDir {
            let _ = result.pop();
        } else {
            result.push(part);
        }
        result
    })
}

fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') || name == "target")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsaved_source_membership_uses_the_discovery_rules() {
        let root = Path::new("workspace");
        let mut config = ProjectConfig::for_root(root);
        config.source_roots = vec![PathBuf::from("src"), PathBuf::from("generated/only.osp")];
        for (relative, expected) in [
            ("src/new.osp", true),
            ("src/new.ospml", true),
            ("src/new.txt", false),
            ("src/.hidden/new.osp", false),
            ("src/target/new.osp", false),
            ("src/../private/new.osp", false),
            ("generated/only.osp", true),
            ("generated/other.osp", false),
        ] {
            assert_eq!(
                config.contains_source(root, &root.join(relative)),
                expected,
                "{relative}"
            );
        }
    }

    #[test]
    fn source_paths_are_classified() {
        let cases = [
            ("a.osp", true, false),
            ("a.ospml", true, false),
            ("a.ospo", false, false),
            ("target", false, true),
            (".cache", false, true),
        ];
        for (path, source, ignored) in cases {
            assert_eq!(
                (is_source(Path::new(path)), hidden(Path::new(path))),
                (source, ignored)
            );
        }
    }
}
