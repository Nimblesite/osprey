//! Incremental syntax snapshots shared by diagnostics and editor features.
//! Implements [LSP-WORKSPACE] and [LSP-PROJECT-BATCH].

use crate::workspace::{file_path, project_root, same_path, uri_of, Sibling};
use lspkit_vfs::{DocumentUri, Vfs};
use osprey_project::{ProjectConfig, ProjectError, SourceFile};
use osprey_syntax::Flavor;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Parsed {
    text: String,
    configured: Option<Flavor>,
    result: Result<SourceFile, Vec<ProjectError>>,
}

#[derive(Debug)]
pub(crate) struct Sources {
    pub(crate) root: PathBuf,
    pub(crate) config: ProjectConfig,
    pub(crate) files: Vec<SourceFile>,
    pub(crate) errors: Vec<ProjectError>,
    pub(crate) incomplete: bool,
}

impl Sources {
    fn new(root: &Path, config: ProjectConfig) -> Self {
        Self {
            root: root.to_path_buf(),
            config,
            files: Vec::new(),
            errors: Vec::new(),
            incomplete: false,
        }
    }

    pub(crate) fn view(&self, uri: &str) -> crate::workspace::View {
        let Some(path) =
            file_path(uri).filter(|path| self.config.contains_source(&self.root, path))
        else {
            return crate::workspace::View::default();
        };
        let current = self
            .files
            .iter()
            .find(|file| same_path(&path, &file.path))
            .cloned();
        let siblings = self
            .files
            .iter()
            .filter(|other| !same_path(&path, &other.path))
            .filter_map(sibling)
            .collect();
        crate::workspace::View {
            siblings,
            current,
            configured: self.config.flavor,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SourceCache {
    disk: BTreeMap<PathBuf, Parsed>,
    live: BTreeMap<PathBuf, Parsed>,
    projects: BTreeMap<PathBuf, Membership>,
    opened: BTreeSet<PathBuf>,
    #[cfg(test)]
    pub(crate) parses: usize,
}

#[derive(Debug)]
struct Membership {
    config: ProjectConfig,
    paths: BTreeSet<PathBuf>,
}

impl SourceCache {
    pub(crate) fn opened(&mut self, uri: &str, open: bool) {
        if let Some(path) = file_path(uri) {
            if open {
                let _ = self.opened.insert(path);
            } else {
                let _ = self.opened.remove(&path);
                let _ = self.live.remove(&path);
            }
        }
    }

    pub(crate) fn documents(&self) -> Vec<DocumentUri> {
        self.opened
            .iter()
            .filter_map(|path| uri_of(path).map(DocumentUri::new))
            .collect()
    }

    pub(crate) fn affected(&self, document: &str, changed: &str) -> bool {
        let root = file_path(document).and_then(|path| project_root(&path));
        let Some((root, path)) = root.zip(file_path(changed)) else {
            return false;
        };
        project_root(&path).as_ref() == Some(&root)
            || self
                .projects
                .get(&root)
                .is_some_and(|members| members.config.contains_source(&root, &path))
    }

    pub(crate) fn load(
        &mut self,
        uri: &str,
        vfs: Option<&Vfs>,
        text: Option<&str>,
    ) -> Option<Result<Sources, Vec<ProjectError>>> {
        let current = file_path(uri)?;
        let root = project_root(&current)?;
        let inputs = match osprey_project::discover(&root) {
            Ok(inputs) => inputs,
            Err(errors) => return Some(Err(errors)),
        };
        if !inputs.0.contains_source(&root, &current) {
            return None;
        }
        Some(Ok(self.project(&root, &current, vfs, text, inputs)))
    }

    fn project(
        &mut self,
        root: &Path,
        current: &Path,
        vfs: Option<&Vfs>,
        text: Option<&str>,
        (config, paths): (ProjectConfig, Vec<PathBuf>),
    ) -> Sources {
        let paths = self.members(root, current, &config, paths);
        let mut sources = Sources::new(root, config);
        self.retain(root, &sources.config, &paths);
        for path in paths {
            let live = live_text(&path, current, text, vfs);
            self.add(&mut sources, path, live);
        }
        sources
    }

    fn members(
        &self,
        root: &Path,
        current: &Path,
        config: &ProjectConfig,
        mut paths: Vec<PathBuf>,
    ) -> Vec<PathBuf> {
        paths.extend(
            self.opened
                .iter()
                .map(PathBuf::as_path)
                .chain(std::iter::once(current))
                .filter(|path| config.contains_source(root, path))
                .map(Path::to_path_buf),
        );
        paths.sort();
        paths.dedup();
        paths
    }

    fn add(&mut self, sources: &mut Sources, path: PathBuf, live: Option<String>) {
        let (selected, incomplete) = self.select(path, live, sources.config.flavor);
        sources.incomplete |= incomplete;
        match selected {
            Ok(file) => sources.files.push(file),
            Err(errors) => sources.errors.extend(errors),
        }
    }

    fn select(
        &mut self,
        path: PathBuf,
        live: Option<String>,
        flavor: Option<Flavor>,
    ) -> (Result<SourceFile, Vec<ProjectError>>, bool) {
        let Some(live) = live else {
            return (self.disk_file(path, flavor), false);
        };
        match self.parse(path.clone(), live, flavor, true) {
            Ok(file) => (Ok(file), false),
            Err(errors) => (self.disk_file(path, flavor).or(Err(errors)), true),
        }
    }

    fn disk_file(
        &mut self,
        path: PathBuf,
        flavor: Option<Flavor>,
    ) -> Result<SourceFile, Vec<ProjectError>> {
        read(&path).and_then(|text| self.parse(path, text, flavor, false))
    }

    fn parse(
        &mut self,
        path: PathBuf,
        text: String,
        configured: Option<Flavor>,
        live: bool,
    ) -> Result<SourceFile, Vec<ProjectError>> {
        let cache = if live { &mut self.live } else { &mut self.disk };
        if let Some(parsed) = cache
            .get(&path)
            .filter(|p| p.text == text && p.configured == configured)
        {
            return parsed.result.clone();
        }
        let parsed = Parsed::new(path.clone(), text, configured);
        #[cfg(test)]
        {
            self.parses += 1;
        }
        let result = parsed.result.clone();
        let _ = cache.insert(path, parsed);
        result
    }

    fn retain(&mut self, root: &Path, config: &ProjectConfig, paths: &[PathBuf]) {
        let members = Membership {
            config: config.clone(),
            paths: paths.iter().cloned().collect(),
        };
        let _ = self.projects.insert(root.to_path_buf(), members);
        let keep = |path: &PathBuf, _: &mut Parsed| {
            self.projects
                .values()
                .any(|members| members.paths.contains(path))
        };
        self.disk.retain(keep);
        self.live.retain(keep);
    }
}

fn read(path: &Path) -> Result<String, Vec<ProjectError>> {
    std::fs::read_to_string(path).map_err(|error| {
        vec![ProjectError {
            message: error.to_string(),
            path: Some(path.to_path_buf()),
            line: None,
            column: None,
        }]
    })
}

impl Parsed {
    fn new(path: PathBuf, text: String, configured: Option<Flavor>) -> Self {
        let result = osprey_project::parse_text(path, text.clone(), configured);
        Self {
            text,
            configured,
            result,
        }
    }
}

fn live_text(path: &Path, current: &Path, text: Option<&str>, vfs: Option<&Vfs>) -> Option<String> {
    if same_path(path, current) && text.is_some() {
        return text.map(str::to_owned);
    }
    vfs?.text(&DocumentUri::new(uri_of(path)?))
}

fn sibling(file: &SourceFile) -> Option<Sibling> {
    Some(Sibling {
        uri: uri_of(&file.path)?,
        source: file.source.clone(),
        program: file.program.clone(),
        flavor: file.flavor,
    })
}
