//! Workspace diagnostic discovery and live scope changes [LSP-DIAGNOSTICS-SCOPE].

use super::{publish, OspreyEngine};
use crate::workspace::{file_path, uri_of};
use lspkit::vfs::DocumentUri;
use lspkit::EngineApi;
use lspkit_server::diagnostics::DiagnosticsBatch;
use lspkit_server::DiagnosticsBus;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub(crate) struct Scope {
    folders: Vec<PathBuf>,
    diagnostics: Value,
    open: BTreeSet<String>,
    published: BTreeSet<String>,
}

pub(crate) fn lock(engine: &OspreyEngine) -> MutexGuard<'_, Scope> {
    engine.scope.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Scope {
    pub(super) fn initialize(&mut self, params: &Value) {
        self.folders = params["workspaceFolders"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|folder| folder["uri"].as_str().and_then(file_path))
            .collect();
        if self.folders.is_empty() {
            self.folders
                .extend(params["rootUri"].as_str().and_then(file_path));
        }
        self.configure(&params["initializationOptions"]);
    }

    pub(super) fn configure(&mut self, settings: &Value) {
        self.diagnostics = settings
            .get("osprey")
            .unwrap_or(settings)
            .get("diagnostics")
            .cloned()
            .unwrap_or(Value::Null);
    }

    pub(super) fn folders_changed(&mut self, event: &Value) {
        for removed in event["removed"].as_array().into_iter().flatten() {
            if let Some(path) = removed["uri"].as_str().and_then(file_path) {
                self.folders.retain(|folder| *folder != path);
            }
        }
        self.folders.extend(
            event["added"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|folder| folder["uri"].as_str().and_then(file_path)),
        );
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
            Some("roots") => self
                .diagnostics
                .get("roots")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
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
            discover(&root, &mut documents);
        }
        documents
    }

    pub(super) fn remember(&mut self, uri: &str) {
        let _ = self.published.insert(uri.into());
    }

    pub(super) fn tracked(&self) -> Vec<DocumentUri> {
        self.published.iter().map(DocumentUri::new).collect()
    }

    pub(super) fn enabled(&self) -> bool {
        self.diagnostics.get("enabled").and_then(Value::as_bool) != Some(false)
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

fn discover(root: &Path, documents: &mut BTreeSet<String>) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("osprey lsp: cannot scan {}: {error}", root.display());
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => visit(&entry, documents),
            Err(error) => eprintln!("osprey lsp: cannot read {}: {error}", root.display()),
        }
    }
}

fn visit(entry: &std::fs::DirEntry, documents: &mut BTreeSet<String>) {
    let path = entry.path();
    let kind = match entry.file_type() {
        Ok(kind) => kind,
        Err(error) => {
            eprintln!("osprey lsp: cannot inspect {}: {error}", path.display());
            return;
        }
    };
    if kind.is_dir() && !excluded(&path) {
        discover(&path, documents);
    }
    if kind.is_file() && source_file(&path) {
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

pub(super) async fn refresh(engine: &OspreyEngine, bus: &DiagnosticsBus) {
    let (documents, removed) = {
        let mut scope = lock(engine);
        let documents = scope.documents();
        let removed = scope
            .published
            .difference(&documents)
            .cloned()
            .collect::<Vec<_>>();
        scope.published.clone_from(&documents);
        (documents, removed)
    };
    for uri in removed {
        bus.publish(DiagnosticsBatch::new(
            DocumentUri::new(uri),
            engine.generation(),
            Vec::new(),
        ))
        .await;
    }
    for uri in documents {
        publish(engine, bus, DocumentUri::new(uri)).await;
    }
}
