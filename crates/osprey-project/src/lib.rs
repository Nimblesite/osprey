//! Osprey project loading and module resolution.
//!
//! The project layer is the only phase allowed to know physical source paths.
//! It parses mixed `.osp`/`.ospml` inputs, assembles logical namespace
//! contributions, resolves module imports, and emits one flavor-neutral
//! canonical program for checking and code generation. Implements
//! [MODULES-MODEL], [MODULES-PATH-INDEPENDENCE], and [MODULES-PROJECT].

mod contribution;
mod manifest;
mod model;
mod source;

pub use manifest::ProjectConfig;
use osprey_syntax::Flavor;
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
}

impl ProjectError {
    fn manifest(line: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
            line: Some(line),
        }
    }

    fn io(path: &Path, error: &std::io::Error) -> Self {
        Self {
            message: error.to_string(),
            path: Some(path.to_path_buf()),
            line: None,
        }
    }
}

/// Read a directory project and parse every configured source file.
pub fn load(root: &Path) -> Result<(ProjectConfig, Vec<SourceFile>), Vec<ProjectError>> {
    let manifest_path = root.join("osprey.toml");
    let config = load_config(root, &manifest_path)?;
    let paths = source::discover(root, &config).map_err(|error| vec![error])?;
    parse_sources(paths, config.flavor).map(|sources| (config, sources))
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

fn parse_source(path: PathBuf, configured: Option<Flavor>) -> Result<SourceFile, Vec<ProjectError>> {
    let source = std::fs::read_to_string(&path)
        .map_err(|error| vec![ProjectError::io(&path, &error)])?;
    let label = path.to_string_lossy();
    let flavor = osprey_syntax::resolve_flavor(configured, &label, &source).map_err(|message| {
        vec![ProjectError {
            message,
            path: Some(path.clone()),
            line: None,
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
