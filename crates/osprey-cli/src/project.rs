//! CLI-facing project input and source-location handling.

use osprey_ast::{Position, Program};
use osprey_project::{AssembledProject, ProjectConfig, ProjectError, SourceFile};
use osprey_syntax::Flavor;
use std::path::{Path, PathBuf};

/// A parsed script or an assembled multi-file project ready for dispatch.
#[derive(Debug)]
pub(crate) struct CompilationInput {
    unit: CompilationUnit,
    source: String,
    display_path: String,
    debug_path: String,
    output: OutputDefault,
}

#[derive(Debug)]
enum CompilationUnit {
    Script(Program),
    Project(AssembledProject),
}

#[derive(Debug)]
enum OutputDefault {
    Source(String),
    Project { root: PathBuf, name: String },
}

impl CompilationInput {
    /// Documentation resolves every module without requiring an application entry.
    /// The selected source supplies assembly context; no application code is run.
    pub(crate) fn documentation_project(
        path: &str,
        config: &ProjectConfig,
        sources: &[SourceFile],
    ) -> Result<Self, Vec<ProjectError>> {
        let mut config = config.clone();
        if config.entry.is_none() {
            config.entry = sources.first().map(|source| source.path.clone());
        }
        let assembled = osprey_project::assemble(&config, sources)?;
        let source = aggregate_sources(&assembled);
        Ok(Self::assembled(
            assembled,
            source,
            path.to_string(),
            OutputDefault::Source(path.to_string()),
        ))
    }

    /// Preserve the historical single-file path for an ordinary script.
    pub(crate) fn script(path: &str, source: String, program: Program) -> Self {
        Self {
            unit: CompilationUnit::Script(program),
            source,
            display_path: path.to_string(),
            debug_path: path.to_string(),
            output: OutputDefault::Source(path.to_string()),
        }
    }

    /// Assemble one module-aware source without sweeping up sibling files.
    pub(crate) fn one_source(
        path: &str,
        flavor: Flavor,
        source: String,
        program: Program,
    ) -> Result<Self, Vec<ProjectError>> {
        let source_file = SourceFile {
            path: normalize_path(Path::new(path)),
            flavor,
            source: source.clone(),
            program,
        };
        let assembled = osprey_project::assemble_one(source_file)?;
        Ok(Self::assembled(
            assembled,
            source,
            path.to_string(),
            OutputDefault::Source(path.to_string()),
        ))
    }

    /// Load a directory project or a path to its `osprey.toml` manifest.
    pub(crate) fn load_project(path: &str) -> Result<Self, Vec<ProjectError>> {
        let selected = Path::new(path);
        if selected.file_name().and_then(|name| name.to_str()) == Some("osprey.toml")
            && !selected.is_file()
        {
            return Err(vec![ProjectError {
                message: "manifest does not exist".to_string(),
                path: Some(selected.to_path_buf()),
                line: None,
                column: None,
            }]);
        }
        let root = project_root(selected);
        let assembled = osprey_project::load_and_assemble(&root)?;
        let source = aggregate_sources(&assembled);
        let name = project_name(&root);
        Ok(Self::assembled(
            assembled,
            source,
            root.display().to_string(),
            OutputDefault::Project { root, name },
        ))
    }

    fn assembled(
        assembled: AssembledProject,
        source: String,
        display_path: String,
        output: OutputDefault,
    ) -> Self {
        let debug_path = assembled.entry().map_or_else(
            || display_path.clone(),
            |entry| entry.path.display().to_string(),
        );
        Self {
            unit: CompilationUnit::Project(assembled),
            source,
            display_path,
            debug_path,
            output,
        }
    }

    /// The flavor-neutral program the checker and every source-level tool read.
    pub(crate) fn program(&self) -> &Program {
        match &self.unit {
            CompilationUnit::Script(program) => program,
            CompilationUnit::Project(project) => &project.program,
        }
    }

    /// The program code generation lowers. A project's opaque aliases are
    /// expanded to their representations there ([MODULES-OPAQUE-TYPES]).
    pub(crate) fn backend_program(&self) -> &Program {
        match &self.unit {
            CompilationUnit::Script(program) => program,
            CompilationUnit::Project(project) => project.backend_program(),
        }
    }

    /// All original source text used to discover project-wide link directives.
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// User-facing path label for diagnostics that have no precise source.
    pub(crate) fn display_path(&self) -> &str {
        &self.display_path
    }

    /// Entry source used for the backend's single-file debug metadata API.
    pub(crate) fn debug_path(&self) -> &str {
        &self.debug_path
    }

    /// Resolve a flattened checker position to the physical file it was
    /// written in, with that file's own line number.
    pub(crate) fn location(&self, position: Position) -> (String, u32, u32) {
        if let CompilationUnit::Project(project) = &self.unit {
            if let Some((source, line)) = project.source_at_line(position.line) {
                return (source.path.display().to_string(), line, position.column);
            }
        }
        (self.display_path.clone(), position.line, position.column)
    }

    /// Format a flattened checker location using its physical source file.
    pub(crate) fn diagnostic(&self, position: Option<Position>, message: &str) -> String {
        let Some(position) = position else {
            return format!("{}: {message}", self.display_path);
        };
        let (path, line, column) = self.location(position);
        format!("{path}:{line}:{column}: {message}")
    }

    /// Render symbols with source-level qualified names where assembly mangled them.
    pub(crate) fn symbols_json(&self) -> String {
        let json = osprey_lsp::symbols_json(self.program());
        let CompilationUnit::Project(project) = &self.unit else {
            return json;
        };
        project_symbols_json(json, project)
    }

    /// Include constants removed by assembly when presenting documentation types.
    pub(crate) fn documentation_symbols_json(&self) -> String {
        let CompilationUnit::Project(project) = &self.unit else {
            return self.symbols_json();
        };
        let mut documented = project.clone();
        documented
            .program
            .statements
            .extend(project.documentation_bindings.clone());
        project_symbols_json(osprey_lsp::symbols_json(&documented.program), &documented)
    }

    /// The finalized module API, including signature exports and opaque types.
    pub(crate) fn public_api(&self) -> Option<&std::collections::BTreeMap<String, bool>> {
        match &self.unit {
            CompilationUnit::Project(project) => Some(&project.public_api),
            CompilationUnit::Script(_) => None,
        }
    }

    pub(crate) fn project_warnings(&self) -> &[osprey_project::ProjectWarning] {
        match &self.unit {
            CompilationUnit::Project(project) => &project.warnings,
            CompilationUnit::Script(_) => &[],
        }
    }

    pub(crate) fn state_boundaries(&self) -> &[osprey_project::StateBoundary] {
        match &self.unit {
            CompilationUnit::Project(project) => &project.state_boundaries,
            CompilationUnit::Script(_) => &[],
        }
    }

    /// Honor `-o`; otherwise put project artifacts beside the manifest and
    /// retain the historical current-directory source stem for scripts.
    pub(crate) fn output_path(&self, explicit: Option<&str>, target: &str) -> PathBuf {
        if let Some(path) = explicit {
            return PathBuf::from(path);
        }
        match &self.output {
            OutputDefault::Source(path) => artifact(Path::new(path), target, false),
            OutputDefault::Project { root, name } => artifact(&root.join(name), target, true),
        }
    }
}

/// Whether a positional path selects project mode without a subcommand.
pub(crate) fn is_project_path(path: &str) -> bool {
    let path = Path::new(path);
    path.is_dir() || path.file_name().and_then(|name| name.to_str()) == Some("osprey.toml")
}

/// Module-bearing single files need resolver/flattening; ordinary scripts must
/// bypass it so their existing IR and debugger symbol names remain exact.
pub(crate) use osprey_project::needs_assembly;

/// Render a loader/resolver failure in the compiler's standard diagnostic form.
pub(crate) fn format_project_error(error: &ProjectError, fallback: &str) -> String {
    let path = error
        .path
        .as_deref()
        .map_or_else(|| fallback.to_string(), |path| path.display().to_string());
    match (error.line, error.column) {
        (Some(line), Some(column)) => format!("{path}:{line}:{column}: {}", error.message),
        (Some(line), None) => format!("{path}:{line}: {}", error.message),
        (None, _) => format!("{path}: {}", error.message),
    }
}

fn project_root(path: &Path) -> PathBuf {
    let root = if path.is_dir() {
        path
    } else {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    };
    normalize_path(root)
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn aggregate_sources(project: &AssembledProject) -> String {
    project
        .sources
        .iter()
        .map(|source| source.source.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn project_name(root: &Path) -> String {
    let manifest = root.join("osprey.toml");
    let config = std::fs::read_to_string(&manifest)
        .ok()
        .and_then(|source| ProjectConfig::parse(&source, &manifest).ok())
        .unwrap_or_else(|| ProjectConfig::for_root(root));
    config.name
}

fn project_symbols_json(json: String, project: &AssembledProject) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&json) else {
        return json;
    };
    let symbols = osprey_lsp::analysis::collect_symbols(&project.program);
    let Some(entries) = value.as_array_mut() else {
        return json;
    };
    for (entry, symbol) in entries.iter_mut().zip(&symbols) {
        update_project_symbol(entry, symbol, project);
    }
    serde_json::to_string(&value).unwrap_or(json)
}

fn update_project_symbol(
    entry: &mut serde_json::Value,
    symbol: &osprey_lsp::analysis::SymbolInfo,
    project: &AssembledProject,
) {
    restore_mangled_values(entry, &project.source_name_by_mangled);
    let Some(object) = entry.as_object_mut() else {
        return;
    };
    if let Some(source_name) = project.source_name_by_mangled.get(&symbol.name) {
        let _ = object.insert("name".to_string(), source_name.clone().into());
    }
    let Some(position) = symbol.position else {
        return;
    };
    let Some((source, line)) = project.source_at_line(position.line) else {
        return;
    };
    let _ = object.insert("line".to_string(), line.into());
    let _ = object.insert("path".to_string(), source.path.display().to_string().into());
}

fn restore_mangled_values(
    value: &mut serde_json::Value,
    source_names: &std::collections::BTreeMap<String, String>,
) {
    match value {
        serde_json::Value::String(text) => {
            for (linkage, source) in source_names {
                if linkage.starts_with("__osp_") {
                    *text = text.replace(linkage, source);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                restore_mangled_values(value, source_names);
            }
        }
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                restore_mangled_values(value, source_names);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn artifact(base: &Path, target: &str, keep_parent: bool) -> PathBuf {
    let output = if keep_parent {
        base.to_path_buf()
    } else {
        PathBuf::from(
            base.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("osprey_out"),
        )
    };
    let extension = match target {
        "wasm32" => ".wasm",
        "ios" | "ios-sim" | "android-arm64" | "android-x64" => ".a",
        _ => "",
    };
    let mut artifact = output.into_os_string();
    artifact.push(extension);
    PathBuf::from(artifact)
}

#[cfg(test)]
mod tests;
