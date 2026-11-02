//! Deterministic source-root discovery for mixed-flavor projects.

use crate::{ProjectConfig, ProjectError};
use std::path::{Path, PathBuf};

/// Discover Osprey sources and explicitly selected Python-flavor files.
pub(crate) fn discover(root: &Path, config: &ProjectConfig) -> Result<Vec<PathBuf>, ProjectError> {
    let mut paths = Vec::new();
    for source_root in &config.source_roots {
        visit(&root.join(source_root), &mut paths)?;
    }
    paths.extend(python_sources(root, config)?);
    validate_flavor(config, &paths)?;
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
    (is_source(&path) || python_selected(root, config, &path)) && inside_roots(root, config, &path)
}

fn inside_roots(root: &Path, config: &ProjectConfig, path: &Path) -> bool {
    config.source_roots.iter().any(|source_root| {
        let source_root = normalize(&root.join(source_root));
        path == source_root
            || path.strip_prefix(&source_root).is_ok_and(|relative| {
                relative
                    .components()
                    .all(|part| !hidden(Path::new(part.as_os_str())))
            })
    })
}

fn python_selected(root: &Path, config: &ProjectConfig, path: &Path) -> bool {
    is_python(path)
        && config
            .python_sources
            .iter()
            .chain(config.entry.iter().filter(|entry| is_python(entry)))
            .any(|selected| normalize(&root.join(selected)) == path)
}

fn validate_flavor(config: &ProjectConfig, paths: &[PathBuf]) -> Result<(), ProjectError> {
    if config.flavor == Some(osprey_syntax::Flavor::Python) {
        if let Some(path) = paths.iter().find(|path| is_source(path)) {
            return Err(selection_error(
                path,
                "Python project flavor conflicts with mixed-flavor source; select flavors per file",
            ));
        }
    }
    Ok(())
}

fn is_python(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "py")
}

fn python_sources(root: &Path, config: &ProjectConfig) -> Result<Vec<PathBuf>, ProjectError> {
    let mut paths = std::collections::BTreeSet::new();
    for source in &config.python_sources {
        let path = validate_python(root, config, source)?;
        if !paths.insert(path.clone()) {
            return Err(selection_error(&path, "duplicate Python source"));
        }
    }
    if let Some(entry) = config.entry.as_deref().filter(|path| is_python(path)) {
        let _ = paths.insert(validate_python(root, config, entry)?);
    }
    Ok(paths.into_iter().collect())
}

fn validate_python(
    root: &Path,
    config: &ProjectConfig,
    source: &Path,
) -> Result<PathBuf, ProjectError> {
    let path = selected_python_path(root, config, source)?;
    let canonical = path
        .canonicalize()
        .map_err(|error| ProjectError::io(&path, &error))?;
    if !path.is_file() || !canonical_in_roots(root, config, &canonical) {
        return Err(selection_error(
            &path,
            "Python source resolves outside its source root or is not a file",
        ));
    }
    Ok(canonical)
}

fn selected_python_path(
    root: &Path,
    config: &ProjectConfig,
    source: &Path,
) -> Result<PathBuf, ProjectError> {
    let path = normalize(&root.join(source));
    if source.is_absolute() || !is_python(&path) || !inside_roots(root, config, &path) {
        return Err(selection_error(
            &path,
            "Python source must be a manifest-relative .py file inside a declared source root",
        ));
    }
    selected_flavor(config, &path)?;
    Ok(path)
}

fn selected_flavor(config: &ProjectConfig, path: &Path) -> Result<(), ProjectError> {
    if config
        .flavor
        .is_some_and(|flavor| flavor != osprey_syntax::Flavor::Python)
    {
        return Err(selection_error(
            path,
            "project flavor conflicts with selected Python source",
        ));
    }
    Ok(())
}

fn canonical_in_roots(root: &Path, config: &ProjectConfig, path: &Path) -> bool {
    config.source_roots.iter().any(|source| {
        root.join(source)
            .canonicalize()
            .is_ok_and(|source| path == source || path.starts_with(source))
    })
}

fn selection_error(path: &Path, message: &str) -> ProjectError {
    ProjectError {
        message: message.into(),
        path: Some(path.into()),
        line: None,
        column: None,
    }
}

/// Whether `path` lies under `directory`, however either spells `..`.
pub(crate) fn within(directory: &Path, path: &Path) -> bool {
    let directory = normalize(directory);
    if directory == Path::new(".") {
        return !path
            .components()
            .any(|part| part == std::path::Component::ParentDir);
    }
    normalize(path).starts_with(directory)
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

    fn python_fixture(name: &str) -> std::io::Result<PathBuf> {
        let root = std::env::temp_dir().join(format!(
            "osprey_python_sources_{}_{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))?;
        for path in [
            "src/main.py",
            "src/model.py",
            "src/reference.py",
            "src/tool.py",
            "src/normal.osp",
            "outside.py",
        ] {
            std::fs::write(root.join(path), "deliberately invalid source")?;
        }
        Ok(root)
    }

    #[test]
    fn python_discovery_keeps_reference_and_tools_out_of_the_project() -> std::io::Result<()> {
        let root = python_fixture("selected")?;
        let mut config = ProjectConfig::for_root(&root);
        config.python_sources = vec!["src/model.py".into(), "src/main.py".into()];
        config.entry = Some("src/main.py".into());
        let result = discover(&root, &config);
        std::fs::remove_dir_all(&root)?;
        assert!(result.is_ok(), "{result:?}");
        if let Ok(paths) = result {
            assert_eq!(paths.len(), 3);
            assert!(!paths
                .iter()
                .any(|path| path.ends_with("reference.py") || path.ends_with("tool.py")));
        }
        Ok(())
    }

    #[test]
    fn python_discovery_rejects_invalid_or_duplicate_selections() -> std::io::Result<()> {
        let root = python_fixture("invalid")?;
        let mut config = ProjectConfig::for_root(&root);
        for paths in [
            vec!["src/missing.py"],
            vec!["src/normal.osp"],
            vec!["outside.py"],
            vec!["src/model.py", "src/./model.py"],
        ] {
            config.python_sources = paths.into_iter().map(PathBuf::from).collect();
            assert!(
                discover(&root, &config).is_err(),
                "accepted {:?}",
                config.python_sources
            );
        }
        std::fs::remove_dir_all(root)
    }

    #[test]
    fn python_discovery_rejects_conflicting_flavor_overrides() -> std::io::Result<()> {
        let root = python_fixture("overrides")?;
        let mut config = ProjectConfig::for_root(&root);
        config.python_sources = vec!["src/model.py".into()];
        config.flavor = Some(osprey_syntax::Flavor::Ml);
        assert!(discover(&root, &config).is_err());
        config.flavor = Some(osprey_syntax::Flavor::Python);
        assert!(
            discover(&root, &config).is_err(),
            "override would parse .osp as Python"
        );
        std::fs::remove_dir_all(root)
    }

    #[cfg(unix)]
    #[test]
    fn python_selected_symlinks_cannot_escape_the_source_root() -> std::io::Result<()> {
        let root = python_fixture("symlinks")?;
        std::os::unix::fs::symlink(root.join("outside.py"), root.join("src/escape.py"))?;
        let mut config = ProjectConfig::for_root(&root);
        config.python_sources = vec!["src/escape.py".into()];
        assert!(discover(&root, &config).is_err());
        std::fs::remove_dir_all(root)
    }

    #[test]
    fn python_selection_paths_must_be_relative_to_the_manifest() -> std::io::Result<()> {
        let root = python_fixture("absolute")?;
        let mut config = ProjectConfig::for_root(&root);
        config.python_sources = vec![root.join("src/model.py")];
        assert!(discover(&root, &config).is_err());
        std::fs::remove_dir_all(root)
    }

    #[test]
    fn python_source_membership_requires_an_explicit_manifest_selection(
    ) -> Result<(), Vec<ProjectError>> {
        let root = Path::new("workspace");
        let config = ProjectConfig::parse("[project]\nsource_roots = [\"src\"]\npython_sources = [\"src/model.py\"]\nentry = \"src/main.py\"\n", &root.join("osprey.toml"))?;
        for (relative, selected) in [
            ("src/model.py", true),
            ("src/main.py", true),
            ("src/reference.py", false),
            ("src/tool.py", false),
            ("src/../model.py", false),
        ] {
            assert_eq!(
                config.contains_source(root, &root.join(relative)),
                selected,
                "{relative}"
            );
        }
        Ok(())
    }
}
