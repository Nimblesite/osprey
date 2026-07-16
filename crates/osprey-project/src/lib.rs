//! Osprey project loading and module resolution.
//!
//! The project layer is the only phase allowed to know physical source paths.
//! It parses mixed `.osp`/`.ospml` inputs, assembles logical namespace
//! contributions, resolves module imports, and emits one flavor-neutral
//! canonical program for checking and code generation. Implements
//! [MODULES-MODEL], [MODULES-PATH-INDEPENDENCE], and [MODULES-PROJECT].

mod annotation;
mod assemble;
mod collect;
mod collect_support;
mod contract;
mod contribution;
mod imports;
mod manifest;
mod model;
mod name_resolve;
mod opaque;
mod purity;
mod resolve;
mod rewrite;
mod signature_collect;
mod source;
mod span;
mod state;
mod state_support;
mod symbol_rewrite;
mod type_rewrite;

pub use manifest::ProjectConfig;
use osprey_syntax::Flavor;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One source file selected for project assembly.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Physical path, used only for reads and diagnostics.
    pub path: PathBuf,
    /// Flavor selected before parsing.
    pub flavor: Flavor,
    /// Original source text, retained for linking directives and diagnostics.
    pub source: String,
    /// Canonical parsed AST.
    pub program: osprey_ast::Program,
}

/// A project-level failure tied to a source or manifest location when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectError {
    /// Human-readable diagnostic.
    pub message: String,
    /// Physical file involved, when applicable.
    pub path: Option<PathBuf>,
    /// One-based source line, when applicable.
    pub line: Option<usize>,
    /// Zero-based source column, when applicable.
    pub column: Option<usize>,
}

impl ProjectError {
    fn manifest(line: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
            line: Some(line),
            column: None,
        }
    }

    fn io(path: &Path, error: &std::io::Error) -> Self {
        Self {
            message: error.to_string(),
            path: Some(path.to_path_buf()),
            line: None,
            column: None,
        }
    }

    fn source(
        source: &SourceMetadata,
        position: Option<osprey_ast::Position>,
        message: impl Into<String>,
    ) -> Self {
        let local = position.and_then(|position| source.local_line(position.line));
        Self {
            message: message.into(),
            path: Some(source.path.clone()),
            line: local.and_then(|line| usize::try_from(line).ok()),
            column: position.and_then(|position| usize::try_from(position.column).ok()),
        }
    }
}

/// Source identity and global-line mapping retained after project flattening.
#[derive(Debug, Clone)]
pub struct SourceMetadata {
    /// Source index used by project diagnostics and [`AssembledProject::entry_source`].
    pub index: usize,
    /// Physical source path.
    pub path: PathBuf,
    /// Flavor selected for this file.
    pub flavor: Flavor,
    /// Original source text, retained for FFI link directives and tooling.
    pub source: String,
    /// First one-based line assigned to this source in the flattened position space.
    pub global_line_start: u32,
    /// Last one-based line assigned to this source in the flattened position space.
    pub global_line_end: u32,
}

impl SourceMetadata {
    /// Map a flattened global line back to this source's one-based local line.
    #[must_use]
    pub fn local_line(&self, global: u32) -> Option<u32> {
        (self.global_line_start..=self.global_line_end)
            .contains(&global)
            .then(|| {
                global
                    .saturating_sub(self.global_line_start)
                    .saturating_add(1)
            })
    }
}

/// One fully assembled project ready for the shared checker and backend.
#[derive(Debug, Clone)]
pub struct AssembledProject {
    /// Flavor-neutral flat program with imports/modules removed and names resolved.
    pub program: osprey_ast::Program,
    /// Ordered executable statements contributed by the entry source.
    pub entry_prologue: Vec<osprey_ast::Stmt>,
    /// Index into [`Self::sources`] for the configured or inferred entry source.
    pub entry_source: usize,
    /// Source metadata and disjoint position ranges in deterministic path order.
    pub sources: Vec<SourceMetadata>,
    /// Source-level qualified name for each deterministic internal linkage name.
    pub source_name_by_mangled: BTreeMap<String, String>,
}

impl AssembledProject {
    /// Metadata for the selected entry source, or `None` for a malformed value.
    #[must_use]
    pub fn entry(&self) -> Option<&SourceMetadata> {
        self.sources.get(self.entry_source)
    }

    /// Find the physical source and local line for a flattened position.
    #[must_use]
    pub fn source_at_line(&self, global: u32) -> Option<(&SourceMetadata, u32)> {
        self.sources
            .iter()
            .find_map(|source| source.local_line(global).map(|line| (source, line)))
    }
}

/// Read a directory project and parse every configured source file.
///
/// # Errors
///
/// Returns discovery, I/O, flavor-selection, and syntax diagnostics.
pub fn load(root: &Path) -> Result<(ProjectConfig, Vec<SourceFile>), Vec<ProjectError>> {
    let manifest_path = root.join("osprey.toml");
    let config = load_config(root, &manifest_path)?;
    let paths = source::discover(root, &config).map_err(|error| vec![error])?;
    parse_sources(paths, config.flavor).map(|sources| (config, sources))
}

/// Assemble already-loaded sources into one resolved canonical program.
///
/// # Errors
///
/// Returns all project graph, import, signature, entry, and state diagnostics.
pub fn assemble(
    config: &ProjectConfig,
    sources: &[SourceFile],
) -> Result<AssembledProject, Vec<ProjectError>> {
    assemble::assemble(config, sources)
}

/// Module-bearing single files need resolver/flattening; ordinary scripts must
/// bypass it so their existing IR and debugger symbol names remain exact.
#[must_use]
pub fn needs_assembly(program: &osprey_ast::Program) -> bool {
    use osprey_ast::Stmt;
    program.statements.iter().any(|statement| {
        matches!(
            statement,
            Stmt::Namespace { .. } | Stmt::Module { .. } | Stmt::Import(_) | Stmt::Signature { .. }
        )
    })
}

/// Assemble one module-aware source in isolation, without sweeping up sibling
/// files — the single-file path shared by the CLI and the language server.
///
/// # Errors
///
/// Returns all project graph, import, signature, entry, and state diagnostics.
pub fn assemble_one(source_file: SourceFile) -> Result<AssembledProject, Vec<ProjectError>> {
    let root = source_file
        .path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let config = ProjectConfig::for_root(&root);
    assemble(&config, &[source_file])
}

/// Load and assemble a directory project in one operation.
///
/// # Errors
///
/// Returns all loading, parsing, and project-assembly diagnostics.
pub fn load_and_assemble(root: &Path) -> Result<AssembledProject, Vec<ProjectError>> {
    let (config, sources) = load(root)?;
    assemble(&config, &sources)
}

fn load_config(root: &Path, manifest_path: &Path) -> Result<ProjectConfig, Vec<ProjectError>> {
    if !manifest_path.is_file() {
        return Ok(ProjectConfig::for_root(root));
    }
    let text = std::fs::read_to_string(manifest_path)
        .map_err(|error| vec![ProjectError::io(manifest_path, &error)])?;
    ProjectConfig::parse(&text, manifest_path).map_err(|mut errors| {
        for error in &mut errors {
            error.path = Some(manifest_path.to_path_buf());
        }
        errors
    })
}

fn parse_sources(
    paths: Vec<PathBuf>,
    configured: Option<Flavor>,
) -> Result<Vec<SourceFile>, Vec<ProjectError>> {
    let mut sources = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        match parse_source(path, configured) {
            Ok(source) => sources.push(source),
            Err(mut source_errors) => errors.append(&mut source_errors),
        }
    }
    if errors.is_empty() {
        Ok(sources)
    } else {
        Err(errors)
    }
}

fn parse_source(
    path: PathBuf,
    configured: Option<Flavor>,
) -> Result<SourceFile, Vec<ProjectError>> {
    let source =
        std::fs::read_to_string(&path).map_err(|error| vec![ProjectError::io(&path, &error)])?;
    let label = path.to_string_lossy();
    let flavor = osprey_syntax::resolve_flavor(configured, &label, &source).map_err(|message| {
        vec![ProjectError {
            message,
            path: Some(path.clone()),
            line: None,
            column: None,
        }]
    })?;
    let parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if !parsed.errors.is_empty() {
        return Err(parsed
            .errors
            .into_iter()
            .map(|error| ProjectError {
                message: error.message,
                path: Some(path.clone()),
                line: usize::try_from(error.position.line).ok(),
                column: usize::try_from(error.position.column).ok(),
            })
            .collect());
    }
    Ok(SourceFile {
        path,
        flavor,
        source,
        program: parsed.program,
    })
}
