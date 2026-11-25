//! Deterministic source-root discovery for mixed-flavor projects.

use crate::{ProjectConfig, ProjectError};
use std::path::{Path, PathBuf};

/// The nearest manifest above a source file.
#[must_use]
pub fn project_root(file: &Path) -> Option<PathBuf> {
    file.parent()?
        .ancestors()
        .find(|directory| directory.join("osprey.toml").is_file())
        .map(Path::to_path_buf)
}

/// A Python file opts in with its source marker or the enclosing manifest.
#[must_use]
pub fn python_source(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .is_ok_and(|text| osprey_syntax::flavor_marker(&text) == Some("python"))
        || project_root(path).is_some_and(|root| {
            crate::discover(&root).is_ok_and(|(config, _)| config.contains_source(&root, path))
        })
}

/// Discover Osprey sources and explicitly selected Python- and TypeScript-flavor files.
pub(crate) fn discover(root: &Path, config: &ProjectConfig) -> Result<Vec<PathBuf>, ProjectError> {
    let mut paths = Vec::new();
    for source_root in &config.source_roots {
        DISCOVERY.visit(&root.join(source_root), &mut paths, &ProjectError::io)?;
    }
    paths.extend(opt_in_sources(root, config)?);
    validate_flavor(config, &paths)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// The rules a source-tree walk applies: which files it collects and which
/// entries it never enters.
pub(crate) struct Rules<'a> {
    /// Whether `path` is a file the walk collects.
    pub(crate) keep: &'a dyn Fn(&Path) -> bool,
    /// Whether `path` is an entry the walk climbs past.
    pub(crate) skip: &'a dyn Fn(&Path) -> bool,
}

impl Rules<'_> {
    /// Collect every kept file at or below `path`, in directory order.
    /// `unreadable` names a directory the walk cannot list, in the caller's
    /// own error type, so discovery and the discipline checker share one
    /// traversal while keeping their messages.
    pub(crate) fn visit<E>(
        &self,
        path: &Path,
        out: &mut Vec<PathBuf>,
        unreadable: &dyn Fn(&Path, &std::io::Error) -> E,
    ) -> Result<(), E> {
        if path.is_file() {
            if (self.keep)(path) {
                out.push(path.to_path_buf());
            }
            return Ok(());
        }
        let entries = std::fs::read_dir(path).map_err(|error| unreadable(path, &error))?;
        for entry in entries {
            let found = entry.map_err(|error| unreadable(path, &error))?.path();
            if !(self.skip)(&found) {
                self.visit(&found, out, unreadable)?;
            }
        }
        Ok(())
    }
}

/// A tree of the flavor's sources only, skipping hidden entries and `target`.
const DISCOVERY: Rules<'_> = Rules {
    keep: &is_source,
    skip: &hidden,
};

/// An Osprey source, or a `.lean` math source ([MATH-CODE-SOURCES]).
fn is_source(path: &Path) -> bool {
    crate::math::is_math(path) || is_native_source(path)
}

pub(crate) fn is_native_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("osp" | "ospml")
    )
}

pub(crate) fn contains(root: &Path, config: &ProjectConfig, path: &Path) -> bool {
    let path = normalize(path);
    (is_source(&path) || opt_in_selected(root, config, &path)) && inside_roots(root, config, &path)
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

/// A flavor whose extension is too common to sweep up: only the files a
/// manifest lists are its sources. Implements [FLAVOR-PYTHON-SELECT],
/// [FLAVOR-TS-SELECT].
struct OptIn {
    label: &'static str,
    extension: &'static str,
    flavor: osprey_syntax::Flavor,
    listed: fn(&ProjectConfig) -> &[PathBuf],
}

const OPT_IN: [OptIn; 2] = [
    OptIn {
        label: "Python",
        extension: "py",
        flavor: osprey_syntax::Flavor::Python,
        listed: |config| &config.python_sources,
    },
    OptIn {
        label: "TypeScript",
        extension: "ts",
        flavor: osprey_syntax::Flavor::TypeScript,
        listed: |config| &config.typescript_sources,
    },
];

fn opt_in_selected(root: &Path, config: &ProjectConfig, path: &Path) -> bool {
    OPT_IN.iter().any(|flavor| {
        flavor.is(path)
            && flavor
                .selections(config)
                .any(|selected| normalize(&root.join(selected)) == path)
    })
}

fn validate_flavor(config: &ProjectConfig, paths: &[PathBuf]) -> Result<(), ProjectError> {
    for flavor in OPT_IN
        .iter()
        .filter(|flavor| config.flavor == Some(flavor.flavor))
    {
        if let Some(path) = paths.iter().find(|path| is_source(path)) {
            return Err(selection_error(
                path,
                &format!(
                    "{} project flavor conflicts with mixed-flavor source; select flavors per file",
                    flavor.label
                ),
            ));
        }
    }
    Ok(())
}

fn opt_in_sources(root: &Path, config: &ProjectConfig) -> Result<Vec<PathBuf>, ProjectError> {
    let mut paths = std::collections::BTreeSet::new();
    for flavor in &OPT_IN {
        for source in (flavor.listed)(config) {
            let path = flavor.validate(root, config, source)?;
            if !paths.insert(path.clone()) {
                let message = format!("duplicate {} source", flavor.label);
                return Err(selection_error(&path, &message));
            }
        }
        if let Some(entry) = config.entry.as_deref().filter(|path| flavor.is(path)) {
            let _ = paths.insert(flavor.validate(root, config, entry)?);
        }
    }
    Ok(paths.into_iter().collect())
}

impl OptIn {
    /// Whether `path` has this flavor's extension; a `.d.ts` declaration file
    /// is never a source.
    fn is(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension == self.extension)
            && !path.to_string_lossy().ends_with(".d.ts")
    }

    /// The manifest's listed files plus an entry of this flavor.
    fn selections<'a>(&'a self, config: &'a ProjectConfig) -> impl Iterator<Item = &'a Path> {
        (self.listed)(config)
            .iter()
            .map(PathBuf::as_path)
            .chain(config.entry.as_deref().filter(|entry| self.is(entry)))
    }

    fn validate(
        &self,
        root: &Path,
        config: &ProjectConfig,
        source: &Path,
    ) -> Result<PathBuf, ProjectError> {
        let path = self.selected_path(root, config, source)?;
        let canonical = path
            .canonicalize()
            .map_err(|error| ProjectError::io(&path, &error))?;
        if !path.is_file() || !canonical_in_roots(root, config, &canonical) {
            let message = format!(
                "{} source resolves outside its source root or is not a file",
                self.label
            );
            return Err(selection_error(&path, &message));
        }
        Ok(canonical)
    }

    fn selected_path(
        &self,
        root: &Path,
        config: &ProjectConfig,
        source: &Path,
    ) -> Result<PathBuf, ProjectError> {
        let path = normalize(&root.join(source));
        if source.is_absolute() || !self.is(&path) || !inside_roots(root, config, &path) {
            let message = format!(
                "{} source must be a manifest-relative .{} file inside a declared source root",
                self.label, self.extension
            );
            return Err(selection_error(&path, &message));
        }
        if config.flavor.is_some_and(|flavor| flavor != self.flavor) {
            let message = format!(
                "project flavor conflicts with selected {} source",
                self.label
            );
            return Err(selection_error(&path, &message));
        }
        Ok(path)
    }
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

    /// [FLAVOR-TS-SELECT]: `.ts` is a common extension, so only listed files
    /// and an explicit `.ts` entry are sources; a declaration file never is.
    #[test]
    fn typescript_sources_are_opt_in_and_never_declaration_files() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!("osprey_ts_sources_{}", std::process::id()));
        std::fs::create_dir_all(root.join("src"))?;
        for path in [
            "src/main.ts",
            "src/routes.ts",
            "src/webpack.config.ts",
            "src/lib.d.ts",
            "src/model.py",
            "src/normal.osp",
            "outside.ts",
        ] {
            std::fs::write(root.join(path), "deliberately invalid source")?;
        }
        let mut config = ProjectConfig::for_root(&root);
        config.typescript_sources = vec!["src/routes.ts".into()];
        config.python_sources = vec!["src/model.py".into()];
        config.entry = Some("src/main.ts".into());
        let found = discover(&root, &config).map(|paths| {
            let mut names: Vec<_> = paths
                .iter()
                .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
                .collect();
            names.sort();
            names
        });
        assert_eq!(
            found.ok(),
            Some(vec![
                "main.ts".to_owned(),
                "model.py".to_owned(),
                "normal.osp".to_owned(),
                "routes.ts".to_owned()
            ])
        );
        for (relative, selected) in [
            ("src/routes.ts", true),
            ("src/main.ts", true),
            ("src/webpack.config.ts", false),
            ("src/lib.d.ts", false),
            ("src/../routes.ts", false),
        ] {
            let path = root.join(relative);
            assert_eq!(config.contains_source(&root, &path), selected, "{relative}");
        }
        for rejected in [
            vec!["src/missing.ts"],
            vec!["src/lib.d.ts"],
            vec!["src/normal.osp"],
            vec!["outside.ts"],
            vec!["src/routes.ts", "src/./routes.ts"],
        ] {
            config.typescript_sources = rejected.into_iter().map(PathBuf::from).collect();
            let listed = &config.typescript_sources;
            assert!(discover(&root, &config).is_err(), "accepted {listed:?}");
        }
        config.typescript_sources = vec!["src/routes.ts".into()];
        config.flavor = Some(osprey_syntax::Flavor::Python);
        assert!(
            discover(&root, &config).is_err(),
            "a Python override would parse .ts as Python"
        );
        config.flavor = Some(osprey_syntax::Flavor::TypeScript);
        assert!(
            discover(&root, &config).is_err(),
            "a TypeScript override would parse .osp as TypeScript"
        );
        let manifest =
            "[project]\nsource_roots = [\"src\"]\ntypescript_sources = [\"src/routes.ts\"]\n";
        let parsed = ProjectConfig::parse(manifest, &root.join("osprey.toml"));
        assert_eq!(
            parsed.ok().map(|config| config.typescript_sources),
            Some(vec![PathBuf::from("src/routes.ts")])
        );
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
