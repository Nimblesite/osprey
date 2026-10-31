//! The discipline registry of [TYPES-DISCIPLINE-REGISTRY] (spec 0050): the
//! `[discipline]` tables found among a project's dependencies. The compiler
//! holds no list of names; everything here is read from manifests on disk.

use super::ERROR_PREFIX;
use crate::manifest::Dependency;
use crate::{ProjectConfig, ProjectError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where the package client installs a dependency: `.osprey/packages/@org/name/<digest>/`.
const INSTALL_ROOT: &str = ".osprey/packages";

/// One registered discipline: a dependency whose manifest declares `[discipline]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discipline {
    /// The word after `types=`.
    pub name: String,
    /// The `@org/name` of the package that registers it.
    pub package: String,
    /// The package directory holding its manifest.
    pub root: PathBuf,
    /// Absolute path of the checker program: one source file, or a project
    /// directory with its own `osprey.toml` whose entry is the checker.
    pub checker: PathBuf,
    /// Per-module time limit for the checker process.
    pub timeout: Duration,
}

/// Every discipline the project's dependencies register, by name.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    by_name: BTreeMap<String, Discipline>,
}

impl Registry {
    /// Read the `[discipline]` table of every `[dependencies]` entry.
    ///
    /// # Errors
    /// Two dependencies registering one name is a `discipline-error` naming
    /// both packages; an unreadable dependency manifest is reported once.
    pub fn discover(config: &ProjectConfig) -> Result<Self, ProjectError> {
        let mut registry = Self::default();
        for dependency in &config.dependencies {
            let Some(root) = locate(&config.directory, dependency) else {
                continue;
            };
            if let Some(discipline) = read(&root, dependency)? {
                registry.insert(discipline)?;
            }
        }
        Ok(registry)
    }

    /// Build a registry from test fixtures.
    ///
    /// # Errors
    /// Duplicate names are rejected exactly as from disk.
    #[cfg(test)]
    pub(super) fn from_disciplines(disciplines: Vec<Discipline>) -> Result<Self, ProjectError> {
        let mut registry = Self::default();
        for discipline in disciplines {
            registry.insert(discipline)?;
        }
        Ok(registry)
    }

    /// The discipline registered under `name`, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Discipline> {
        self.by_name.get(name)
    }

    fn insert(&mut self, discipline: Discipline) -> Result<(), ProjectError> {
        if let Some(existing) = self.by_name.get(&discipline.name) {
            return Err(duplicate(existing, &discipline));
        }
        let _previous = self.by_name.insert(discipline.name.clone(), discipline);
        Ok(())
    }
}

/// A dependency's directory: its `path` when given, else the single installed
/// release under `.osprey/packages/<name>/`. A dependency with neither is not
/// a discipline and is skipped; the solver reports missing packages.
fn locate(project: &Path, dependency: &Dependency) -> Option<PathBuf> {
    if let Some(path) = &dependency.path {
        return Some(project.join(path));
    }
    let installed = project.join(INSTALL_ROOT).join(&dependency.name);
    let mut releases: Vec<PathBuf> = std::fs::read_dir(installed)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("osprey.toml").is_file())
        .collect();
    releases.sort();
    releases.pop()
}

fn read(root: &Path, dependency: &Dependency) -> Result<Option<Discipline>, ProjectError> {
    let manifest = root.join("osprey.toml");
    let text = match std::fs::read_to_string(&manifest) {
        Ok(text) => text,
        Err(error) => return Err(ProjectError::io(&manifest, &error)),
    };
    let config = ProjectConfig::parse(&text, &manifest).map_err(|errors| ProjectError {
        message: format!(
            "{ERROR_PREFIX}: manifest of `{}` is malformed: {}",
            dependency.name,
            errors.first().map_or("", |error| error.message.as_str())
        ),
        path: Some(manifest.clone()),
        line: None,
        column: None,
    })?;
    Ok(config.discipline.map(|decl| Discipline {
        name: decl.name,
        package: dependency.name.clone(),
        root: root.to_path_buf(),
        checker: root.join(decl.checker),
        timeout: Duration::from_secs(decl.timeout_seconds),
    }))
}

fn duplicate(first: &Discipline, second: &Discipline) -> ProjectError {
    ProjectError {
        message: format!(
            "{ERROR_PREFIX}: type discipline `{}` is registered by both `{}` and `{}`; \
             depend on one of them",
            first.name, first.package, second.package
        ),
        path: Some(second.root.join("osprey.toml")),
        line: None,
        column: None,
    }
}
