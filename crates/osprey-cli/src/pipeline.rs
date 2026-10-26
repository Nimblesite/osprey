//! Pipeline support for the compiler driver.

use crate::invocation::{Cli, Warnings, TEST_COVERAGE_BUILD_ENV};
#[cfg(test)]
use crate::native::stem_of;
use crate::native::{build_input, compile_ir, execute_native, NativeOptions};
use crate::profiling::execute_profiled;
use crate::project::CompilationInput;
use crate::{
    android, doctests, ios, project, report_syntax_errors, sandbox, target_capabilities, toolchain,
    warnings, wasm,
};
#[cfg(test)]
use std::path::PathBuf;
use std::process::ExitCode;

/// Parse, gate (syntax → sandbox → types), and dispatch the selected mode.
pub(super) fn run(cli: &Cli) -> ExitCode {
    if cli.mode == "--doctests" {
        return doctests::run(cli);
    }
    let input = match load_input(cli) {
        Ok(input) => input,
        Err(code) => return code,
    };
    let violations = sandbox::violations(input.program(), cli.policy);
    if !violations.is_empty() {
        for violation in &violations {
            eprintln!("{}: {violation}", input.display_path());
        }
        return ExitCode::FAILURE;
    }
    dispatch(cli, &input)
}

pub(crate) fn load_input(cli: &Cli) -> Result<CompilationInput, ExitCode> {
    if project::is_project_path(&cli.path) {
        return load_project(cli);
    }
    let source = std::fs::read_to_string(&cli.path).map_err(|error| {
        eprintln!("error: cannot read {}: {error}", cli.path);
        ExitCode::from(2)
    })?;
    let flavor =
        osprey_syntax::resolve_flavor(cli.flavor, &cli.path, &source).map_err(|error| {
            eprintln!("{error}");
            ExitCode::from(2)
        })?;
    load_script(&cli.path, source, flavor)
}

fn load_project(cli: &Cli) -> Result<CompilationInput, ExitCode> {
    if cli.flavor.is_some() {
        eprintln!("error: --flavor applies to single files; projects select flavor per source");
        return Err(ExitCode::from(2));
    }
    CompilationInput::load_project(&cli.path).map_err(|errors| {
        print_project_errors(&errors, &cli.path);
        ExitCode::FAILURE
    })
}

fn load_script(
    path: &str,
    source: String,
    flavor: osprey_syntax::Flavor,
) -> Result<CompilationInput, ExitCode> {
    let parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if report_syntax_errors(path, &parsed.errors) {
        return Err(ExitCode::FAILURE);
    }
    // Preserve contracts before static discharge [STAGE-LOWER-ORDER-PHASE].
    if project::needs_assembly(&parsed.program) {
        return CompilationInput::one_source(path, flavor, source, parsed.program).map_err(
            |errors| {
                print_project_errors(&errors, path);
                ExitCode::FAILURE
            },
        );
    }
    Ok(CompilationInput::script(path, source, parsed.program))
}

pub(super) fn print_project_errors(errors: &[osprey_project::ProjectError], fallback: &str) {
    for error in errors {
        eprintln!("{}", project::format_project_error(error, fallback));
    }
}

/// Route the type-gated modes: an ill-typed program never reaches codegen.
pub(super) fn dispatch(cli: &Cli, input: &CompilationInput) -> ExitCode {
    match cli.mode.as_str() {
        "--check" => run_check(cli, input),
        "--symbols" => print_metadata(&input.symbols_json()),
        "--source-graph" => match input.dependency_graph_json() {
            Ok(graph) => print_metadata(&graph),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        "--list-tests" => print_metadata(&osprey_lsp::tests_json(input.program())),
        "--llvm" | "--run" | "--compile" if report_type_errors(input, cli.warnings) > 0 => {
            ExitCode::FAILURE
        }
        "--llvm" | "--run" | "--compile" if target_error(cli, input).is_some() => ExitCode::FAILURE,
        "--llvm" => print_ir(cli, input),
        "--run" => run_program(cli, input),
        "--compile" => compile_program_to_disk(cli, input),
        _ => print_metadata(&format!("{:#?}", input.program())),
    }
}

fn print_metadata(text: &str) -> ExitCode {
    // Editor metadata remains available for parsable, ill-typed files.
    println!("{text}");
    ExitCode::SUCCESS
}

fn print_ir(cli: &Cli, input: &CompilationInput) -> ExitCode {
    match target_ir(cli, input) {
        Ok(ir) => {
            print!("{ir}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}: {error}", input.display_path());
            ExitCode::FAILURE
        }
    }
}

/// Type-check `program`, print every error in `file:line:col: message` form,
/// and return how many there were. The shared gate for every compiling mode.
///
/// Warnings print alongside the errors and are deliberately not counted: a
/// redundant annotation is a defect to delete, never a reason to fail a build
/// ([TYPE-ANNOTATION-REDUNDANT]).
///
/// `--quiet` never reaches here: it silences progress, and a warning is a
/// diagnostic, not progress. Only `--no-warnings` ([`Warnings::Skip`]) skips
/// the warning pass, and it skips the computation too:
/// the redundant-annotation oracle re-infers the whole program once per
/// proof, which a packaging build must not pay for advice nobody reads.
/// Errors are reported either way.
pub(crate) fn report_type_errors(input: &CompilationInput, warnings: Warnings) -> usize {
    let errors = osprey_types::check_program(input.program());
    for e in &errors {
        eprintln!("{}", input.diagnostic(e.position, &e.message));
    }
    if warnings == Warnings::Report {
        warnings::report(input, &warnings::collect(input));
    }
    errors.len()
}

pub(super) fn run_check(cli: &Cli, input: &CompilationInput) -> ExitCode {
    if report_type_errors(input, cli.warnings) == 0 && target_error(cli, input).is_none() {
        if !cli.quiet {
            println!(
                "{}: ok ({} statements)",
                input.display_path(),
                input.program().statements.len()
            );
        }
        return ExitCode::SUCCESS;
    }
    ExitCode::FAILURE
}

/// Capability errors are compilation diagnostics, before LLVM or linking.
/// [IOS-TARGET-CAPABILITIES] [WASM-TARGET-CAPABILITIES]
pub(super) fn target_error(cli: &Cli, input: &CompilationInput) -> Option<ExitCode> {
    if let Err(error) = target_capabilities::validate(input.program(), &cli.target)
        .and_then(|()| target_capabilities::validate_links(input.source(), &cli.target))
    {
        eprintln!("{}: {error}", input.display_path());
        return Some(ExitCode::FAILURE);
    }
    match cli.target.as_str() {
        "wasm32" => reject_wasm_options(cli),
        "python" => app_target_error(&cli.mode, crate::python_target::validate(cli), || {
            target_ir(cli, input)
        }),
        _ => mobile_target_error(cli, input),
    }
}

fn mobile_target_error(cli: &Cli, input: &CompilationInput) -> Option<ExitCode> {
    let source = || target_ir(cli, input);
    if android::Target::parse(&cli.target).is_some() {
        return app_target_error(&cli.mode, android::validate(cli), source);
    }
    if let Some(target) = ios::Target::parse(&cli.target) {
        return app_target_error(&cli.mode, ios::validate(cli, target), source);
    }
    None
}

/// The app-library targets (iOS, Android) share one order: reject the
/// unsupported options, then — for `--check` alone — generate the C ABI and
/// report its failure as a diagnostic. `source` stays lazy so no other mode
/// pays for ABI generation, and so option errors always win the race.
pub(super) fn app_target_error(
    mode: &str,
    validation: Result<(), ExitCode>,
    source: impl FnOnce() -> Result<String, String>,
) -> Option<ExitCode> {
    if let Err(code) = validation {
        return Some(code);
    }
    if mode == "--check" {
        return source().err().map(|error| toolchain::fail(&error));
    }
    None
}

pub(super) fn target_ir(cli: &Cli, input: &CompilationInput) -> Result<String, String> {
    let program = input.backend_program();
    let path = input.debug_path();
    let scope = input.export_scope(cli.exports.entry_only());
    if cli.target == "python" {
        return crate::python_target::source(program, path, &scope).map(|(ir, _)| ir);
    }
    if let Some(target) = android::Target::parse(&cli.target) {
        return android::source(program, path, target, &scope).map(|(ir, _)| ir);
    }
    if let Some(target) = ios::Target::parse(&cli.target) {
        return ios::source(program, path, target, &scope).map(|(ir, _)| ir);
    }
    if cli.target == "wasm32" {
        return wasm::program_ir(program).map_err(|error| error.to_string());
    }
    compile_ir(path, program, build_kind(cli)).map_err(|error| error.to_string())
}

pub(super) fn reject_debug_cross_target(cli: &Cli) -> Option<ExitCode> {
    if cli.debug {
        eprintln!("error: --debug is currently supported only for --target=native");
        return Some(ExitCode::from(2));
    }
    if cli.profile {
        eprintln!("error: --profile is currently supported only for --target=native");
        return Some(ExitCode::from(2));
    }
    None
}

/// wasm32 takes the native-only build-flag rejections, but not the mobile
/// targets' "no managed archive" one: `--memory=arc` links the ARC runtime
/// built for wasip1, and the target defaults to it when `--memory` is omitted.
/// `--memory=gc` is refused — the tracing collector walks a shadow stack that
/// wasm32 does not have — and `--memory=none` stays the explicit opt-out.
/// Implements [WASM-TARGET-MEMORY].
pub(super) fn reject_wasm_options(cli: &Cli) -> Option<ExitCode> {
    if let Some(code) = reject_debug_cross_target(cli) {
        return Some(code);
    }
    (cli.memory == "gc").then(|| {
        toolchain::fail(
            "wasm32 has no garbage-collecting runtime; use --memory=arc (the default) or --memory=none",
        )
    })
}

/// Hosted targets refuse native-only flags and unsupported tracing GC.
/// A target that produces a LIBRARY rather than a runnable
/// image also refuses `--run`, and names the host that calls it in `host_hint`;
/// `None` marks a target with a `--run` form of its own.
/// Implements [IOS-TARGET-OPTIONS] and [ANDROID-TARGET-OPTIONS].
pub(super) fn reject_cross_target_options(
    cli: &Cli,
    platform: &str,
    host_hint: Option<&str>,
) -> Result<(), ExitCode> {
    if let Some(code) = reject_debug_cross_target(cli) {
        return Err(code);
    }
    if cli.memory == "gc" {
        return Err(toolchain::fail(&format!(
            "{platform} has no tracing GC runtime; use --memory=arc (the default)"
        )));
    }
    match host_hint {
        Some(hint) if cli.mode == "--run" => Err(toolchain::fail(&format!(
            "{platform} produces an app-logic library; use --compile and call it from {hint}"
        ))),
        _ => Ok(()),
    }
}

/// The native build kind this invocation asked for (`--debug` and `--profile`
/// are mutually exclusive; `parse_args` enforces that).
pub(super) fn build_kind(cli: &Cli) -> osprey_debug::BuildKind {
    if cli.debug && cli.debug_options.info == crate::debug_options::DebugInfo::None {
        osprey_debug::BuildKind::DebugWithoutInfo
    } else if cli.debug {
        osprey_debug::DebugBuild::ON.kind()
    } else if cli.profile {
        osprey_debug::BuildKind::Profile
    } else if std::env::var_os(TEST_COVERAGE_BUILD_ENV).is_some() {
        osprey_debug::BuildKind::Coverage
    } else {
        osprey_debug::DebugBuild::OFF.kind()
    }
}

/// `--compile`: build the executable, WASM module or iPhone C ABI archive.
pub(super) fn compile_program_to_disk(cli: &Cli, input: &CompilationInput) -> ExitCode {
    let out = input.output_path(cli.output.as_deref(), &cli.target);
    match build_artifact(cli, input, &out) {
        Ok(()) => {
            if !cli.quiet {
                println!("{}", out.display());
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn build_artifact(
    cli: &Cli,
    input: &CompilationInput,
    out: &std::path::Path,
) -> Result<(), ExitCode> {
    if cli.target == "wasm32" {
        if let Some(code) = reject_wasm_options(cli) {
            return Err(code);
        }
        return wasm::build_with_memory(
            input.debug_path(),
            input.backend_program(),
            out,
            &cli.memory,
        );
    }
    if cli.target != "native" {
        return build_mobile(cli, input, out);
    }
    build_input(input, out, NativeOptions::from_cli(cli))
}

fn build_mobile(
    cli: &Cli,
    input: &CompilationInput,
    out: &std::path::Path,
) -> Result<(), ExitCode> {
    let path = input.debug_path();
    let program = input.backend_program();
    if cli.target == "python" {
        return crate::python_target::build(cli, input, out);
    }
    let scope = input.export_scope(cli.exports.entry_only());
    if let Some(target) = android::Target::parse(&cli.target) {
        return android::build(path, program, out, target, &scope, &cli.memory);
    }
    if let Some(target) = ios::Target::parse(&cli.target) {
        ios::validate(cli, target)?;
        return ios::build(path, program, out, target, &scope, &cli.memory);
    }
    Err(toolchain::fail(&format!(
        "unknown mobile target: {}",
        cli.target
    )))
}

/// The output artifact path: the explicit `-o` value, else the source stem in
/// the current directory — with a `.wasm` extension for the wasm target.
#[cfg(test)]
pub(super) fn output_path(src: &str, output: Option<&str>, target: &str) -> PathBuf {
    match output {
        Some(o) => PathBuf::from(o),
        None if target == "wasm32" => PathBuf::from(format!("{}.wasm", stem_of(src))),
        None => PathBuf::from(stem_of(src)),
    }
}

/// Compile to a temp artifact and run it — the `--run` end-to-end path. Native
/// runs the executable directly; wasm runs it under a WASI host (`wasmtime`).
pub(super) fn run_program(cli: &Cli, input: &CompilationInput) -> ExitCode {
    match cli.target.as_str() {
        "python" => {
            toolchain::fail("Python libraries must run inside a Python host; use python -m osprey")
        }
        "wasm32" => match reject_wasm_options(cli) {
            Some(code) => code,
            None => wasm::run(input.debug_path(), input.backend_program(), &cli.memory),
        },
        "android-arm64" | "android-x64" => {
            toolchain::fail("Android libraries must run inside a host app; use --compile")
        }
        name => match ios::Target::parse(name) {
            Some(target) => ios::validate(cli, target).map_or_else(
                |code| code,
                |()| toolchain::fail("Apple app-logic libraries must run inside a host app"),
            ),
            None => run_native(cli, input),
        },
    }
}

fn run_native(cli: &Cli, input: &CompilationInput) -> ExitCode {
    let result = if cli.profile {
        execute_profiled(cli, input)
    } else {
        let output = (cli.debug && (cli.output.is_some() || cli.debug_options.keeps_artifacts()))
            .then(|| input.output_path(cli.output.as_deref(), "native"));
        execute_native(input, NativeOptions::from_cli(cli), output.as_deref())
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(code) => code,
    }
}
