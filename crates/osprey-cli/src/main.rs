//! `osprey` — the Osprey compiler's command-line front end.
//!
//! Modes: report type errors (`--check`, the default — the editor's
//! diagnostics path), dump the AST (`--ast`), emit LLVM IR (`--llvm`), build
//! an executable (`--compile`), compile-and-run via clang (`--run`), emit the
//! document outline as JSON (`--symbols`), list statically-discoverable test
//! cases as JSON (`--list-tests`, [TESTING-LIST]), or print a built-in's
//! signature as markdown (`--hover <name>`). `--profile` runs under the
//! sampling CPU profiler and prints a report ([PROF-CLI-RUN],
//! docs/specs/0028-Profiler.md). `osprey test` discovers and runs
//! test suites ([TESTING-CLI-RUN], `test_cmd`). Every compiling mode gates on Hindley-Milner
//! type inference first — an ill-typed program never reaches codegen — and on
//! the capability sandbox (`--sandbox`, `--no-http`, `--no-websocket`,
//! `--no-fs`, `--no-ffi`). `--quiet` suppresses non-essential output. The C
//! driver used to link the emitted IR is `clang`, overridable via `OSPREY_CC`.
//!
//! `osprey lsp` runs the Language Server Protocol over stdio (the `osprey-lsp`
//! crate, built on the published lspkit crates); the `--symbols`/`--hover`
//! outline/signature helpers it shares now live there too.

mod android;
mod docs;
mod doctests;
mod document_entries;
mod document_source;
mod fmt;
mod ios;
mod ios_abi;
mod project;
mod sandbox;
mod target_capabilities;
mod test_cmd;
mod test_coverage;
mod test_skips;
#[cfg(test)]
#[path = "../../testkit.rs"]
mod testkit;
mod toolchain;
mod warnings;
mod wasm;

use osprey_syntax::Flavor;
use std::process::ExitCode;

mod executable_cache;
mod invocation;
mod linking;
mod native;
mod pipeline;
mod profiling;
use invocation::{
    parse_args, parse_flavor, parse_memory, Cli, TEST_CACHE_DIR_ENV, TEST_COVERAGE_BUILD_ENV, USAGE,
};
use linking::find_runtime_lib;
#[cfg(test)]
use native::c_compiler;
use native::{child_exit_code, native_executable, scratch_stem};
use pipeline::{
    build_kind, load_input, reject_cross_target_options, report_type_errors, run, target_error,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .split_first()
        .map(|(first, rest)| (first.as_str(), rest))
    {
        Some(("--version", _)) => report_version(&args),
        Some(("lsp", _)) => run_lsp(),
        Some(("fmt", rest)) => fmt::run(rest),
        Some(("test", rest)) => test_cmd::run(rest),
        _ if args.iter().any(|arg| arg == "--docs") => docs::run(&args),
        _ => run_arguments(&args),
    }
}

fn report_version(args: &[String]) -> ExitCode {
    // [SWR-VERSION-BUILD-STAMPING] [SWR-VERSION-CLI-OUTPUT]
    let version = option_env!("OSPREY_VERSION").map_or("0.0.0-dev", |value| value);
    if args.iter().any(|arg| arg == "--json") {
        println!(
            "{{\"manifestVersion\":1,\"name\":\"osprey\",\"version\":\"{version}\",\
\"kind\":\"cli\",\"product\":\"osprey\"}}"
        );
    } else {
        println!("osprey {version}");
    }
    ExitCode::SUCCESS
}

fn run_arguments(args: &[String]) -> ExitCode {
    let cli = match parse_args(args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match cli.mode.as_str() {
        "--deps" => report_dependencies(&cli.path, cli.flavor),
        "--hover" => report_hover(&cli.path),
        _ => run(&cli),
    }
}

fn report_hover(name: &str) -> ExitCode {
    if let Some(markdown) = osprey_lsp::builtin_hover(name) {
        println!("{markdown}");
    }
    ExitCode::SUCCESS
}

/// Print every function's dependency set, one `name: op, op` line per function
/// that has one. Implements [STAGE-SIGNALS-DIRTY].
fn report_dependencies(path: &str, flavor: Option<Flavor>) -> ExitCode {
    let program = match dependency_program(path, flavor) {
        Ok(program) => program,
        Err(code) => return code,
    };
    // Check before static discharge erases operations [STAGE-SIGNALS-EXACT].
    if dependency_type_errors(path, &program) {
        return ExitCode::FAILURE;
    }
    for (function, operations) in osprey_ast::stage::dependencies(&program) {
        if !operations.is_empty() {
            println!("{function}: {}", operations.join(", "));
        }
    }
    ExitCode::SUCCESS
}

fn dependency_program(path: &str, flavor: Option<Flavor>) -> Result<osprey_ast::Program, ExitCode> {
    let source = std::fs::read_to_string(path).map_err(|_error| {
        eprintln!("error: cannot read {path}");
        ExitCode::from(2)
    })?;
    let flavor = osprey_syntax::resolve_flavor(flavor, path, &source).map_err(|_error| {
        eprintln!("error: cannot resolve the flavor of {path}");
        ExitCode::from(2)
    })?;
    let parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if report_syntax_errors(path, &parsed.errors) {
        return Err(ExitCode::FAILURE);
    }
    Ok(parsed.program)
}

fn dependency_type_errors(path: &str, program: &osprey_ast::Program) -> bool {
    let errors = osprey_types::check_program(program);
    for error in &errors {
        if let Some(position) = error.position {
            eprintln!(
                "{path}:{}:{}: {}",
                position.line, position.column, error.message
            );
        } else {
            eprintln!("{path}: {}", error.message);
        }
    }
    !errors.is_empty()
}

/// Print syntax errors in the one canonical `path:line:col: message` form and
/// say whether there were any, so every mode that refuses to answer from a file
/// that did not parse refuses it identically.
fn report_syntax_errors(path: &str, errors: &[osprey_syntax::SyntaxError]) -> bool {
    for err in errors {
        eprintln!(
            "{path}:{}:{}: {}",
            err.position.line, err.position.column, err.message
        );
    }
    !errors.is_empty()
}

/// Run the stdio language server to completion on a fresh Tokio runtime.
fn run_lsp() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("osprey lsp: cannot start async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(osprey_lsp::run_stdio()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("osprey lsp: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
