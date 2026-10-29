//! `osprey` — the Osprey compiler's command-line front end.
//!
//! Modes: report type errors (`--check`, the default — the editor's
//! diagnostics path), dump the AST (`--ast`), emit LLVM IR (`--llvm`), build
//! an executable (`--compile`), compile-and-run via clang (`--run`), emit the
//! document outline as JSON (`--symbols`), list statically-discoverable test
//! cases as JSON (`--list-tests`, [TESTING-LIST]), or print a built-in's
//! signature as markdown (`--hover <name>`). `--profile` runs under the
//! sampling CPU profiler and prints a report ([PROF-CLI-RUN],
//! docs/specs/tooling/0028-Profiler.md). `osprey test` discovers and runs
//! test suites ([TESTING-CLI-RUN], `test_cmd`). Every compiling mode gates on Hindley-Milner
//! type inference first — an ill-typed program never reaches codegen — and on
//! the capability sandbox (`--sandbox`, `--no-http`, `--no-websocket`,
//! `--no-fs`, `--no-ffi`). `--quiet` suppresses progress output and nothing
//! else: errors and warnings are diagnostics and print regardless.
//! `--no-warnings` skips the warning pass, which packaging builds use because
//! the redundant-annotation oracle re-infers the program per proof. The C
//! driver used to link the emitted IR is `clang`, overridable via `OSPREY_CC`.
//!
//! `osprey lsp` runs the Language Server Protocol over stdio (the `osprey-lsp`
//! crate, built on the pinned lspkit crates); the `--symbols`/`--hover`
//! outline/signature helpers it shares now live there too.

mod android;
mod debug_options;
mod docs;
mod doctests;
mod document_entries;
mod document_source;
mod fmt;
mod html_escape;
mod hunt;
mod ios;
mod ios_abi;
mod project;
mod sandbox;
mod signature;
mod target_capabilities;
mod test_cmd;
mod test_coverage;
mod test_discovery;
mod test_skips;
#[cfg(test)]
#[path = "../../testkit.rs"]
mod testkit;
mod toolchain;
mod warnings;
#[cfg(test)]
use osprey_syntax::Flavor;
mod wasm;

use std::process::ExitCode;

mod executable_cache;
mod invocation;
mod linking;
mod native;
mod pipeline;
mod profiling;
mod python_target;
use invocation::{
    parse_args, parse_flavor, parse_memory, Cli, TEST_CACHE_DIR_ENV, TEST_COVERAGE_BUILD_ENV, USAGE,
};
use linking::find_runtime_lib;
#[cfg(test)]
use native::c_compiler;
use native::{child_exit_code, native_executable, scratch_stem};
use pipeline::{load_input, reject_cross_target_options, report_type_errors, run, target_error};

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
        Some(("hunt", rest)) => hunt::run(rest),
        _ if args.iter().any(|arg| arg == "--docs") => docs::run(&args),
        _ => run_arguments(&args),
    }
}

fn report_version(args: &[String]) -> ExitCode {
    // [SWR-VERSION-BUILD-STAMPING] [SWR-VERSION-CLI-OUTPUT]
    let version = option_env!("OSPREY_VERSION").map_or("0.0.0-dev", |value| value);
    // The commit follows the probed `name version` line, so a stale build is
    // visible without disturbing the version a probe parses.
    let commit = env!("OSPREY_COMMIT");
    if args.iter().any(|arg| arg == "--json") {
        println!(
            "{{\"manifestVersion\":1,\"name\":\"osprey\",\"version\":\"{version}\",\
\"kind\":\"cli\",\"product\":\"osprey\",\"commit\":\"{commit}\"}}"
        );
    } else {
        println!("osprey {version}\ncommit {commit}");
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
        "--deps" => report_dependencies(&cli),
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

/// Print checked signal may-read sets after module assembly, before discharge.
/// Implements [STAGE-SIGNALS-EXACT].
fn report_dependencies(cli: &Cli) -> ExitCode {
    let input = match load_input(cli) {
        Ok(input) => input,
        Err(code) => return code,
    };
    match osprey_types::signal_dependencies(input.program()) {
        Ok(dependencies) => {
            println!("# conservative signal may-read sets; runtime choices include all possible targets; <unknown> is an unresolved remainder");
            for (function, operations) in dependencies {
                if !operations.is_empty() {
                    println!("{function}: {}", operations.join(", "));
                }
            }
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{}", input.diagnostic(error.position, &error.message));
            }
            ExitCode::FAILURE
        }
    }
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
    let served = runtime.block_on(osprey_lsp::run_stdio());
    runtime.shutdown_background();
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("osprey lsp: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
