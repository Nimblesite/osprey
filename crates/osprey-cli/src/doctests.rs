//! Execute documentation examples independently in their source flavor.
//! Implements [DOC-DOCTEST-HARNESS].

mod entry;
mod execute;
mod process;
mod synthesize;

use crate::document_entries::{collect, DocEntry};
use crate::document_source::{load, SourceSet};
use crate::project::CompilationInput;
use crate::Cli;
use osprey_ast::DocExample;
use std::process::ExitCode;

pub(crate) fn run(cli: &Cli) -> ExitCode {
    let sources = match load(cli) {
        Ok(sources) => sources,
        Err(error) => return crate::toolchain::fail(&error),
    };
    let input = match sources.input(cli) {
        Ok(input) => input,
        Err(error) => return crate::toolchain::fail(&error),
    };
    if crate::report_type_errors(&input) != 0 {
        return ExitCode::FAILURE;
    }
    run_sources(cli, &sources)
}

fn run_sources(cli: &Cli, sources: &SourceSet) -> ExitCode {
    let mut passed = 0;
    let mut failed = 0;
    for (source_index, source) in sources.sources.iter().enumerate() {
        for entry in collect(&source.program) {
            for (index, example) in entry.examples().enumerate() {
                let name = format!(
                    "{}: {} example {}",
                    source.path.display(),
                    entry.qualified_name,
                    index + 1
                );
                match run_example(cli, sources, source_index, &entry, example, &name) {
                    Ok(()) => passed += 1,
                    Err(error) => {
                        eprintln!("{name}: {error}");
                        failed += 1;
                    }
                }
            }
        }
    }
    println!("doctests: {passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_example(
    cli: &Cli,
    sources: &SourceSet,
    source_index: usize,
    entry: &DocEntry,
    example: &DocExample,
    name: &str,
) -> Result<(), String> {
    let source = sources
        .sources
        .get(source_index)
        .ok_or("documentation source is missing")?;
    let parsed = osprey_syntax::parse_program_with_flavor(&example.code, source.flavor);
    if crate::report_syntax_errors(name, &parsed.errors) {
        return Err("documentation example contains syntax errors".into());
    }
    let program = synthesize::program(
        &sources.sources,
        sources.config.as_ref(),
        source_index,
        &entry.scope,
        parsed.program,
    )?;
    let context = sources
        .sources
        .iter()
        .map(|source| source.source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let input = CompilationInput::script(name, format!("{context}\n{}", example.code), program);
    execute::check(cli, &input, example)
}
