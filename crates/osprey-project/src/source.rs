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

fn hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') || name == "target")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_extensions_are_exact() {
        assert!(is_source(Path::new("a.osp")));
        assert!(is_source(Path::new("a.ospml")));
        assert!(!is_source(Path::new("a.ospo")));
        assert!(hidden(Path::new("target")));
        assert!(hidden(Path::new(".cache")));
    }
}
