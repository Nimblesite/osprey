//! Original documentation-bearing sources before project assembly.
//! Both API export and doctests use the compiler's flavor/project rules.
//! Implements [DOC-EXPORT] and [DOC-DOCTEST-HARNESS].

use crate::{project, Cli};
use osprey_project::{ProjectConfig, SourceFile};
use std::path::Path;

pub(crate) struct SourceSet {
    pub(crate) sources: Vec<SourceFile>,
    pub(crate) config: Option<ProjectConfig>,
}

impl SourceSet {
    pub(crate) fn input(&self, cli: &Cli) -> Result<project::CompilationInput, String> {
        match &self.config {
            Some(config) => {
                project::CompilationInput::documentation_project(&cli.path, config, &self.sources)
                    .map_err(|errors| {
                        errors
                            .iter()
                            .map(|error| project::format_project_error(error, &cli.path))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
            }
            None => crate::load_input(cli)
                .map_err(|_exit_code| "documentation source could not be loaded".into()),
        }
    }
}

pub(crate) fn load(cli: &Cli) -> Result<SourceSet, String> {
    if project::is_project_path(&cli.path) {
        return load_project(cli);
    }
    let path = std::fs::canonicalize(&cli.path).map_err(|error| error.to_string())?;
    let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let flavor = osprey_syntax::resolve_flavor(cli.flavor, &cli.path, &source)?;
    let parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if crate::report_syntax_errors(&cli.path, &parsed.errors) {
        return Err(format!(
            "{}: documentation source contains syntax errors",
            cli.path
        ));
    }
    Ok(SourceSet {
        config: None,
        sources: vec![SourceFile {
            path,
            flavor,
            source,
            program: parsed.program,
        }],
    })
}

fn load_project(cli: &Cli) -> Result<SourceSet, String> {
    if cli.flavor.is_some() {
        return Err("--flavor applies to single files; projects select flavor per source".into());
    }
    let selected = Path::new(&cli.path);
    if !selected.exists() {
        return Err(format!("{}: project path does not exist", cli.path));
    }
    let root = if selected.is_dir() {
        selected
    } else {
        selected.parent().unwrap_or_else(|| Path::new("."))
    };
    osprey_project::load(root)
        .map(|(config, sources)| SourceSet {
            config: Some(config),
            sources,
        })
        .map_err(|errors| {
            errors
                .iter()
                .map(|error| project::format_project_error(error, &cli.path))
                .collect::<Vec<_>>()
                .join("\n")
        })
}
