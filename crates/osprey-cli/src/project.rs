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

    /// The flavor-neutral program consumed by the checker and backend.
    pub(crate) fn program(&self) -> &Program {
        match &self.unit {
            CompilationUnit::Script(program) => program,
            CompilationUnit::Project(project) => &project.program,
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

    /// Format a flattened checker location using its physical source file.
    pub(crate) fn diagnostic(&self, position: Option<Position>, message: &str) -> String {
        let Some(position) = position else {
            return format!("{}: {message}", self.display_path);
        };
        if let CompilationUnit::Project(project) = &self.unit {
            if let Some((source, line)) = project.source_at_line(position.line) {
                return format!(
                    "{}:{line}:{}: {message}",
                    source.path.display(),
                    position.column
                );
            }
        }
        format!(
            "{}:{}:{}: {message}",
            self.display_path, position.line, position.column
        )
    }

    /// Render symbols with source-level qualified names where assembly mangled them.
    pub(crate) fn symbols_json(&self) -> String {
        let json = osprey_lsp::symbols_json(self.program());
        let CompilationUnit::Project(project) = &self.unit else {
            return json;
        };
        project_symbols_json(json, project)
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
    if target == "wasm32" {
        let mut wasm = output.into_os_string();
        wasm.push(".wasm");
        return PathBuf::from(wasm);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_directory_and_manifest_project_inputs() {
        assert!(is_project_path("osprey.toml"));
        assert!(!is_project_path("main.osp"));
        assert!(is_project_path(
            std::env::temp_dir().to_string_lossy().as_ref()
        ));
    }

    #[test]
    fn lexical_and_absolute_project_roots_have_one_identity() {
        let lexical = project_root(Path::new("."));
        let absolute = normalize_path(Path::new("."));
        assert_eq!(lexical, absolute);
        assert_eq!(
            ProjectConfig::for_root(&lexical).name,
            ProjectConfig::for_root(&absolute).name
        );
    }

    #[test]
    fn only_module_aware_programs_request_single_source_assembly() {
        let script = osprey_syntax::parse_program("let answer = 42\n").program;
        let module =
            osprey_syntax::parse_program("module Answers {\n    export let answer = 42\n}\n")
                .program;
        assert!(!needs_assembly(&script));
        assert!(needs_assembly(&module));
    }

    #[test]
    fn source_output_defaults_match_the_existing_cli() {
        let program = osprey_syntax::parse_program("let answer = 42\n").program;
        let input = CompilationInput::script("nested/main.osp", String::new(), program);
        assert_eq!(input.output_path(None, "native"), PathBuf::from("main"));
        assert_eq!(
            input.output_path(None, "wasm32"),
            PathBuf::from("main.wasm")
        );
        assert_eq!(
            input.output_path(Some("custom/out"), "native"),
            PathBuf::from("custom/out")
        );
    }

    #[test]
    fn aggregate_sources_keeps_link_directives_from_every_file() {
        let sources = [
            ("first.osp", "// @link: sqlite3\nlet x = 1\n"),
            ("second.ospml", "// @linkdir: /opt/lib\ny = 2\n"),
        ];
        let sources = sources
            .iter()
            .enumerate()
            .map(|(index, (path, source))| osprey_project::SourceMetadata {
                index,
                path: PathBuf::from(path),
                flavor: Flavor::Default,
                source: (*source).to_string(),
                global_line_start: 1,
                global_line_end: 2,
            })
            .collect();
        let project = AssembledProject {
            program: Program {
                statements: Vec::new(),
            },
            entry_prologue: Vec::new(),
            entry_source: 0,
            sources,
            source_name_by_mangled: std::collections::BTreeMap::new(),
        };
        let aggregated = aggregate_sources(&project);
        assert!(aggregated.contains("// @link: sqlite3"));
        assert!(aggregated.contains("// @linkdir: /opt/lib"));

        let input = CompilationInput::assembled(
            project,
            aggregated,
            "demo".to_string(),
            OutputDefault::Project {
                root: PathBuf::from("out"),
                name: "demo".to_string(),
            },
        );
        assert_eq!(input.debug_path(), "first.osp");
        assert_eq!(
            input.diagnostic(Some(Position { line: 2, column: 3 }), "bad"),
            "first.osp:2:3: bad"
        );
        assert_eq!(
            input.output_path(None, "wasm32"),
            PathBuf::from("out/demo.wasm")
        );
    }

    #[test]
    fn project_symbol_mapping_changes_exact_names_and_localizes_positions() {
        let program = osprey_syntax::parse_program("fn main() = 1\nfn remaining() = 2\n").program;
        let source = osprey_project::SourceMetadata {
            index: 0,
            path: PathBuf::from("src/main.osp"),
            flavor: Flavor::Default,
            source: String::new(),
            global_line_start: 1,
            global_line_end: 2,
        };
        let mut source_names = std::collections::BTreeMap::new();
        let _ = source_names.insert("main".to_string(), "app::main".to_string());
        let project = AssembledProject {
            program,
            entry_prologue: Vec::new(),
            entry_source: 0,
            sources: vec![source],
            source_name_by_mangled: source_names,
        };
        let json = project_symbols_json(osprey_lsp::symbols_json(&project.program), &project);
        assert!(json.contains("\"name\":\"app::main\""));
        assert!(json.contains("\"name\":\"remaining\""));
        assert!(!json.contains("reapp::maining"));
        assert!(json.contains("\"path\":\"src/main.osp\""));
    }

    fn empty_project(
        source_name_by_mangled: std::collections::BTreeMap<String, String>,
    ) -> AssembledProject {
        AssembledProject {
            program: Program {
                statements: Vec::new(),
            },
            entry_prologue: Vec::new(),
            entry_source: 0,
            sources: Vec::new(),
            source_name_by_mangled,
        }
    }

    #[test]
    fn an_entryless_project_falls_back_to_its_display_path() {
        let input = CompilationInput::assembled(
            empty_project(std::collections::BTreeMap::new()),
            String::new(),
            "whole/project".to_string(),
            OutputDefault::Project {
                root: PathBuf::from("out"),
                name: "demo".to_string(),
            },
        );
        // No entry source → debug_path and an unlocatable diagnostic both use display_path.
        assert_eq!(input.debug_path(), "whole/project");
        assert_eq!(
            input.diagnostic(Some(Position { line: 7, column: 2 }), "oops"),
            "whole/project:7:2: oops"
        );
        assert_eq!(input.diagnostic(None, "oops"), "whole/project: oops");
    }

    #[test]
    fn project_symbol_projection_is_a_noop_on_unmappable_input() {
        let project = empty_project(std::collections::BTreeMap::new());
        // Non-JSON input is returned verbatim.
        assert_eq!(
            project_symbols_json("not json".to_string(), &project),
            "not json"
        );
        // Valid JSON that is not an array is also returned verbatim.
        assert_eq!(
            project_symbols_json("{\"a\":1}".to_string(), &project),
            "{\"a\":1}"
        );
        // An array entry that is not an object, has no mapped name, and no
        // position exercises every early-return guard in update_project_symbol.
        assert_eq!(project_symbols_json("[42]".to_string(), &project), "[42]");
    }

    #[test]
    fn project_errors_include_every_available_location_component() {
        let cases = [
            (
                Some(PathBuf::from("src/a.osp")),
                Some(2),
                Some(3),
                "src/a.osp:2:3: bad",
            ),
            (None, Some(4), None, "fallback:4: bad"),
            (
                Some(PathBuf::from("src/b.osp")),
                None,
                Some(9),
                "src/b.osp: bad",
            ),
        ];
        for (path, line, column, expected) in cases {
            let error = ProjectError {
                message: "bad".to_string(),
                path,
                line,
                column,
            };
            assert_eq!(format_project_error(&error, "fallback"), expected);
        }
    }
}
