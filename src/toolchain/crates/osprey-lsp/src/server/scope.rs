//! Workspace diagnostic discovery and live scope changes [LSP-DIAGNOSTICS-SCOPE],
//! less the files `osprey.diagnostics.exclude` names [LSP-DIAGNOSTICS-EXCLUDE].

use super::{publish, OspreyEngine};
use crate::workspace::{file_path, uri_of};
use lspkit::vfs::DocumentUri;
use lspkit::EngineApi;
use lspkit_server::diagnostics::DiagnosticsBatch;
use lspkit_server::DiagnosticsBus;
use serde_json::Value;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub(crate) struct Scope {
    folders: Vec<PathBuf>,
    diagnostics: Value,
    /// The `osprey.analysis` settings ([LSP-ANALYSIS-SCOPE]).
    analysis: Value,
    open: BTreeSet<String>,
    published: BTreeSet<String>,
    /// Edited documents (checked with their siblings) and those siblings,
    /// checked before the sweep.
    urgent: VecDeque<(String, bool)>,
    /// Closed documents the sweep still has to check; requests go first.
    pending: VecDeque<String>,
    in_flight: usize,
    progress: bool,
}

pub(crate) fn lock(engine: &OspreyEngine) -> MutexGuard<'_, Scope> {
    engine.scope.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The folder paths the entries of `folders` name.
fn folder_paths(folders: &Value) -> impl Iterator<Item = PathBuf> + '_ {
    folders
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|folder| folder["uri"].as_str().and_then(file_path))
}

/// The strings `value` holds when it holds an array.
fn strings(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .into_iter()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
}

impl Scope {
    pub(super) fn initialize(&mut self, params: &Value) {
        self.folders = folder_paths(&params["workspaceFolders"]).collect();
        if self.folders.is_empty() {
            self.folders
                .extend(params["rootUri"].as_str().and_then(file_path));
        }
        self.progress = params
            .pointer("/capabilities/window/workDoneProgress")
            .and_then(Value::as_bool)
            == Some(true);
        self.configure(&params["initializationOptions"]);
    }

    /// Whether the client shows work-done progress for the sweep.
    pub(super) fn reports_progress(&self) -> bool {
        self.progress
    }

    /// How many queued documents are left to check, counting the one being
    /// checked.
    pub(super) fn sweep_left(&self) -> usize {
        self.urgent.len() + self.pending.len() + self.in_flight
    }

    /// Check `uri` off the request path, ahead of the repository sweep, then
    /// its siblings when `siblings` is set.
    pub(super) fn queue_now(&mut self, uri: &str, siblings: bool) {
        if self.covers(uri) && !self.urgent.iter().any(|(queued, _)| queued == uri) {
            self.remember(uri);
            self.urgent.push_back((uri.into(), siblings));
        }
    }

    /// The next document to check, edited ones before the repository sweep,
    /// and whether its siblings follow it.
    pub(super) fn next_swept(&mut self) -> Option<(DocumentUri, bool)> {
        let (next, siblings) = self
            .urgent
            .pop_front()
            .or_else(|| self.pending.pop_front().map(|uri| (uri, false)))?;
        self.in_flight += 1;
        Some((DocumentUri::new(next), siblings))
    }

    /// Finished checking `uri`: whether its diagnostics are still in scope,
    /// since a refresh may have dropped it while it was being checked.
    pub(super) fn swept(&mut self, uri: &str) -> bool {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.covers(uri) && self.published.contains(uri)
    }

    pub(super) fn configure(&mut self, settings: &Value) {
        let osprey = settings.get("osprey").unwrap_or(settings);
        let section = |name: &str| osprey.get(name).cloned().unwrap_or(Value::Null);
        self.diagnostics = section("diagnostics");
        self.analysis = section("analysis");
    }

    /// Whether displayed types are inferred from the whole project and its
    /// packages (the default) or from each file alone
    /// (`osprey.analysis.scope = "file"`). Implements [LSP-ANALYSIS-SCOPE].
    pub(crate) fn whole_project(&self) -> bool {
        self.analysis.get("scope").and_then(Value::as_str) != Some("file")
    }

    pub(super) fn folders_changed(&mut self, event: &Value) {
        for removed in event["removed"].as_array().into_iter().flatten() {
            if let Some(path) = removed["uri"].as_str().and_then(file_path) {
                self.folders.retain(|folder| *folder != path);
            }
        }
        self.folders.extend(folder_paths(&event["added"]));
        self.folders.sort();
        self.folders.dedup();
    }

    pub(super) fn opened(&mut self, uri: &str, opened: bool) {
        if opened {
            let _ = self.open.insert(uri.into());
        } else {
            let _ = self.open.remove(uri);
        }
    }

    fn roots(&self) -> Vec<PathBuf> {
        match self.diagnostics.get("scope").and_then(Value::as_str) {
            Some("openFiles") => Vec::new(),
            Some("roots") => strings(self.diagnostics.get("roots"))
                .flat_map(|root| resolve_root(root, &self.folders))
                .collect(),
            _ => self
                .folders
                .iter()
                .map(|folder| repository(folder))
                .collect(),
        }
    }

    fn documents(&self) -> BTreeSet<String> {
        if !self.enabled() {
            return BTreeSet::new();
        }
        let mut documents = self.open.clone();
        for root in self.roots() {
            let unignored = unignored(&root);
            discover(&root, unignored.as_ref(), &mut documents);
        }
        documents.retain(|uri| !self.excludes(uri));
        documents
    }

    pub(super) fn remember(&mut self, uri: &str) {
        let _ = self.published.insert(uri.into());
    }

    pub(super) fn tracked(&self) -> Vec<DocumentUri> {
        self.published.iter().map(DocumentUri::new).collect()
    }

    fn enabled(&self) -> bool {
        self.diagnostics.get("enabled").and_then(Value::as_bool) != Some(false)
    }

    /// Whether `uri` gets diagnostics: they are enabled and no exclude glob
    /// matches it. The CLI never reads this setting: `osprey <file> --check`
    /// still reports every error.
    pub(super) fn covers(&self, uri: &str) -> bool {
        self.enabled() && !self.excludes(uri)
    }

    /// Exclude globs match the absolute path or the path under a workspace
    /// folder; `**` spans folders, `*` and `?` stay within one.
    fn excludes(&self, uri: &str) -> bool {
        let Some(path) = file_path(uri) else {
            return false;
        };
        let candidates = std::iter::once(path.as_path())
            .chain(
                self.folders
                    .iter()
                    .filter_map(|folder| path.strip_prefix(folder).ok()),
            )
            .map(segments)
            .collect::<Vec<_>>();
        self.exclude_globs().iter().any(|glob| {
            let glob = glob
                .split('/')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            candidates.iter().any(|path| glob_match(&glob, path))
        })
    }

    fn exclude_globs(&self) -> Vec<&str> {
        strings(self.diagnostics.get("exclude")).collect()
    }
}

fn segments(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|part| match part {
            std::path::Component::Normal(name) => name.to_str().map(String::from),
            _ => None,
        })
        .collect()
}

fn glob_match(glob: &[&str], path: &[String]) -> bool {
    match glob.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len())
            .filter_map(|skip| path.get(skip..))
            .any(|tail| glob_match(rest, tail)),
        Some((part, rest)) => path.split_first().is_some_and(|(name, tail)| {
            wildcard(part.as_bytes(), name.as_bytes()) && glob_match(rest, tail)
        }),
    }
}

fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.split_first(), name.split_first()) {
        (None, _) => name.is_empty(),
        (Some((b'*', rest)), _) => (0..=name.len())
            .filter_map(|skip| name.get(skip..))
            .any(|tail| wildcard(rest, tail)),
        (Some((b'?', rest)), Some((_, tail))) => wildcard(rest, tail),
        (Some((expected, rest)), Some((actual, tail))) => {
            expected == actual && wildcard(rest, tail)
        }
        (Some(_), None) => false,
    }
}

fn repository(folder: &Path) -> PathBuf {
    folder
        .ancestors()
        .find(|path| path.join(".git").exists())
        .unwrap_or(folder)
        .to_path_buf()
}

fn resolve_root(root: &str, folders: &[PathBuf]) -> Vec<PathBuf> {
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
fn unignored(root: &Path) -> Option<HashSet<PathBuf>> {
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

fn discover(root: &Path, unignored: Option<&HashSet<PathBuf>>, documents: &mut BTreeSet<String>) {
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
        Some("py") => python_file(path),
        _ => false,
    }
}

fn python_file(path: &Path) -> bool {
    // CPython tools are not Osprey. A marker or manifest must opt in.
    if std::fs::read_to_string(path)
        .is_ok_and(|text| osprey_syntax::flavor_marker(&text) == Some("python"))
    {
        return true;
    }
    crate::workspace::project_root(path).is_some_and(|root| {
        osprey_project::discover(&root)
            .is_ok_and(|(_, files)| files.iter().any(|file| file == path))
    })
}

/// Clear documents that left the scope, re-check the open ones now, and queue
/// every closed one for the sweep the work queue runs between requests.
pub(super) async fn refresh(engine: &OspreyEngine, bus: &DiagnosticsBus) {
    let (open, removed) = {
        let mut scope = lock(engine);
        let documents = scope.documents();
        let removed = scope
            .published
            .difference(&documents)
            .cloned()
            .collect::<Vec<_>>();
        let (open, closed) = documents
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|uri| scope.open.contains(uri));
        scope.pending = closed.into();
        scope.published = documents;
        (open, removed)
    };
    for uri in removed {
        bus.publish(DiagnosticsBatch::new(
            DocumentUri::new(uri),
            engine.generation(),
            Vec::new(),
        ))
        .await;
    }
    for uri in open {
        publish(engine, bus, DocumentUri::new(uri)).await;
    }
}

/// Queue `doc`, then every document its edit can affect, for diagnostics the
/// work queue publishes beside requests [LSP-DIAGNOSTICS].
pub(super) fn queue_edit(engine: &OspreyEngine, doc: &DocumentUri) {
    lock(engine).queue_now(doc.as_str(), true);
}

pub(super) fn queue_siblings(engine: &OspreyEngine, changed: &DocumentUri) {
    for doc in siblings(engine, changed) {
        lock(engine).queue_now(doc.as_str(), false);
    }
}

/// A closed buffer's siblings fall back to the saved file: republish them
/// before the scope refresh clears what left it.
pub(super) async fn publish_siblings(
    engine: &OspreyEngine,
    bus: &DiagnosticsBus,
    changed: &DocumentUri,
) {
    for doc in siblings(engine, changed) {
        publish(engine, bus, doc).await;
    }
}

/// The diagnostic documents an edit to `changed` can affect.
fn siblings(engine: &OspreyEngine, changed: &DocumentUri) -> Vec<DocumentUri> {
    let mut docs = lock(engine).tracked();
    docs.extend(engine.project_cache.documents());
    docs.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    docs.dedup();
    docs.retain(|doc| {
        doc != changed
            && engine
                .project_cache
                .affected(doc.as_str(), changed.as_str())
    });
    docs
}
