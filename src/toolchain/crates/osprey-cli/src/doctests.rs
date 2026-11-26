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
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) fn run(cli: &Cli) -> ExitCode {
    let sources = match load(cli) {
        Ok(sources) => sources,
        Err(error) => return crate::toolchain::fail(&error),
    };
    let input = match sources.input(cli) {
        Ok(input) => input,
        Err(error) => return crate::toolchain::fail(&error),
    };
    if crate::report_type_errors(&input, cli.warnings) != 0 {
        return ExitCode::FAILURE;
    }
    run_sources(cli, &sources)
}

/// Every declaration's examples, spread over one worker per core: each example
/// builds its own scratch program and runs as its own process, so they share
/// nothing but the read-only sources. [DOC-DOCTEST-HARNESS]
fn run_sources(cli: &Cli, sources: &SourceSet) -> ExitCode {
    let entries: Vec<(usize, DocEntry)> = sources
        .sources
        .iter()
        .enumerate()
        .flat_map(|(index, source)| {
            collect(&source.program)
                .into_iter()
                .map(move |e| (index, e))
        })
        .collect();
    let (next, passed, failed) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers.min(entries.len()) {
            let _ = scope.spawn(|| {
                while let Some((index, entry)) = entries.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let (ok, bad) = check_entry(cli, sources, *index, entry);
                    let _ = passed.fetch_add(ok, Ordering::Relaxed);
                    let _ = failed.fetch_add(bad, Ordering::Relaxed);
                }
            });
        }
    });
    let (passed, failed) = (passed.into_inner(), failed.into_inner());
    println!("doctests: {passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Run one declaration's examples, and count as a failure everything in its
/// `# Examples` section that would otherwise check nothing: an expectation
/// nobody compares must never read as a pass. Returns `(passed, failed)`.
fn check_entry(
    cli: &Cli,
    sources: &SourceSet,
    source_index: usize,
    entry: &DocEntry,
) -> (usize, usize) {
    let path = sources
        .sources
        .get(source_index)
        .map_or_else(String::new, |source| source.path.display().to_string());
    let mut failed = 0;
    for problem in entry.example_problems() {
        eprintln!("{path}: {}: {problem}", entry.qualified_name);
        failed += 1;
    }
    let mut passed = 0;
    for (index, example) in entry.examples().enumerate() {
        let name = format!("{path}: {} example {}", entry.qualified_name, index + 1);
        match run_example(cli, sources, source_index, entry, example, &name) {
            Ok(()) => passed += 1,
            Err(error) => {
                eprintln!("{name}: {error}");
                failed += 1;
            }
        }
    }
    (passed, failed)
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
    let warnings = if cli.warnings == crate::invocation::Warnings::Skip {
        Vec::new()
    } else {
        osprey_types::source_warnings(&parsed.program, &example.code, source.flavor, &source.path)
    };
    let synthesized = synthesize::program(
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
    let source = format!("{context}\n{}", example.code);
    let input = CompilationInput::doctest(name, source, synthesized.checked, warnings);
    execute::check(cli, &input, synthesized.backend, example)
}
