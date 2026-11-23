//! Incremental syntax snapshots shared by diagnostics and editor features.
//! Implements [LSP-WORKSPACE] and [LSP-PROJECT-BATCH].

use crate::workspace::{
    file_path, lexical, project_root, project_roots, same_path, uri_of, Sibling,
};
use lspkit::vfs::{DocumentUri, Vfs};
use osprey_project::{ProjectConfig, ProjectError, SourceFile};
use osprey_syntax::Flavor;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
    /// The files of the other projects that include the document: siblings
    /// for editor features, never assembled with this project's own.
    neighbours: Vec<SourceFile>,
    pub(crate) errors: Vec<ProjectError>,
    pub(crate) incomplete: bool,
}

impl Sources {
    /// Use declarations parsed before a missing field or pattern in the live buffer.
    pub(super) fn recover_current(&mut self, uri: &str, text: &str) {
        let Some(path) =
            file_path(uri).filter(|path| self.config.contains_source(&self.root, path))
        else {
            return;
        };
        let current = self
            .files
            .iter_mut()
            .find(|file| same_path(&file.path, &path));
        if current.as_ref().is_some_and(|file| file.source == text) {
            return;
        }
        let Ok(flavor) = osprey_syntax::resolve_flavor(self.config.flavor, uri, text) else {
            return;
        };
        let program = osprey_syntax::parse_program_with_flavor(text, flavor).program;
        let recovered = SourceFile {
            path,
            source: text.to_owned(),
            program,
            flavor,
            discipline: None,
        };
        if let Some(file) = current {
            *file = recovered;
        } else {
            self.files.push(recovered);
        }
    }

    fn new(root: &Path, config: ProjectConfig) -> Self {
        Self {
            root: root.to_path_buf(),
            config,
            files: Vec::new(),
            neighbours: Vec::new(),
            errors: Vec::new(),
            incomplete: false,
        }
    }

    /// Take another project's files as neighbours, skipping files already
    /// present. Assembling them with this project's would declare a shared
    /// file twice and give the program two entries.
    fn adopt(&mut self, other: Sources) {
        for file in other.files {
            if !self
                .everyone()
                .any(|known| same_path(&known.path, &file.path))
            {
                self.neighbours.push(file);
            }
        }
        self.incomplete |= other.incomplete;
    }

    fn everyone(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.iter().chain(&self.neighbours)
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
            .everyone()
            .filter(|other| !same_path(&path, &other.path))
            .filter_map(sibling)
            .collect();
        crate::workspace::View {
            siblings,
            current,
            configured: self.config.flavor,
            checked: None,
        }
    }
}

/// Closed projects whose syntax and analysis outlive the next project loaded.
/// The diagnostics sweep visits every project of the repository in turn, so
/// keeping them all pinned an idle server's memory to the repository's size.
const RECENT_PROJECTS: usize = 4;

#[derive(Debug, Default)]
pub(crate) struct SourceCache {
    disk: BTreeMap<PathBuf, Parsed>,
    live: BTreeMap<PathBuf, Parsed>,
    /// Every loaded project's membership: cheap, and what sibling detection
    /// answers from ([`Self::affected`]).
    projects: BTreeMap<PathBuf, Membership>,
    /// The latest loaded project roots, newest first.
    recent: VecDeque<PathBuf>,
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

    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.disk.len() + self.live.len()
    }

    pub(crate) fn documents(&self) -> Vec<DocumentUri> {
        self.opened
            .iter()
            .filter_map(|path| uri_of(path).map(DocumentUri::new))
            .collect()
    }

    /// Whether a change to `changed` can alter `document`'s project: the two
    /// share an enclosing manifest, or a project the document was loaded into
    /// lists the changed file. Answered from the loaded memberships alone, so a
    /// watcher storm never walks the disk.
    pub(crate) fn affected(&self, document: &str, changed: &str) -> bool {
        let Some((document, changed)) = file_path(document).zip(file_path(changed)) else {
            return false;
        };
        let enclosing = (project_root(&document), project_root(&changed));
        if enclosing.0.is_some() && enclosing.0 == enclosing.1 {
            return true;
        }
        self.projects.iter().any(|(root, members)| {
            members.config.contains_source(root, &document)
                && (enclosing.1.as_ref() == Some(root)
                    || members.config.contains_source(root, &changed))
        })
    }

    /// The nearest project the document belongs to, with the files of every
    /// other project that includes it as neighbours, so a shared source root
    /// sees every program that includes it.
    pub(crate) fn load(
        &mut self,
        uri: &str,
        vfs: Option<&Vfs>,
        text: Option<&str>,
    ) -> Option<Result<Sources, Vec<ProjectError>>> {
        let current = file_path(uri)?;
        let roots = project_roots(&current);
        let (first, rest) = roots.split_first()?;
        let inputs = match osprey_project::discover(first) {
            Ok(inputs) => inputs,
            Err(errors) => return Some(Err(errors)),
        };
        let mut sources = self.project(first, &current, vfs, text, inputs);
        for root in rest {
            if let Ok(inputs) = osprey_project::discover(root) {
                let more = self.project(root, &current, vfs, text, inputs);
                sources.adopt(more);
            }
        }
        Some(Ok(sources))
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
        // `source_roots = ["../model"]` discovers `generate/../model/m.osp`
        // while the editor opened `model/m.osp`: one file, listed once.
        let mut paths: Vec<PathBuf> = paths.iter().map(|path| lexical(path)).collect();
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
        self.recent.retain(|recent| recent != root);
        self.recent.push_front(root.to_path_buf());
        self.recent.truncate(RECENT_PROJECTS);
        let kept = self.kept();
        let keep = |path: &PathBuf, _: &mut Parsed| {
            kept.iter()
                .filter_map(|root| self.projects.get(root))
                .any(|members| members.paths.contains(path))
        };
        self.disk.retain(keep);
        self.live.retain(keep);
    }

    /// The project roots whose memory stays: each open document's own
    /// project, and the most recently loaded ones. A shared file many
    /// projects list pins only its own, never every dependent.
    pub(crate) fn kept(&self) -> BTreeSet<PathBuf> {
        self.opened
            .iter()
            .filter_map(|path| self.owner(path))
            .chain(self.recent.iter().cloned())
            .collect()
    }

    /// The enclosing manifest's project when it lists `path`, else the first
    /// loaded project that does.
    fn owner(&self, path: &Path) -> Option<PathBuf> {
        let lists = |root: &PathBuf| {
            self.projects
                .get(root)
                .is_some_and(|members| members.paths.contains(path))
        };
        project_root(path).filter(lists).or_else(|| {
            self.projects
                .iter()
                .find(|(_, members)| members.paths.contains(path))
                .map(|(root, _)| root.clone())
        })
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
        uri: uri_of(&lexical(&file.path))?,
        source: file.source.clone(),
        program: file.program.clone(),
        flavor: file.flavor,
    })
}
