//! `osprey <file> --signature`: the ML export signature of every module of
//! one source file, as an importing checker receives it under `hm`
//! ([TYPES-DISCIPLINE-PROTOCOL]). A reference discipline such as
//! `passthrough` asks the compiler for its module's signature this way
//! instead of carrying a second inference.

use crate::invocation::Cli;
use osprey_project::{ProjectConfig, SourceFile};
use std::path::Path;
use std::process::ExitCode;

/// Print the signature blocks of the file at `cli.path`, or the diagnostic.
pub(crate) fn print(cli: &Cli) -> ExitCode {
    match signature(cli) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn signature(cli: &Cli) -> Result<String, String> {
    if crate::project::is_project_path(&cli.path) {
        return Err("--signature applies to a single source file".to_string());
    }
    let path = Path::new(&cli.path);
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("error: cannot read {}: {error}", cli.path))?;
    let flavor = osprey_syntax::resolve_flavor(cli.flavor, &cli.path, &source)?;
    let parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if let Some(error) = parsed.errors.first() {
        return Err(format!(
            "{}:{}:{}: {}",
            cli.path, error.position.line, error.position.column, error.message
        ));
    }
    let file = SourceFile {
        path: std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
        flavor,
        source,
        program: parsed.program,
        discipline: None,
    };
    let root = path.parent().map_or_else(|| Path::new("."), Path::new);
    osprey_project::discipline::export_signature(&ProjectConfig::for_root(root), &file)
        .map_err(|error| error.message)
}
