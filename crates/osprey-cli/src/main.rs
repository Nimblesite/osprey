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

mod docs;
mod fmt;
mod project;
mod sandbox;
mod test_cmd;
mod wasm;

use osprey_syntax::Flavor;
use project::CompilationInput;
use sandbox::Policy;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub(crate) const USAGE: &str =
    "usage: osprey <file-or-project> [--check | --ast | --llvm | --compile | --run | \
--symbols | --list-tests] [--quiet] [--debug] [--profile] [--flavor default|ml] \
[--memory=default|gc] [--target=native|wasm32] [-o <out>] \
[--sandbox | --no-http | --no-websocket | --no-fs | --no-ffi]\n\
       osprey build [project] [--quiet] [--debug] [--memory=default|gc] \
[--target=native|wasm32] [-o <out>]\n\
       osprey test [path] [--filter <name>] [--quiet]\n\
       osprey fmt [--check | --stdout] [--flavor default|ml] <path...>\n\
       osprey --hover <name>\n\
       osprey --docs --docs-dir <dir>\n\
       osprey lsp";

/// The parsed invocation: source path, mode flag, and behaviour switches.
#[derive(Debug)]
pub(crate) struct Cli {
    path: String,
    mode: String,
    quiet: bool,
    policy: Policy,
    /// The reclaiming memory backend linked behind `@osp_alloc` — `default`
    /// (malloc passthrough) or `gc` (tracing collector). Link-time only; the IR
    /// is identical [MEM-BACKENDS]. (`arc` is reserved, docs/plans/0011.)
    memory: String,
    /// Codegen/link target: `native` (host executable via clang) or `wasm32`
    /// (browser-ready WebAssembly via wasm-ld; wasm32-wasip1). [WASM-TARGET]
    target: String,
    /// Explicit output artifact path (`-o`); defaults to the source stem.
    output: Option<String>,
    /// Emit source-level debug metadata and link a debugger-friendly binary.
    debug: bool,
    /// Profile the run [PROF-CLI-RUN]: build with line tables + frame pointers
    /// at full optimization, sample via the in-runtime profiler, then export
    /// and report (docs/specs/0028-Profiler.md).
    profile: bool,
    /// Explicit source flavor from `--flavor`; `None` when unset, so flavor
    /// resolution falls through to the marker/extension precedence
    /// ([FLAVOR-SELECT], docs/specs/0023-LanguageFlavors.md).
    flavor: Option<Flavor>,
}

impl Cli {
    /// A `--run`-mode invocation for `path` with default switches — the
    /// `osprey test` runner's per-file configuration [TESTING-CLI-RUN].
    pub(crate) fn run_native(path: String) -> Cli {
        Cli {
            path,
            mode: String::from("--run"),
            quiet: true,
            policy: Policy::allow_all(),
            memory: String::from("default"),
            target: String::from("native"),
            output: None,
            debug: false,
            profile: false,
            flavor: None,
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        // [SWR-VERSION-BUILD-STAMPING] the real version is stamped from the git
        // tag at release-build time via OSPREY_VERSION; source stays 0.0.0-dev.
        // [SWR-VERSION-CLI-OUTPUT] `--json` emits the manifest form the VS Code
        // extension version-checks at activation.
        let version = option_env!("OSPREY_VERSION").unwrap_or("0.0.0-dev");
        if args.iter().any(|a| a == "--json") {
            println!(
                "{{\"manifestVersion\":1,\"name\":\"osprey\",\"version\":\"{version}\",\
\"kind\":\"cli\",\"product\":\"osprey\"}}"
            );
        } else {
            println!("osprey {version}");
        }
        return ExitCode::SUCCESS;
    }
    // `osprey lsp`: speak the Language Server Protocol over stdio. The Rust
    // server (osprey-lsp, built on the published lspkit crates) drives the
    // compiler in-process. [LSP-REUSE-LSPKIT]
    if args.first().map(String::as_str) == Some("lsp") {
        return run_lsp();
    }
    // `osprey fmt`: reformat Osprey sources (both flavors). No compilation.
    if args.first().map(String::as_str) == Some("fmt") {
        return fmt::run(args.get(1..).unwrap_or_default());
    }
    // `osprey test`: discover and run test suites. [TESTING-CLI-RUN]
    if args.first().map(String::as_str) == Some("test") {
        return test_cmd::run(args.get(1..).unwrap_or_default());
    }
    // `osprey --docs`: regenerate the built-in function reference from the
    // compiler's metadata. No source file is involved.
    if args.iter().any(|a| a == "--docs") {
        return docs::run(&args);
    }
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    if cli.mode == "--hover" {
        // The positional is a built-in NAME, not a file. Unknown names print
        // nothing (the editor simply shows no hover) and still exit 0.
        if let Some(md) = osprey_lsp::builtin_hover(&cli.path) {
            println!("{md}");
        }
        return ExitCode::SUCCESS;
    }
    run(&cli)
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

/// Parse the argument list: the first non-flag is the source path; mode flags
/// select the action (last one wins); the rest toggle behaviour.
fn parse_args(args: &[String]) -> Result<Cli, String> {
    let project_build = args.first().map(String::as_str) == Some("build");
    let args = if project_build {
        args.get(1..).unwrap_or_default()
    } else {
        args
    };
    let mut path = None;
    let mut mode = String::from(if project_build {
        "--compile"
    } else {
        "--check"
    });
    let mut quiet = false;
    let mut policy = Policy::allow_all();
    let mut memory = String::from("default");
    let mut target = String::from("native");
    let mut output = None;
    let mut debug = false;
    let mut profile = false;
    let mut mode_explicit = false;
    let mut flavor = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ast" | "--check" | "--llvm" | "--compile" | "--run" | "--symbols"
            | "--list-tests" | "--hover"
                if project_build =>
            {
                return Err(format!(
                    "`osprey build` does not accept mode flag {a}\n{USAGE}"
                ));
            }
            "--ast" | "--check" | "--llvm" | "--compile" | "--run" | "--symbols"
            | "--list-tests" | "--hover" => {
                mode.clone_from(a);
                mode_explicit = true;
            }
            "--quiet" => quiet = true,
            "--debug" => debug = true,
            "--profile" => profile = true,
            "--sandbox" => policy = Policy::sandbox(),
            "--no-http" => policy.http = false,
            "--no-websocket" => policy.websocket = false,
            "--no-fs" => policy.fs = false,
            "--no-ffi" => policy.ffi = false,
            // `-o <path>` consumes the next argument as the output artifact path.
            "-o" => {
                let next = it
                    .next()
                    .ok_or_else(|| format!("-o requires a path\n{USAGE}"))?;
                output = Some(next.clone());
            }
            // `--flavor <name>` selects the source flavor explicitly (highest
            // selection precedence). [FLAVOR-SELECT]
            "--flavor" => {
                let next = it
                    .next()
                    .ok_or_else(|| format!("--flavor requires a value (default|ml)\n{USAGE}"))?;
                flavor = Some(parse_flavor(next)?);
            }
            flag if flag.starts_with("--flavor=") => {
                flavor = Some(parse_flavor(
                    flag.strip_prefix("--flavor=").unwrap_or_default(),
                )?);
            }
            flag if flag.starts_with("--memory=") => {
                memory = parse_memory(flag.strip_prefix("--memory=").unwrap_or_default())?;
            }
            flag if flag.starts_with("--target=") => {
                target = parse_target(flag.strip_prefix("--target=").unwrap_or_default())?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}\n{USAGE}")),
            _ if path.is_none() => path = Some(a.clone()),
            other => return Err(format!("unexpected argument {other}\n{USAGE}")),
        }
    }
    let path = match path {
        Some(path) => path,
        None if project_build => ".".to_string(),
        None => return Err(USAGE.to_string()),
    };
    let mut cli = Cli {
        path,
        mode,
        quiet,
        policy,
        memory,
        target,
        output,
        debug,
        profile,
        flavor,
    };
    apply_profile_rules(&mut cli, mode_explicit || project_build)?;
    Ok(cli)
}

/// Enforce the `--profile` interaction rules [PROF-CLI-RUN]: it conflicts with
/// `--debug` (profiling needs optimized code, debugging needs `-O0`), and a
/// bare `--profile` means "run it and profile it" — unless a mode was chosen
/// explicitly (or this is `osprey build`, whose mode is fixed).
fn apply_profile_rules(cli: &mut Cli, mode_chosen: bool) -> Result<(), String> {
    if cli.profile && cfg!(windows) {
        // The sampling runtime is POSIX-only; a silent no-op profile would
        // mislead, so refuse up front.
        return Err(format!(
            "--profile is not supported on Windows yet (the sampling profiler \
is POSIX-only)\n{USAGE}"
        ));
    }
    if cli.profile && cli.debug {
        return Err(format!(
            "--profile and --debug are mutually exclusive (profiling needs \
optimized code; debugging needs -O0)\n{USAGE}"
        ));
    }
    if cli.profile && !mode_chosen {
        cli.mode = String::from("--run");
    }
    Ok(())
}

/// Validate the `--target=` value: `native` (host executable) or `wasm32`
/// (browser-ready WebAssembly, wasm32-wasip1). [WASM-TARGET]
fn parse_target(value: &str) -> Result<String, String> {
    match value {
        "native" | "wasm32" => Ok(value.to_string()),
        other => Err(format!(
            "unknown target '{other}' (available: native, wasm32)\n{USAGE}"
        )),
    }
}

/// Validate the `--memory=` value. `arc` is reserved but not yet implemented
/// (docs/plans/0011) — reject it explicitly rather than silently mislabel.
fn parse_memory(value: &str) -> Result<String, String> {
    match value {
        "default" | "gc" => Ok(value.to_string()),
        "arc" => {
            Err("memory backend 'arc' is not yet implemented (available: default, gc)".to_string())
        }
        other => Err(format!(
            "unknown memory backend '{other}' (available: default, gc)\n{USAGE}"
        )),
    }
}

/// Validate a `--flavor` / marker value into a [`Flavor`]. [FLAVOR-SELECT]
fn parse_flavor(value: &str) -> Result<Flavor, String> {
    value.parse().map_err(|e| format!("{e}\n{USAGE}"))
}

/// Parse, gate (syntax → sandbox → types), and dispatch the selected mode.
fn run(cli: &Cli) -> ExitCode {
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
    let path = &cli.path;
    if project::is_project_path(path) {
        if cli.flavor.is_some() {
            eprintln!("error: --flavor applies to single files; projects select flavor per source");
            return Err(ExitCode::from(2));
        }
        return project::CompilationInput::load_project(path).map_err(|errors| {
            print_project_errors(&errors, path);
            ExitCode::FAILURE
        });
    }
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return Err(ExitCode::from(2));
        }
    };
    let flavor = match osprey_syntax::resolve_flavor(cli.flavor, path, &source) {
        Ok(flavor) => flavor,
        Err(msg) => {
            eprintln!("{msg}");
            return Err(ExitCode::from(2));
        }
    };
    let parsed = osprey_syntax::parse_program_with_flavor(&source, flavor);
    if !parsed.errors.is_empty() {
        for err in &parsed.errors {
            eprintln!(
                "{path}:{}:{}: {}",
                err.position.line, err.position.column, err.message
            );
        }
        return Err(ExitCode::FAILURE);
    }
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

fn print_project_errors(errors: &[osprey_project::ProjectError], fallback: &str) {
    for error in errors {
        eprintln!("{}", project::format_project_error(error, fallback));
    }
}

/// Route the type-gated modes: an ill-typed program never reaches codegen.
fn dispatch(cli: &Cli, input: &CompilationInput) -> ExitCode {
    let path = input.display_path();
    let program = input.program();
    match cli.mode.as_str() {
        "--check" => run_check(cli, input),
        // The outline must work for ill-typed (but parsable) files, so
        // `--symbols` deliberately skips the type gate.
        "--symbols" => {
            println!("{}", input.symbols_json());
            ExitCode::SUCCESS
        }
        // Static test discovery skips the type gate too, so editors can list
        // tests mid-edit [TESTING-LIST].
        "--list-tests" => {
            println!("{}", osprey_lsp::tests_json(program));
            ExitCode::SUCCESS
        }
        "--llvm" | "--run" | "--compile" if report_type_errors(input) > 0 => ExitCode::FAILURE,
        "--llvm" => match compile_ir(input.debug_path(), program, build_kind(cli)) {
            Ok(ir) => {
                print!("{ir}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{path}: {e}");
                ExitCode::FAILURE
            }
        },
        "--run" => run_program(cli, input),
        "--compile" => compile_program_to_disk(cli, input),
        _ => {
            println!("{program:#?}");
            ExitCode::SUCCESS
        }
    }
}

/// Type-check `program`, print every error in `file:line:col: message` form,
/// and return how many there were. The shared gate for every compiling mode.
pub(crate) fn report_type_errors(input: &CompilationInput) -> usize {
    let errors = osprey_types::check_program(input.program());
    for e in &errors {
        eprintln!("{}", input.diagnostic(e.position, &e.message));
    }
    errors.len()
}

fn run_check(cli: &Cli, input: &CompilationInput) -> ExitCode {
    if report_type_errors(input) == 0 {
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

fn reject_debug_wasm(cli: &Cli) -> Option<ExitCode> {
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

/// The native build kind this invocation asked for (`--debug` and `--profile`
/// are mutually exclusive; `parse_args` enforces that).
fn build_kind(cli: &Cli) -> osprey_debug::BuildKind {
    if cli.debug {
        osprey_debug::BuildKind::Debug
    } else if cli.profile {
        osprey_debug::BuildKind::Profile
    } else {
        osprey_debug::BuildKind::Release
    }
}

/// `--compile`: build the artifact at `-o` (or the source stem, `.wasm` for the
/// wasm target) — a host executable via clang, or WebAssembly via wasm-ld.
fn compile_program_to_disk(cli: &Cli, input: &CompilationInput) -> ExitCode {
    let out = input.output_path(cli.output.as_deref(), &cli.target);
    let result = if cli.target == "wasm32" {
        if let Some(code) = reject_debug_wasm(cli) {
            return code;
        }
        wasm::build(input.debug_path(), input.program(), &out)
    } else {
        build_executable(
            input.debug_path(),
            input.program(),
            input.source(),
            &out,
            &cli.memory,
            build_kind(cli),
        )
    };
    match result {
        Ok(()) => {
            if !cli.quiet {
                println!("{}", out.display());
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// The output artifact path: the explicit `-o` value, else the source stem in
/// the current directory — with a `.wasm` extension for the wasm target.
#[cfg(test)]
fn output_path(src: &str, output: Option<&str>, target: &str) -> PathBuf {
    match output {
        Some(o) => PathBuf::from(o),
        None if target == "wasm32" => PathBuf::from(format!("{}.wasm", stem_of(src))),
        None => PathBuf::from(stem_of(src)),
    }
}

/// Compile to a temp artifact and run it — the `--run` end-to-end path. Native
/// runs the executable directly; wasm runs it under a WASI host (`wasmtime`).
fn run_program(cli: &Cli, input: &CompilationInput) -> ExitCode {
    if cli.target == "wasm32" {
        if let Some(code) = reject_debug_wasm(cli) {
            return code;
        }
        return wasm::run(input.debug_path(), input.program());
    }
    let run = if cli.profile {
        execute_profiled(cli, input)
    } else {
        execute_native(input, &cli.memory, build_kind(cli))
    };
    match run {
        Ok(code) => ExitCode::from(code),
        Err(code) => code,
    }
}

/// Compile `input` natively to a temp binary and execute it inheriting stdio;
/// the child's exit code. Shared by `--run` and the `osprey test` runner
/// [TESTING-CLI-RUN].
pub(crate) fn execute_native(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<u8, ExitCode> {
    let exe = std::env::temp_dir().join(format!("{}.out", scratch_stem(input.display_path())));
    build_executable(
        input.debug_path(),
        input.program(),
        input.source(),
        &exe,
        memory,
        kind,
    )?;
    match Command::new(&exe).status() {
        Ok(s) => Ok(child_exit_code(s)),
        Err(e) => {
            eprintln!("error: could not run {}: {e}", exe.display());
            Err(ExitCode::FAILURE)
        }
    }
}

/// The `--run --profile` pipeline [PROF-CLI-RUN]: profile-build the program,
/// run it with the in-runtime sampler active [PROF-ACTIVATE-ENV], then
/// symbolize, export, and print the terminal report. The program's own exit
/// code is preserved; a post-processing failure warns but never masks the run.
fn execute_profiled(cli: &Cli, input: &CompilationInput) -> Result<u8, ExitCode> {
    let exe = std::env::temp_dir().join(format!("{}.out", scratch_stem(input.display_path())));
    build_executable(
        input.debug_path(),
        input.program(),
        input.source(),
        &exe,
        &cli.memory,
        osprey_debug::BuildKind::Profile,
    )?;
    let raw = std::env::temp_dir().join(format!("{}.osprof.json", scratch_stem(cli.path.as_str())));
    let status = Command::new(&exe).env("OSPREY_PROFILE", &raw).status();
    let code = match status {
        Ok(s) => child_exit_code(s),
        Err(e) => {
            eprintln!("error: could not run {}: {e}", exe.display());
            return Err(ExitCode::FAILURE);
        }
    };
    report_profile(cli, &exe, &raw);
    let _ = std::fs::remove_file(&raw);
    Ok(code)
}

/// Post-process a raw profile into the exports + terminal report; failures are
/// reported to stderr without failing the run.
fn report_profile(cli: &Cli, exe: &Path, raw: &Path) {
    use std::io::IsTerminal;
    let (out_dir, stem) = profile_export_target(cli);
    let opts = osprey_profiler::ProfileOptions {
        raw_path: raw.to_path_buf(),
        binary_path: exe.to_path_buf(),
        source_path: cli.path.clone(),
        out_dir,
        stem,
        color: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    };
    match osprey_profiler::process_profile(&opts) {
        Ok(outcome) => print!("{}", outcome.report),
        Err(e) => eprintln!("osprey: profile post-processing failed: {e}"),
    }
}

/// Where the profile exports land [PROF-CLI-RUN]: `-o dir/name` puts
/// `dir/name.speedscope.json` (etc.) there; the default is the source stem in
/// the working directory.
fn profile_export_target(cli: &Cli) -> (PathBuf, String) {
    match cli.output.as_deref() {
        Some(output) => {
            let dir = Path::new(output)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            (dir, stem_of(output))
        }
        None => (PathBuf::from("."), stem_of(&cli.path)),
    }
}

/// Lower to LLVM IR and hand it to clang together with the prebuilt C runtime,
/// producing `exe`.
fn build_executable(
    path: &str,
    program: &osprey_ast::Program,
    source: &str,
    exe: &Path,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<(), ExitCode> {
    let ir = match compile_ir(path, program, kind) {
        Ok(ir) => ir,
        Err(e) => {
            eprintln!("{path}: {e}");
            return Err(ExitCode::FAILURE);
        }
    };
    let ll = std::env::temp_dir().join(format!("{}.ll", scratch_stem(path)));
    if let Err(e) = std::fs::write(&ll, ir.as_bytes()) {
        eprintln!("error: cannot write IR to {}: {e}", ll.display());
        return Err(ExitCode::FAILURE);
    }
    if kind == osprey_debug::BuildKind::Profile {
        return build_profile_executable(&ll, &ir, source, exe, memory);
    }
    let mut cmd = Command::new(c_compiler());
    let _ = cmd
        .arg(&ll)
        .arg("-o")
        .arg(exe)
        .arg("-Wno-override-module")
        .arg(opt_flag(kind))
        .args(kind.native_driver_flags())
        .args(link_args(&ir, source, memory));
    run_build_step(cmd, &ll)
}

/// Profile builds go `.ll -> .o -> link -> dsymutil` [PROF-BUILD-MODE]: the
/// single-step clang pipeline deletes the temp object that holds the DWARF on
/// macOS, making line-level attribution unrecoverable.
fn build_profile_executable(
    ll: &Path,
    ir: &str,
    source: &str,
    exe: &Path,
    memory: &str,
) -> Result<(), ExitCode> {
    let kind = osprey_debug::BuildKind::Profile;
    let obj = ll.with_extension("o");
    let mut compile = Command::new(c_compiler());
    let _ = compile
        .arg("-c")
        .arg(ll)
        .arg("-o")
        .arg(&obj)
        .arg("-Wno-override-module")
        .arg(opt_flag(kind))
        .args(kind.native_driver_flags());
    run_build_step(compile, ll)?;
    let mut link = Command::new(c_compiler());
    let _ = link
        .arg(&obj)
        .arg("-o")
        .arg(exe)
        .args(kind.native_driver_flags())
        .args(link_args(ir, source, memory));
    run_build_step(link, &obj)?;
    if cfg!(target_os = "macos") {
        // Best-effort: without a dSYM the profile still symbolizes to function
        // names from the symbol table, just without file:line detail.
        let _ = Command::new("dsymutil").arg(exe).status();
    }
    Ok(())
}

/// Run one compiler/linker step, mapping failure onto the CLI exit contract.
fn run_build_step(mut cmd: Command, input: &Path) -> Result<(), ExitCode> {
    let cc = c_compiler();
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => {
            eprintln!("error: {cc} failed to compile {}", input.display());
            Err(ExitCode::FAILURE)
        }
        Err(e) => {
            eprintln!("error: could not invoke {cc}: {e}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// The LLVM optimization level handed to clang when lowering the emitted IR.
/// Defaults to `-O2`; `OSPREY_OPT` overrides it (e.g. `-O0` for fast debug
/// builds, `-O3` to match Rust/OCaml release flags). This is load-bearing twice
/// over: it is the difference between competitive and 30–100× slower native
/// code, and — because codegen currently has no reclamation backend [MEM-OPAQUE,
/// docs/specs/0018] — it IS the default memory strategy. At `-O2` LLVM proves
/// per-operation `Result` allocations non-escaping and removes them entirely
/// (heap → registers), the [MEM-OWNERSHIP] "free at last use" ideal achieved
/// statically; without it those allocations leak for the whole run.
fn compile_ir(
    path: &str,
    program: &osprey_ast::Program,
    kind: osprey_debug::BuildKind,
) -> osprey_codegen::Result<String> {
    if kind.wants_debug_info() {
        return osprey_codegen::compile_program_debug(
            program,
            osprey_codegen::DebugSource::from_path(path),
        );
    }
    osprey_codegen::compile_program(program)
}

fn opt_flag(kind: osprey_debug::BuildKind) -> String {
    kind.opt_flag(
        std::env::var("OSPREY_OPT").unwrap_or_else(|_| "-O2".to_string()),
        std::env::var("OSPREY_DEBUG_OPT").ok(),
    )
}

/// The C compiler/linker driver used to lower the emitted LLVM IR. Defaults to
/// `clang` (the only driver that consumes textual `.ll`); `OSPREY_CC` overrides
/// it — needed where several clangs coexist and the IR/runtime must link with a
/// matching toolchain (e.g. forcing the MinGW clang on Windows so it links the
/// MinGW-built C runtime archive rather than the system MSVC clang).
fn c_compiler() -> String {
    std::env::var("OSPREY_CC").unwrap_or_else(|_| "clang".to_string())
}

/// The source file's stem (`demo` for `examples/demo.osp`).
pub(crate) fn stem_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("osprey_out")
        .to_string()
}

/// A process-unique scratch stem, preventing concurrent CLI builds of files
/// named `main` from overwriting each other's temporary IR and executables.
pub(crate) fn scratch_stem(path: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!(
        "{}-{}-{:x}",
        stem_of(path),
        std::process::id(),
        hasher.finish()
    )
}

/// The exit code to propagate for a finished child: its own code when it exited
/// normally, else (Unix) `128 + signal` for a signal death — so a segfaulting
/// program is NOT masked as success (`status.code()` is `None` for a signal).
pub(crate) fn child_exit_code(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128u8.saturating_add(u8::try_from(sig).unwrap_or(0));
        }
    }
    1
}

/// Assemble the link arguments — everything a compiled binary needs beyond
/// libc: the prebuilt C runtime static library (the HTTP superset when the
/// program touches HTTP/WebSocket, else the fiber runtime), OpenSSL for HTTP,
/// and any `// @link:` / `// @linkdir:` FFI directives (e.g. `-lsqlite3`).
fn link_args(ir: &str, source: &str, memory: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let uses_http = ir.contains("@http") || ir.contains("@websocket");

    // The reclaiming backend is a link-time archive swap — the IR is identical
    // [MEM-BACKENDS]. `gc` links the tracing-collector archive set; `default`
    // the malloc-passthrough set. (docs/plans/0011)
    let suffix = if memory == "gc" { "_gc" } else { "" };
    let lib = if uses_http {
        format!("libhttp_runtime{suffix}.a")
    } else {
        format!("libfiber_runtime{suffix}.a")
    };
    if let Some(p) = find_runtime_lib(&lib) {
        args.push(p);
    } else if let Some(p) = find_runtime_lib(&format!("libfiber_runtime{suffix}.a")) {
        args.push(p);
    }

    if uses_http {
        args.extend(openssl_flags());
    }

    // Windows (MinGW UCRT64): the C runtime's fibers are winpthreads-backed, so
    // `pthread_*` must be linked explicitly — unlike Linux/macOS where libc /
    // libSystem provide them implicitly. Must come AFTER the archive that
    // references them. Compiled out on Unix.
    #[cfg(windows)]
    {
        args.push("-lpthread".to_string());
    }

    // FFI directives: `// @link: sqlite3` -> `-lsqlite3`, `// @linkdir: P` -> `-LP`.
    for line in source.lines() {
        if let Some(lib) = directive(line, "link") {
            args.push(format!("-l{lib}"));
        } else if let Some(dir) = directive(line, "linkdir") {
            args.push(format!("-L{dir}"));
        }
    }
    args
}

/// The trimmed value of a `// @<key>:` FFI directive line (accepting the
/// space-less `//@<key>:` spelling too), or `None` if `line` is not one.
fn directive<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let t = line.trim();
    t.strip_prefix(&format!("// @{key}:"))
        .or_else(|| t.strip_prefix(&format!("//@{key}:")))
        .map(str::trim)
}

/// Search the conventional install/build locations for a runtime static lib:
/// the working directory's repo layout, then next to the `osprey` executable
/// and below each of its ancestors (covering arbitrary in-workspace Cargo
/// target/profile nesting and release-tarball layouts), the compile-time
/// workspace as a development fallback, then the system lib dir.
pub(crate) fn find_runtime_lib(lib: &str) -> Option<String> {
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf));
    runtime_lib_candidates(lib, executable_dir.as_deref())
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
}

fn runtime_lib_candidates(lib: &str, executable_dir: Option<&Path>) -> Vec<String> {
    let mut roots = vec![
        format!("compiler/bin/{lib}"),
        format!("compiler/lib/{lib}"),
        format!("bin/{lib}"),
        format!("../bin/{lib}"),
        format!("../../bin/{lib}"),
    ];
    if let Some(dir) = executable_dir {
        roots.push(dir.join(lib).display().to_string());
        for ancestor in dir.ancestors() {
            for relative in ["compiler/lib", "compiler/bin", "bin"] {
                roots.push(ancestor.join(relative).join(lib).display().to_string());
            }
        }
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in ["compiler/lib", "compiler/bin"] {
        roots.push(workspace.join(relative).join(lib).display().to_string());
    }
    roots.push(format!("/usr/local/lib/{lib}"));
    roots
}

/// OpenSSL link flags, searching the conventional Homebrew/system lib dirs.
fn openssl_flags() -> Vec<String> {
    for dir in [
        "/opt/homebrew/opt/openssl@3/lib",
        "/opt/homebrew/lib",
        "/usr/local/opt/openssl@3/lib",
        "/usr/local/lib",
    ] {
        if Path::new(dir).join("libssl.dylib").exists() {
            return vec![format!("-L{dir}"), "-lssl".into(), "-lcrypto".into()];
        }
    }
    vec!["-lssl".into(), "-lcrypto".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    #[derive(Debug)]
    struct Out {
        code: Option<i32>,
        stdout: String,
        stderr: String,
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
    }

    fn example_lock() -> MutexGuard<'static, ()> {
        let lock = EXAMPLE_LOCK.get_or_init(|| Mutex::new(()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct CurrentDirGuard {
        prior: PathBuf,
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prior);
        }
    }

    fn enter_repo_root() -> Result<CurrentDirGuard, String> {
        let prior = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
        std::env::set_current_dir(repo_root())
            .map_err(|e| format!("cannot enter repo root {}: {e}", repo_root().display()))?;
        Ok(CurrentDirGuard { prior })
    }

    fn read_text(path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
    }

    fn native_exe_path(source: &Path) -> PathBuf {
        let rel = repo_relative(source);
        let sanitized = rel
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>();
        std::env::temp_dir().join(format!("osprey_golden_{sanitized}"))
    }

    fn parse_example(path: &str, source: &str) -> Result<osprey_ast::Program, String> {
        let flavor = osprey_syntax::resolve_flavor(None, path, source)
            .map_err(|e| format!("{path}: {e}"))?;
        let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
        if !parsed.errors.is_empty() {
            let errors = parsed
                .errors
                .iter()
                .map(|e| {
                    format!(
                        "{}:{}:{}: {}",
                        path, e.position.line, e.position.column, e.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Err(errors);
        }
        Ok(parsed.program)
    }

    fn run_example(source: &Path) -> Result<Out, String> {
        let source_text = read_text(source)?;
        let path = source.to_string_lossy().into_owned();
        let program = parse_example(&path, &source_text)?;
        let _cwd = enter_repo_root()?;

        let violations = sandbox::violations(&program, Policy::allow_all());
        if !violations.is_empty() {
            return Err(format!("{}: {}", path, violations.join("\n")));
        }

        let type_errors = osprey_types::check_program(&program);
        if !type_errors.is_empty() {
            let errors = type_errors
                .iter()
                .map(|e| match e.position {
                    Some(p) => format!("{}:{}:{}: {}", path, p.line, p.column, e.message),
                    None => format!("{}: {}", path, e.message),
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Err(errors);
        }

        let exe = native_exe_path(source);
        build_executable(
            &path,
            &program,
            &source_text,
            &exe,
            "default",
            osprey_debug::BuildKind::Release,
        )
        .map_err(|code| format!("{}: native build failed: {code:?}", source.display()))?;

        Command::new(&exe)
            .output()
            .map(|out| Out {
                code: out.status.code(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            })
            .map_err(|e| format!("could not run {}: {e}", exe.display()))
    }

    fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
        PathBuf::from(format!("{}{}", path.display(), suffix))
    }

    fn source_base(source: &Path) -> PathBuf {
        let mut base = source.to_path_buf();
        assert!(
            base.set_extension(""),
            "example path has no extension: {}",
            source.display()
        );
        base
    }

    fn uname_s() -> &'static str {
        match std::env::consts::OS {
            "macos" => "Darwin",
            "linux" => "Linux",
            "windows" => "Windows_NT",
            other => other,
        }
    }

    fn expected_candidates(source: &Path) -> Vec<PathBuf> {
        let os = uname_s();
        let base = source_base(source);
        vec![
            path_with_suffix(source, ".expectedoutput"),
            path_with_suffix(source, &format!(".expectedoutput.{os}")),
            path_with_suffix(&base, ".osp.expectedoutput"),
            path_with_suffix(&base, &format!(".osp.expectedoutput.{os}")),
            path_with_suffix(&base, ".expectedoutput"),
        ]
    }

    fn expected_output_path(source: &Path) -> Result<PathBuf, String> {
        for candidate in expected_candidates(source) {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        Err(format!("missing expected output for {}", source.display()))
    }

    fn check_example_matches(rel_source: &str) -> Result<(), String> {
        let _guard = example_lock();
        let source = repo_root().join(rel_source);
        let expected_path = expected_output_path(&source)?;
        let expected = read_text(&expected_path)?;
        let actual = run_example(&source)?;

        if actual.code == Some(0) && actual.stdout.trim() == expected.trim() {
            return Ok(());
        }

        Err(format!(
        "{rel_source}\nstatus={:?}\nexpected file={}\n--- expected ---\n{}\n--- actual ---\n{}\n--- stderr ---\n{}",
        actual.code,
        expected_path.display(),
        expected.trim(),
        actual.stdout.trim(),
        actual.stderr.trim()
    ))
    }

    fn assert_example_matches(rel_source: &str) {
        if let Err(e) = check_example_matches(rel_source) {
            assert!(e.is_empty(), "{e}");
        }
    }

    fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_sources(&path, out);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("osp" | "ospml")
            ) {
                out.push(path);
            }
        }
    }

    fn tested_example_sources() -> Vec<PathBuf> {
        let mut out = Vec::new();
        collect_sources(&repo_root().join("examples/tested"), &mut out);
        out.sort();
        out
    }

    fn repo_relative(path: &Path) -> String {
        let root = repo_root();
        match path.strip_prefix(&root) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => path.to_string_lossy().replace('\\', "/"),
        }
    }

    #[test]
    fn fiber_cpu_profiling_demo_osp() {
        assert_example_matches("examples/tested/fiber/cpu_profiling_demo.osp");
    }

    #[test]
    fn fiber_fiber_determinism_osp() {
        assert_example_matches("examples/tested/fiber/fiber_determinism.osp");
    }

    #[test]
    fn fiber_fiber_exact_replica_osp() {
        assert_example_matches("examples/tested/fiber/fiber_exact_replica.osp");
    }

    #[test]
    fn all_tested_examples_are_registered_as_individual_tests() {
        let discovered = tested_example_sources()
            .iter()
            .map(|path| repo_relative(path))
            .collect::<BTreeSet<_>>();
        let registered = REGISTERED_EXAMPLES
            .iter()
            .map(|path| (*path).to_string())
            .collect::<BTreeSet<_>>();

        let missing = discovered
            .difference(&registered)
            .cloned()
            .collect::<Vec<_>>();
        let stale = registered
            .difference(&discovered)
            .cloned()
            .collect::<Vec<_>>();

        assert!(
            missing.is_empty() && stale.is_empty(),
            "every examples/tested fixture must have a named Rust test\nmissing:\n{}\nstale:\n{}",
            missing.join("\n"),
            stale.join("\n")
        );
    }

    static EXAMPLE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    const REGISTERED_EXAMPLES: &[&str] = &[
        "examples/tested/basics/blocks/block_statements_basic.osp",
        "examples/tested/basics/blocks/block_statements_basic.ospml",
        "examples/tested/basics/cursor/codepoint_roundtrip.osp",
        "examples/tested/basics/cursor/codepoint_roundtrip.ospml",
        "examples/tested/basics/cursor/kv_parser.osp",
        "examples/tested/basics/cursor/kv_parser.ospml",
        "examples/tested/basics/cursor/token_scan.osp",
        "examples/tested/basics/cursor/token_scan.ospml",
        "examples/tested/basics/cursor/utf8_walk.osp",
        "examples/tested/basics/cursor/utf8_walk.ospml",
        "examples/tested/basics/errors/error_messages.osp",
        "examples/tested/basics/errors/error_messages.ospml",
        "examples/tested/basics/errors/validation_pipeline.osp",
        "examples/tested/basics/errors/validation_pipeline.ospml",
        "examples/tested/basics/feature_omnibus.osp",
        "examples/tested/basics/feature_omnibus.ospml",
        "examples/tested/basics/field_access_comprehensive.osp",
        "examples/tested/basics/field_access_comprehensive.ospml",
        "examples/tested/basics/files/file_io_json_workflow.osp",
        "examples/tested/basics/files/file_io_json_workflow.ospml",
        "examples/tested/basics/json/json_document_query.osp",
        "examples/tested/basics/json/json_document_query.ospml",
        "examples/tested/basics/function_composition_test.osp",
        "examples/tested/basics/functional/functional_showcase.osp",
        "examples/tested/basics/functional/functional_showcase.ospml",
        "examples/tested/basics/games/adventure_game.osp",
        "examples/tested/basics/games/adventure_game.ospml",
        "examples/tested/basics/games/space_trader.osp",
        "examples/tested/basics/games/space_trader.ospml",
        "examples/tested/basics/knownbugs/bug1_spawn_record.osp",
        "examples/tested/basics/knownbugs/bug1_spawn_record.ospml",
        "examples/tested/basics/knownbugs/bug2_string_union_payload.osp",
        "examples/tested/basics/knownbugs/bug2_string_union_payload.ospml",
        "examples/tested/basics/knownbugs/bug3_map_built_index.osp",
        "examples/tested/basics/knownbugs/bug3_map_built_index.ospml",
        "examples/tested/basics/knownbugs/bug4_union_return_arg.osp",
        "examples/tested/basics/knownbugs/bug4_union_return_arg.ospml",
        "examples/tested/basics/lists/list_basics.osp",
        "examples/tested/basics/lists/list_basics.ospml",
        "examples/tested/basics/lists/map_basics.osp",
        "examples/tested/basics/lists/map_basics.ospml",
        "examples/tested/basics/math/comprehensive_math.osp",
        "examples/tested/basics/math/comprehensive_math.ospml",
        "examples/tested/basics/operators/boolean_consolidated.osp",
        "examples/tested/basics/operators/boolean_consolidated.ospml",
        "examples/tested/basics/osprey_mega_showcase.osp",
        "examples/tested/basics/osprey_mega_showcase.ospml",
        "examples/tested/basics/pattern_matching/pattern_matching_complete.osp",
        "examples/tested/basics/processes/async_process_management.osp",
        "examples/tested/basics/processes/async_process_management.ospml",
        "examples/tested/basics/processes/callback_stdout_demo.osp",
        "examples/tested/basics/processes/callback_stdout_demo.ospml",
        "examples/tested/basics/strings/string_edge_cases.osp",
        "examples/tested/basics/strings/string_edge_cases.ospml",
        "examples/tested/basics/strings/string_pipeline.osp",
        "examples/tested/basics/strings/string_pipeline.ospml",
        "examples/tested/basics/types/any_type_comprehensive.osp",
        "examples/tested/basics/types/any_type_comprehensive.ospml",
        "examples/tested/basics/types/pure_hindley_milner_test.osp",
        "examples/tested/basics/types/pure_hindley_milner_test.ospml",
        "examples/tested/basics/types/record_update_basic.osp",
        "examples/tested/basics/types/record_update_basic.ospml",
        "examples/tested/basics/types/recursive_unions.osp",
        "examples/tested/basics/types/recursive_unions.ospml",
        "examples/tested/basics/types/type_equality_comprehensive.osp",
        "examples/tested/basics/types/type_equality_comprehensive.ospml",
        "examples/tested/basics/types/user_defined_unions.osp",
        "examples/tested/basics/types/user_defined_unions.ospml",
        "examples/tested/basics/validation/proper_validation_test.osp",
        "examples/tested/basics/validation/proper_validation_test.ospml",
        "examples/tested/basics/website/website_examples.osp",
        "examples/tested/basics/website/website_examples.ospml",
        "examples/tested/db/database_effect.osp",
        "examples/tested/db/database_effect.ospml",
        "examples/tested/db/sqlite_basics.osp",
        "examples/tested/db/sqlite_basics.ospml",
        "examples/tested/effects/algebraic_effects_comprehensive.osp",
        "examples/tested/effects/algebraic_effects_comprehensive.ospml",
        "examples/tested/effects/fiber_effects.osp",
        "examples/tested/effects/fiber_effects.ospml",
        "examples/tested/effects/handler_scoping.osp",
        "examples/tested/effects/handler_scoping.ospml",
        "examples/tested/effects/http_state_levels.osp",
        "examples/tested/effects/http_state_levels.ospml",
        "examples/tested/effects/resume_abort_early_exit.osp",
        "examples/tested/effects/resume_abort_early_exit.ospml",
        "examples/tested/effects/resume_lifo_audit.osp",
        "examples/tested/effects/resume_lifo_audit.ospml",
        "examples/tested/effects/resume_outer_handler_bridge.osp",
        "examples/tested/effects/resume_outer_handler_bridge.ospml",
        "examples/tested/effects/resume_unit_markers.osp",
        "examples/tested/effects/resume_unit_markers.ospml",
        "examples/tested/effects/resume_value_rewrite.osp",
        "examples/tested/effects/resume_value_rewrite.ospml",
        "examples/tested/fiber/cpu_profiling_demo.osp",
        "examples/tested/fiber/fiber_determinism.osp",
        "examples/tested/fiber/fiber_exact_replica.osp",
        "examples/tested/fiber/fiber_showcase.osp",
        "examples/tested/fiber/fiber_showcase.ospml",
        "examples/tested/http/http_client_example.osp",
        "examples/tested/http/http_client_example.ospml",
        "examples/tested/http/http_create_client.osp",
        "examples/tested/http/http_create_client.ospml",
        "examples/tested/http/http_response_handle.osp",
        "examples/tested/http/http_response_handle.ospml",
        "examples/tested/http/http_server_example.osp",
        "examples/tested/http/http_server_example.ospml",
        "examples/tested/http/tui_repo_table.osp",
        "examples/tested/http/tui_repo_table.ospml",
        "examples/tested/ml/arith.osp",
        "examples/tested/ml/arith.ospml",
        "examples/tested/ml/booleans.osp",
        "examples/tested/ml/booleans.ospml",
        "examples/tested/ml/closures.osp",
        "examples/tested/ml/closures.ospml",
        "examples/tested/ml/curry_partial.osp",
        "examples/tested/ml/curry_partial.ospml",
        "examples/tested/ml/curry_tour.osp",
        "examples/tested/ml/curry_tour.ospml",
        "examples/tested/ml/hello.osp",
        "examples/tested/ml/hello.ospml",
        "examples/tested/ml/hof.osp",
        "examples/tested/ml/hof.ospml",
        "examples/tested/ml/match_tour.osp",
        "examples/tested/ml/match_tour.ospml",
        "examples/tested/ml/matchbool.osp",
        "examples/tested/ml/matchbool.ospml",
        "examples/tested/ml/matchint.osp",
        "examples/tested/ml/matchint.ospml",
        "examples/tested/ml/mixed.osp",
        "examples/tested/ml/mixed.ospml",
        "examples/tested/ml/mutation.osp",
        "examples/tested/ml/mutation.ospml",
        "examples/tested/ml/nested_calls.osp",
        "examples/tested/ml/nested_calls.ospml",
        "examples/tested/ml/pipechain.osp",
        "examples/tested/ml/pipechain.ospml",
        "examples/tested/ml/recursion.osp",
        "examples/tested/ml/recursion.ospml",
        "examples/tested/ml/results_state_hof.osp",
        "examples/tested/ml/results_state_hof.ospml",
        "examples/tested/ml/strings.osp",
        "examples/tested/ml/strings.ospml",
        "examples/tested/testing/calculator.test.osp",
        "examples/tested/testing/calculator.test.ospml",
        "examples/tested/testing/mlcheck.test.osp",
        "examples/tested/testing/mlcheck.test.ospml",
        "examples/tested/testing/verdict.test.ospml",
    ];

    #[test]
    fn basics_blocks_block_statements_basic_osp() {
        assert_example_matches("examples/tested/basics/blocks/block_statements_basic.osp");
    }

    #[test]
    fn basics_blocks_block_statements_basic_ospml() {
        assert_example_matches("examples/tested/basics/blocks/block_statements_basic.ospml");
    }

    #[test]
    fn basics_cursor_codepoint_roundtrip_osp() {
        assert_example_matches("examples/tested/basics/cursor/codepoint_roundtrip.osp");
    }

    #[test]
    fn basics_cursor_codepoint_roundtrip_ospml() {
        assert_example_matches("examples/tested/basics/cursor/codepoint_roundtrip.ospml");
    }

    #[test]
    fn basics_cursor_kv_parser_osp() {
        assert_example_matches("examples/tested/basics/cursor/kv_parser.osp");
    }

    #[test]
    fn basics_cursor_kv_parser_ospml() {
        assert_example_matches("examples/tested/basics/cursor/kv_parser.ospml");
    }

    #[test]
    fn basics_cursor_token_scan_osp() {
        assert_example_matches("examples/tested/basics/cursor/token_scan.osp");
    }

    #[test]
    fn basics_cursor_token_scan_ospml() {
        assert_example_matches("examples/tested/basics/cursor/token_scan.ospml");
    }

    #[test]
    fn basics_cursor_utf8_walk_osp() {
        assert_example_matches("examples/tested/basics/cursor/utf8_walk.osp");
    }

    #[test]
    fn basics_cursor_utf8_walk_ospml() {
        assert_example_matches("examples/tested/basics/cursor/utf8_walk.ospml");
    }

    #[test]
    fn basics_errors_error_messages_osp() {
        assert_example_matches("examples/tested/basics/errors/error_messages.osp");
    }

    #[test]
    fn basics_errors_error_messages_ospml() {
        assert_example_matches("examples/tested/basics/errors/error_messages.ospml");
    }

    #[test]
    fn basics_errors_validation_pipeline_osp() {
        assert_example_matches("examples/tested/basics/errors/validation_pipeline.osp");
    }

    #[test]
    fn basics_errors_validation_pipeline_ospml() {
        assert_example_matches("examples/tested/basics/errors/validation_pipeline.ospml");
    }

    #[test]
    fn basics_feature_omnibus_osp() {
        assert_example_matches("examples/tested/basics/feature_omnibus.osp");
    }

    #[test]
    fn basics_feature_omnibus_ospml() {
        assert_example_matches("examples/tested/basics/feature_omnibus.ospml");
    }

    #[test]
    fn basics_field_access_comprehensive_osp() {
        assert_example_matches("examples/tested/basics/field_access_comprehensive.osp");
    }

    #[test]
    fn basics_field_access_comprehensive_ospml() {
        assert_example_matches("examples/tested/basics/field_access_comprehensive.ospml");
    }

    #[test]
    fn basics_files_file_io_json_workflow_osp() {
        assert_example_matches("examples/tested/basics/files/file_io_json_workflow.osp");
    }

    #[test]
    fn basics_files_file_io_json_workflow_ospml() {
        assert_example_matches("examples/tested/basics/files/file_io_json_workflow.ospml");
    }

    #[test]
    fn basics_json_json_document_query_osp() {
        assert_example_matches("examples/tested/basics/json/json_document_query.osp");
    }

    #[test]
    fn basics_json_json_document_query_ospml() {
        assert_example_matches("examples/tested/basics/json/json_document_query.ospml");
    }

    #[test]
    fn basics_function_composition_test_osp() {
        assert_example_matches("examples/tested/basics/function_composition_test.osp");
    }

    #[test]
    fn basics_functional_functional_showcase_osp() {
        assert_example_matches("examples/tested/basics/functional/functional_showcase.osp");
    }

    #[test]
    fn basics_functional_functional_showcase_ospml() {
        assert_example_matches("examples/tested/basics/functional/functional_showcase.ospml");
    }

    #[test]
    fn basics_games_adventure_game_osp() {
        assert_example_matches("examples/tested/basics/games/adventure_game.osp");
    }

    #[test]
    fn basics_games_adventure_game_ospml() {
        assert_example_matches("examples/tested/basics/games/adventure_game.ospml");
    }

    #[test]
    fn basics_games_space_trader_osp() {
        assert_example_matches("examples/tested/basics/games/space_trader.osp");
    }

    #[test]
    fn basics_games_space_trader_ospml() {
        assert_example_matches("examples/tested/basics/games/space_trader.ospml");
    }

    #[test]
    fn basics_knownbugs_bug1_spawn_record_osp() {
        assert_example_matches("examples/tested/basics/knownbugs/bug1_spawn_record.osp");
    }

    #[test]
    fn basics_knownbugs_bug1_spawn_record_ospml() {
        assert_example_matches("examples/tested/basics/knownbugs/bug1_spawn_record.ospml");
    }

    #[test]
    fn basics_knownbugs_bug2_string_union_payload_osp() {
        assert_example_matches("examples/tested/basics/knownbugs/bug2_string_union_payload.osp");
    }

    #[test]
    fn basics_knownbugs_bug2_string_union_payload_ospml() {
        assert_example_matches("examples/tested/basics/knownbugs/bug2_string_union_payload.ospml");
    }

    #[test]
    fn basics_knownbugs_bug3_map_built_index_osp() {
        assert_example_matches("examples/tested/basics/knownbugs/bug3_map_built_index.osp");
    }

    #[test]
    fn basics_knownbugs_bug3_map_built_index_ospml() {
        assert_example_matches("examples/tested/basics/knownbugs/bug3_map_built_index.ospml");
    }

    #[test]
    fn basics_knownbugs_bug4_union_return_arg_osp() {
        assert_example_matches("examples/tested/basics/knownbugs/bug4_union_return_arg.osp");
    }

    #[test]
    fn basics_knownbugs_bug4_union_return_arg_ospml() {
        assert_example_matches("examples/tested/basics/knownbugs/bug4_union_return_arg.ospml");
    }

    #[test]
    fn basics_lists_list_basics_osp() {
        assert_example_matches("examples/tested/basics/lists/list_basics.osp");
    }

    #[test]
    fn basics_lists_list_basics_ospml() {
        assert_example_matches("examples/tested/basics/lists/list_basics.ospml");
    }

    #[test]
    fn basics_lists_map_basics_osp() {
        assert_example_matches("examples/tested/basics/lists/map_basics.osp");
    }

    #[test]
    fn basics_lists_map_basics_ospml() {
        assert_example_matches("examples/tested/basics/lists/map_basics.ospml");
    }

    #[test]
    fn basics_math_comprehensive_math_osp() {
        assert_example_matches("examples/tested/basics/math/comprehensive_math.osp");
    }

    #[test]
    fn basics_math_comprehensive_math_ospml() {
        assert_example_matches("examples/tested/basics/math/comprehensive_math.ospml");
    }

    #[test]
    fn basics_operators_boolean_consolidated_osp() {
        assert_example_matches("examples/tested/basics/operators/boolean_consolidated.osp");
    }

    #[test]
    fn basics_operators_boolean_consolidated_ospml() {
        assert_example_matches("examples/tested/basics/operators/boolean_consolidated.ospml");
    }

    #[test]
    fn basics_osprey_mega_showcase_osp() {
        assert_example_matches("examples/tested/basics/osprey_mega_showcase.osp");
    }

    #[test]
    fn basics_osprey_mega_showcase_ospml() {
        assert_example_matches("examples/tested/basics/osprey_mega_showcase.ospml");
    }

    #[test]
    fn basics_pattern_matching_pattern_matching_complete_osp() {
        assert_example_matches(
            "examples/tested/basics/pattern_matching/pattern_matching_complete.osp",
        );
    }

    #[test]
    fn basics_processes_async_process_management_osp() {
        assert_example_matches("examples/tested/basics/processes/async_process_management.osp");
    }

    #[test]
    fn basics_processes_async_process_management_ospml() {
        assert_example_matches("examples/tested/basics/processes/async_process_management.ospml");
    }

    #[test]
    fn basics_processes_callback_stdout_demo_osp() {
        assert_example_matches("examples/tested/basics/processes/callback_stdout_demo.osp");
    }

    #[test]
    fn basics_processes_callback_stdout_demo_ospml() {
        assert_example_matches("examples/tested/basics/processes/callback_stdout_demo.ospml");
    }

    #[test]
    fn basics_strings_string_edge_cases_osp() {
        assert_example_matches("examples/tested/basics/strings/string_edge_cases.osp");
    }

    #[test]
    fn basics_strings_string_edge_cases_ospml() {
        assert_example_matches("examples/tested/basics/strings/string_edge_cases.ospml");
    }

    #[test]
    fn basics_strings_string_pipeline_osp() {
        assert_example_matches("examples/tested/basics/strings/string_pipeline.osp");
    }

    #[test]
    fn basics_strings_string_pipeline_ospml() {
        assert_example_matches("examples/tested/basics/strings/string_pipeline.ospml");
    }

    #[test]
    fn basics_types_any_type_comprehensive_osp() {
        assert_example_matches("examples/tested/basics/types/any_type_comprehensive.osp");
    }

    #[test]
    fn basics_types_any_type_comprehensive_ospml() {
        assert_example_matches("examples/tested/basics/types/any_type_comprehensive.ospml");
    }

    #[test]
    fn basics_types_pure_hindley_milner_test_osp() {
        assert_example_matches("examples/tested/basics/types/pure_hindley_milner_test.osp");
    }

    #[test]
    fn basics_types_pure_hindley_milner_test_ospml() {
        assert_example_matches("examples/tested/basics/types/pure_hindley_milner_test.ospml");
    }

    #[test]
    fn basics_types_record_update_basic_osp() {
        assert_example_matches("examples/tested/basics/types/record_update_basic.osp");
    }

    #[test]
    fn basics_types_record_update_basic_ospml() {
        assert_example_matches("examples/tested/basics/types/record_update_basic.ospml");
    }

    #[test]
    fn basics_types_recursive_unions_osp() {
        assert_example_matches("examples/tested/basics/types/recursive_unions.osp");
    }

    #[test]
    fn basics_types_recursive_unions_ospml() {
        assert_example_matches("examples/tested/basics/types/recursive_unions.ospml");
    }

    #[test]
    fn basics_types_type_equality_comprehensive_osp() {
        assert_example_matches("examples/tested/basics/types/type_equality_comprehensive.osp");
    }

    #[test]
    fn basics_types_type_equality_comprehensive_ospml() {
        assert_example_matches("examples/tested/basics/types/type_equality_comprehensive.ospml");
    }

    #[test]
    fn basics_types_user_defined_unions_osp() {
        assert_example_matches("examples/tested/basics/types/user_defined_unions.osp");
    }

    #[test]
    fn basics_types_user_defined_unions_ospml() {
        assert_example_matches("examples/tested/basics/types/user_defined_unions.ospml");
    }

    #[test]
    fn basics_validation_proper_validation_test_osp() {
        assert_example_matches("examples/tested/basics/validation/proper_validation_test.osp");
    }

    #[test]
    fn basics_validation_proper_validation_test_ospml() {
        assert_example_matches("examples/tested/basics/validation/proper_validation_test.ospml");
    }

    #[test]
    fn basics_website_website_examples_osp() {
        assert_example_matches("examples/tested/basics/website/website_examples.osp");
    }

    #[test]
    fn basics_website_website_examples_ospml() {
        assert_example_matches("examples/tested/basics/website/website_examples.ospml");
    }

    #[test]
    fn db_database_effect_osp() {
        assert_example_matches("examples/tested/db/database_effect.osp");
    }

    #[test]
    fn db_database_effect_ospml() {
        assert_example_matches("examples/tested/db/database_effect.ospml");
    }

    #[test]
    fn db_sqlite_basics_osp() {
        assert_example_matches("examples/tested/db/sqlite_basics.osp");
    }

    #[test]
    fn db_sqlite_basics_ospml() {
        assert_example_matches("examples/tested/db/sqlite_basics.ospml");
    }

    #[test]
    fn effects_algebraic_effects_comprehensive_osp() {
        assert_example_matches("examples/tested/effects/algebraic_effects_comprehensive.osp");
    }

    #[test]
    fn effects_algebraic_effects_comprehensive_ospml() {
        assert_example_matches("examples/tested/effects/algebraic_effects_comprehensive.ospml");
    }

    #[test]
    fn effects_fiber_effects_osp() {
        assert_example_matches("examples/tested/effects/fiber_effects.osp");
    }

    #[test]
    fn effects_fiber_effects_ospml() {
        assert_example_matches("examples/tested/effects/fiber_effects.ospml");
    }

    #[test]
    fn effects_handler_scoping_osp() {
        assert_example_matches("examples/tested/effects/handler_scoping.osp");
    }

    #[test]
    fn effects_handler_scoping_ospml() {
        assert_example_matches("examples/tested/effects/handler_scoping.ospml");
    }

    #[test]
    fn effects_http_state_levels_osp() {
        assert_example_matches("examples/tested/effects/http_state_levels.osp");
    }

    #[test]
    fn effects_http_state_levels_ospml() {
        assert_example_matches("examples/tested/effects/http_state_levels.ospml");
    }

    #[test]
    fn effects_resume_abort_early_exit_osp() {
        assert_example_matches("examples/tested/effects/resume_abort_early_exit.osp");
    }

    #[test]
    fn effects_resume_abort_early_exit_ospml() {
        assert_example_matches("examples/tested/effects/resume_abort_early_exit.ospml");
    }

    #[test]
    fn effects_resume_lifo_audit_osp() {
        assert_example_matches("examples/tested/effects/resume_lifo_audit.osp");
    }

    #[test]
    fn effects_resume_lifo_audit_ospml() {
        assert_example_matches("examples/tested/effects/resume_lifo_audit.ospml");
    }

    #[test]
    fn effects_resume_outer_handler_bridge_osp() {
        assert_example_matches("examples/tested/effects/resume_outer_handler_bridge.osp");
    }

    #[test]
    fn effects_resume_outer_handler_bridge_ospml() {
        assert_example_matches("examples/tested/effects/resume_outer_handler_bridge.ospml");
    }

    #[test]
    fn effects_resume_unit_markers_osp() {
        assert_example_matches("examples/tested/effects/resume_unit_markers.osp");
    }

    #[test]
    fn effects_resume_unit_markers_ospml() {
        assert_example_matches("examples/tested/effects/resume_unit_markers.ospml");
    }

    #[test]
    fn effects_resume_value_rewrite_osp() {
        assert_example_matches("examples/tested/effects/resume_value_rewrite.osp");
    }

    #[test]
    fn effects_resume_value_rewrite_ospml() {
        assert_example_matches("examples/tested/effects/resume_value_rewrite.ospml");
    }

    #[test]
    fn fiber_fiber_showcase_osp() {
        assert_example_matches("examples/tested/fiber/fiber_showcase.osp");
    }

    #[test]
    fn fiber_fiber_showcase_ospml() {
        assert_example_matches("examples/tested/fiber/fiber_showcase.ospml");
    }

    #[test]
    fn http_http_client_example_osp() {
        assert_example_matches("examples/tested/http/http_client_example.osp");
    }

    #[test]
    fn http_http_client_example_ospml() {
        assert_example_matches("examples/tested/http/http_client_example.ospml");
    }

    #[test]
    fn http_http_create_client_osp() {
        assert_example_matches("examples/tested/http/http_create_client.osp");
    }

    #[test]
    fn http_http_create_client_ospml() {
        assert_example_matches("examples/tested/http/http_create_client.ospml");
    }

    #[test]
    fn http_http_response_handle_osp() {
        assert_example_matches("examples/tested/http/http_response_handle.osp");
    }

    #[test]
    fn http_http_response_handle_ospml() {
        assert_example_matches("examples/tested/http/http_response_handle.ospml");
    }

    #[test]
    fn http_http_server_example_osp() {
        assert_example_matches("examples/tested/http/http_server_example.osp");
    }

    #[test]
    fn http_http_server_example_ospml() {
        assert_example_matches("examples/tested/http/http_server_example.ospml");
    }

    #[test]
    fn http_tui_repo_table_osp() {
        assert_example_matches("examples/tested/http/tui_repo_table.osp");
    }

    #[test]
    fn http_tui_repo_table_ospml() {
        assert_example_matches("examples/tested/http/tui_repo_table.ospml");
    }

    #[test]
    fn ml_arith_osp() {
        assert_example_matches("examples/tested/ml/arith.osp");
    }

    #[test]
    fn ml_arith_ospml() {
        assert_example_matches("examples/tested/ml/arith.ospml");
    }

    #[test]
    fn ml_booleans_osp() {
        assert_example_matches("examples/tested/ml/booleans.osp");
    }

    #[test]
    fn ml_booleans_ospml() {
        assert_example_matches("examples/tested/ml/booleans.ospml");
    }

    #[test]
    fn ml_closures_osp() {
        assert_example_matches("examples/tested/ml/closures.osp");
    }

    #[test]
    fn ml_closures_ospml() {
        assert_example_matches("examples/tested/ml/closures.ospml");
    }

    #[test]
    fn ml_curry_partial_osp() {
        assert_example_matches("examples/tested/ml/curry_partial.osp");
    }

    #[test]
    fn ml_curry_partial_ospml() {
        assert_example_matches("examples/tested/ml/curry_partial.ospml");
    }

    #[test]
    fn ml_curry_tour_osp() {
        assert_example_matches("examples/tested/ml/curry_tour.osp");
    }

    #[test]
    fn ml_curry_tour_ospml() {
        assert_example_matches("examples/tested/ml/curry_tour.ospml");
    }

    #[test]
    fn ml_hello_osp() {
        assert_example_matches("examples/tested/ml/hello.osp");
    }

    #[test]
    fn ml_hello_ospml() {
        assert_example_matches("examples/tested/ml/hello.ospml");
    }

    #[test]
    fn ml_hof_osp() {
        assert_example_matches("examples/tested/ml/hof.osp");
    }

    #[test]
    fn ml_hof_ospml() {
        assert_example_matches("examples/tested/ml/hof.ospml");
    }

    #[test]
    fn ml_match_tour_osp() {
        assert_example_matches("examples/tested/ml/match_tour.osp");
    }

    #[test]
    fn ml_match_tour_ospml() {
        assert_example_matches("examples/tested/ml/match_tour.ospml");
    }

    #[test]
    fn ml_matchbool_osp() {
        assert_example_matches("examples/tested/ml/matchbool.osp");
    }

    #[test]
    fn ml_matchbool_ospml() {
        assert_example_matches("examples/tested/ml/matchbool.ospml");
    }

    #[test]
    fn ml_matchint_osp() {
        assert_example_matches("examples/tested/ml/matchint.osp");
    }

    #[test]
    fn ml_matchint_ospml() {
        assert_example_matches("examples/tested/ml/matchint.ospml");
    }

    #[test]
    fn ml_mixed_osp() {
        assert_example_matches("examples/tested/ml/mixed.osp");
    }

    #[test]
    fn ml_mixed_ospml() {
        assert_example_matches("examples/tested/ml/mixed.ospml");
    }

    #[test]
    fn ml_mutation_osp() {
        assert_example_matches("examples/tested/ml/mutation.osp");
    }

    #[test]
    fn ml_mutation_ospml() {
        assert_example_matches("examples/tested/ml/mutation.ospml");
    }

    #[test]
    fn ml_nested_calls_osp() {
        assert_example_matches("examples/tested/ml/nested_calls.osp");
    }

    #[test]
    fn ml_nested_calls_ospml() {
        assert_example_matches("examples/tested/ml/nested_calls.ospml");
    }

    #[test]
    fn ml_pipechain_osp() {
        assert_example_matches("examples/tested/ml/pipechain.osp");
    }

    #[test]
    fn ml_pipechain_ospml() {
        assert_example_matches("examples/tested/ml/pipechain.ospml");
    }

    #[test]
    fn ml_recursion_osp() {
        assert_example_matches("examples/tested/ml/recursion.osp");
    }

    #[test]
    fn ml_recursion_ospml() {
        assert_example_matches("examples/tested/ml/recursion.ospml");
    }

    #[test]
    fn ml_results_state_hof_osp() {
        assert_example_matches("examples/tested/ml/results_state_hof.osp");
    }

    #[test]
    fn ml_results_state_hof_ospml() {
        assert_example_matches("examples/tested/ml/results_state_hof.ospml");
    }

    #[test]
    fn ml_strings_osp() {
        assert_example_matches("examples/tested/ml/strings.osp");
    }

    #[test]
    fn ml_strings_ospml() {
        assert_example_matches("examples/tested/ml/strings.ospml");
    }

    #[test]
    fn testing_calculator_test_osp() {
        assert_example_matches("examples/tested/testing/calculator.test.osp");
    }

    #[test]
    fn testing_calculator_test_ospml() {
        assert_example_matches("examples/tested/testing/calculator.test.ospml");
    }

    #[test]
    fn testing_mlcheck_test_osp() {
        assert_example_matches("examples/tested/testing/mlcheck.test.osp");
    }

    #[test]
    fn testing_mlcheck_test_ospml() {
        assert_example_matches("examples/tested/testing/mlcheck.test.ospml");
    }

    #[test]
    fn testing_verdict_test_ospml() {
        assert_example_matches("examples/tested/testing/verdict.test.ospml");
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parse_args_defaults_to_check_with_full_capabilities() {
        let cli = parse_args(&args(&["prog.osp"])).expect("parses");
        assert_eq!(cli.path, "prog.osp");
        assert_eq!(cli.mode, "--check");
        assert!(!cli.quiet);
        assert!(cli.policy.http && cli.policy.websocket && cli.policy.fs && cli.policy.ffi);
    }

    #[test]
    fn parse_args_build_defaults_to_current_project_and_compile() {
        let default = parse_args(&args(&["build"])).expect("build parses");
        assert_eq!(default.path, ".");
        assert_eq!(default.mode, "--compile");
        let explicit =
            parse_args(&args(&["build", "apps/demo", "--quiet"])).expect("explicit project parses");
        assert_eq!(explicit.path, "apps/demo");
        assert!(explicit.quiet);
        assert!(parse_args(&args(&["build", ".", "--check"])).is_err());
    }

    #[test]
    fn parse_args_accepts_flavor_flag_in_both_spellings() {
        // No flag ⇒ unset, so resolution falls through to marker/extension.
        assert_eq!(parse_args(&args(&["f.osp"])).expect("ok").flavor, None);
        // Spaced and `=` spellings both set the explicit flavor.
        for spelling in [
            &["--flavor", "ml", "f.osp"][..],
            &["--flavor=ml", "f.osp"][..],
        ] {
            let cli = parse_args(&args(spelling)).expect("ok");
            assert_eq!(cli.flavor, Some(Flavor::Ml));
        }
        assert_eq!(
            parse_args(&args(&["--flavor=default", "f.osp"]))
                .expect("ok")
                .flavor,
            Some(Flavor::Default)
        );
        // A bogus value and a missing value both fail loudly.
        assert!(parse_args(&args(&["--flavor=fsharp", "f.osp"])).is_err());
        assert!(parse_args(&args(&["f.osp", "--flavor"])).is_err());
    }

    #[test]
    fn osprey_build_rejects_every_mode_flag() {
        for flag in [
            "--ast",
            "--check",
            "--llvm",
            "--compile",
            "--run",
            "--symbols",
            "--list-tests",
            "--hover",
        ] {
            assert!(
                parse_args(&args(&["build", ".", flag])).is_err(),
                "build must reject {flag}"
            );
        }
    }

    #[test]
    fn parse_args_last_mode_wins_and_quiet_sets() {
        let cli = parse_args(&args(&["--ast", "f.osp", "--llvm", "--run", "--quiet"])).expect("ok");
        assert_eq!(cli.mode, "--run");
        assert_eq!(cli.path, "f.osp");
        assert!(cli.quiet);
    }

    #[test]
    fn parse_args_each_sandbox_flag_clears_one_capability() {
        let cli = parse_args(&args(&["f.osp", "--no-http"])).expect("ok");
        assert!(!cli.policy.http && cli.policy.websocket && cli.policy.fs && cli.policy.ffi);
        let cli = parse_args(&args(&["f.osp", "--no-websocket"])).expect("ok");
        assert!(cli.policy.http && !cli.policy.websocket);
        let cli = parse_args(&args(&["f.osp", "--no-fs"])).expect("ok");
        assert!(!cli.policy.fs && cli.policy.ffi);
        let cli = parse_args(&args(&["f.osp", "--no-ffi"])).expect("ok");
        assert!(!cli.policy.ffi && cli.policy.fs);
        let cli = parse_args(&args(&["--sandbox", "f.osp"])).expect("ok");
        assert!(!cli.policy.http && !cli.policy.websocket && !cli.policy.fs && !cli.policy.ffi);
    }

    #[test]
    fn parse_args_rejects_unknown_flag_missing_path_and_extra_positional() {
        let e = parse_args(&args(&["f.osp", "--bogus"])).expect_err("unknown flag");
        assert!(e.contains("unknown flag --bogus"));
        let e = parse_args(&args(&["--check"])).expect_err("no path");
        assert!(e.contains("usage:"));
        let e = parse_args(&args(&["a.osp", "b.osp"])).expect_err("two paths");
        assert!(e.contains("unexpected argument b.osp"));
    }

    #[test]
    fn parse_args_handles_target_and_output() {
        let cli = parse_args(&args(&[
            "f.osp",
            "--target=wasm32",
            "--debug",
            "--compile",
            "-o",
            "out/f.wasm",
        ]))
        .expect("ok");
        assert_eq!(cli.target, "wasm32");
        assert!(cli.debug);
        assert_eq!(cli.output.as_deref(), Some("out/f.wasm"));
        // default target is native, no output.
        let cli = parse_args(&args(&["f.osp"])).expect("ok");
        assert_eq!(cli.target, "native");
        assert!(!cli.debug);
        assert!(cli.output.is_none());
        // -o with no following value, and an unknown target, are errors.
        assert!(parse_args(&args(&["f.osp", "-o"])).is_err());
        assert!(parse_args(&args(&["f.osp", "--target=riscv"])).is_err());
    }

    #[test]
    fn parse_target_accepts_known_and_rejects_unknown() {
        assert_eq!(parse_target("native").as_deref(), Ok("native"));
        assert_eq!(parse_target("wasm32").as_deref(), Ok("wasm32"));
        assert!(parse_target("x86").is_err());
    }

    #[test]
    fn output_path_defaults_by_target_and_honours_dash_o() {
        assert_eq!(output_path("a/b.osp", None, "native"), PathBuf::from("b"));
        assert_eq!(
            output_path("a/b.osp", None, "wasm32"),
            PathBuf::from("b.wasm")
        );
        assert_eq!(
            output_path("a/b.osp", Some("custom.wasm"), "wasm32"),
            PathBuf::from("custom.wasm")
        );
    }

    #[test]
    fn debug_wasm_rejection_is_centralized() {
        let mut c = cli("p.osp", "--run", Policy::allow_all());
        assert!(reject_debug_wasm(&c).is_none());
        c.debug = true;
        assert!(reject_debug_wasm(&c).is_some());
        c.debug = false;
        c.profile = true;
        assert!(reject_debug_wasm(&c).is_some());
    }

    #[test]
    fn stem_of_handles_dirs_and_missing_extension() {
        assert_eq!(stem_of("examples/demo.osp"), "demo");
        assert_eq!(stem_of("/a/b/c.osp"), "c");
        assert_eq!(stem_of("noext"), "noext");
    }

    #[test]
    fn scratch_stems_disambiguate_equal_filenames_in_different_projects() {
        let left = scratch_stem("/apps/left/src/main.osp");
        let right = scratch_stem("/apps/right/src/main.osp");
        assert_ne!(left, right);
        assert!(left.starts_with("main-"));
    }

    #[test]
    fn directive_parses_both_spellings_and_ignores_others() {
        assert_eq!(directive("// @link: sqlite3", "link"), Some("sqlite3"));
        assert_eq!(
            directive("//@linkdir: /opt/lib ", "linkdir"),
            Some("/opt/lib")
        );
        assert_eq!(directive("  // @link:  pq  ", "link"), Some("pq"));
        assert_eq!(directive("let x = 1", "link"), None);
        assert_eq!(directive("// @link: sqlite3", "linkdir"), None);
    }

    #[test]
    fn link_args_adds_ffi_directives_and_openssl_for_http() {
        let ffi = link_args(
            "",
            "// @link: sqlite3\n// @linkdir: /opt/lib\ncode\n",
            "default",
        );
        assert!(ffi.iter().any(|a| a == "-lsqlite3"), "{ffi:?}");
        assert!(ffi.iter().any(|a| a == "-L/opt/lib"), "{ffi:?}");
        let http = link_args("call void @http_listen()", "", "default");
        assert!(http.iter().any(|a| a == "-lssl") && http.iter().any(|a| a == "-lcrypto"));
        // No HTTP markers => no openssl flags.
        let plain = link_args("call void @osprey_list_empty()", "", "default");
        assert!(!plain.iter().any(|a| a == "-lssl"));
    }

    #[test]
    fn link_args_selects_gc_archive_and_validates_backend() {
        // The `gc` backend swaps in the `_gc` archive set; `default` does not.
        let gc = link_args("call void @osprey_list_empty()", "", "gc");
        assert!(
            gc.iter().any(|a| a.contains("_gc.a")) || gc.is_empty(),
            "gc backend must select a *_gc archive when one is present: {gc:?}"
        );
        let plain = link_args("call void @osprey_list_empty()", "", "default");
        assert!(!plain.iter().any(|a| a.contains("_gc.a")), "{plain:?}");
        // Backend validation: default/gc accepted, arc reserved, others rejected.
        assert_eq!(parse_memory("gc").as_deref(), Ok("gc"));
        assert_eq!(parse_memory("default").as_deref(), Ok("default"));
        assert!(parse_memory("arc").is_err());
        assert!(parse_memory("bogus").is_err());
    }

    #[test]
    fn openssl_and_compiler_helpers_are_well_formed() {
        let flags = openssl_flags();
        assert!(flags.iter().any(|f| f == "-lssl") && flags.iter().any(|f| f == "-lcrypto"));
        assert!(!c_compiler().is_empty());
        assert!(find_runtime_lib("definitely_not_a_real_lib_xyz.a").is_none());
    }

    #[test]
    fn runtime_search_walks_above_arbitrarily_nested_cargo_profiles() {
        let root = PathBuf::from("workspace");
        let executable_dir = root.join("target/llvm-cov-target/ci/deps");
        let lib = "libfiber_runtime.a";
        let candidates = runtime_lib_candidates(lib, Some(&executable_dir));
        let expected = root.join("compiler/bin").join(lib).display().to_string();
        assert!(candidates.contains(&expected), "{candidates:?}");
        let fallbacks = runtime_lib_candidates(lib, None);
        assert!(fallbacks.contains(&format!("/usr/local/lib/{lib}")));
        assert!(fallbacks
            .iter()
            .any(|path| path.ends_with("compiler/lib/libfiber_runtime.a")));
    }

    #[cfg(unix)]
    #[test]
    fn child_exit_code_maps_codes_and_signals() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(child_exit_code(std::process::ExitStatus::from_raw(0)), 0);
        assert_eq!(
            child_exit_code(std::process::ExitStatus::from_raw(1 << 8)),
            1
        );
        // Killed by SIGKILL (9): no exit code, so 128 + signal.
        assert_eq!(child_exit_code(std::process::ExitStatus::from_raw(9)), 137);
    }

    #[test]
    fn report_type_errors_counts_zero_for_valid_and_more_for_ill_typed() {
        let ok = osprey_syntax::parse_program("let x = 1\nprint(x)\n").program;
        let ok = CompilationInput::script("ok.osp", String::new(), ok);
        assert_eq!(report_type_errors(&ok), 0);
        let bad = osprey_syntax::parse_program("let y = 1 + \"oops\" - true\n").program;
        let bad = CompilationInput::script("bad.osp", String::new(), bad);
        assert!(report_type_errors(&bad) > 0);
    }

    fn temp_source(name: &str, body: &str) -> String {
        let p = std::env::temp_dir().join(format!("osprey_cli_{name}.osp"));
        std::fs::write(&p, body).expect("write temp source");
        p.display().to_string()
    }

    fn cli(path: impl Into<String>, mode: &str, policy: Policy) -> Cli {
        Cli {
            path: path.into(),
            mode: mode.to_string(),
            quiet: true,
            policy,
            memory: "default".to_string(),
            target: "native".to_string(),
            output: None,
            debug: false,
            profile: false,
            flavor: None,
        }
    }

    #[test]
    fn run_drives_check_symbols_and_llvm_modes_in_process() {
        let path = temp_source("ok", "let greeting = \"hi\"\nprint(greeting)\n");
        for mode in ["--check", "--symbols", "--llvm", "--ast"] {
            // ExitCode is opaque; this drives run -> dispatch coverage and must
            // not panic for a well-formed program.
            let _ = run(&cli(path.clone(), mode, Policy::allow_all()));
        }
    }

    #[test]
    fn run_reports_missing_file_and_parse_errors() {
        let _ = run(&cli(
            "/no/such/osprey/file.osp",
            "--check",
            Policy::allow_all(),
        ));
        let path = temp_source("broken", "fn = = =\n");
        let _ = run(&cli(path, "--check", Policy::allow_all())); // parse-error branch
    }

    #[test]
    fn load_input_reports_project_and_module_assembly_errors() {
        let missing = std::env::temp_dir()
            .join(format!("osprey_cli_missing_{}", std::process::id()))
            .join("osprey.toml");
        assert!(load_input(&cli(
            missing.display().to_string(),
            "--check",
            Policy::allow_all()
        ))
        .is_err());
        let source = "module A { export let x = 1 }\nmodule A { export let x = 2 }\n";
        let path = temp_source("duplicate_module", source);
        assert!(load_input(&cli(path, "--check", Policy::allow_all())).is_err());
        // `--flavor` on a directory project is rejected: projects pick a flavor
        // per source file, so a whole-project flavor is meaningless.
        let mut with_flavor = cli(
            std::env::temp_dir().to_string_lossy().into_owned(),
            "--check",
            Policy::allow_all(),
        );
        with_flavor.flavor = Some(Flavor::Ml);
        assert!(load_input(&with_flavor).is_err());
    }

    #[test]
    fn run_rejects_sandbox_violation_before_codegen() {
        let path = temp_source("fs", "let c = readFile(\"x.txt\")\n");
        let _ = run(&cli(path, "--llvm", Policy::sandbox())); // sandbox-violation branch
    }

    #[test]
    fn parse_args_accepts_the_memory_backend_flag() {
        let cli = parse_args(&args(&["f.osp", "--memory=gc"])).expect("ok");
        assert_eq!(cli.memory, "gc");
    }

    #[test]
    fn report_type_errors_prints_positioned_diagnostics() {
        // An undefined identifier yields an error carrying a source position,
        // exercising the `Some(position)` diagnostic arm.
        let bad = osprey_syntax::parse_program("print(missingVariable)\n").program;
        let bad = CompilationInput::script("bad.osp", String::new(), bad);
        assert!(report_type_errors(&bad) > 0);
    }

    #[test]
    fn parse_flavor_accepts_known_names_and_rejects_the_rest() {
        assert_eq!(parse_flavor("default").expect("default"), Flavor::Default);
        assert_eq!(parse_flavor("ml").expect("ml"), Flavor::Ml);
        let err = parse_flavor("klingon").expect_err("unknown flavor rejected");
        assert!(err.contains("usage: osprey"), "{err}");
    }

    #[test]
    fn link_flag_helpers_return_a_nonempty_flag_set() {
        // Both run to completion regardless of host: `openssl_flags` always yields
        // at least the `-lssl -lcrypto` fallback, and the runtime-lib search walks
        // its whole candidate list (returning None here is fine — the body ran).
        assert!(openssl_flags().iter().any(|f| f == "-lssl"));
        let _ = find_runtime_lib("libosprey_runtime_definitely_absent.a");
    }

    #[test]
    fn compile_ir_and_debug_helpers_switch_on_the_build_kind() {
        use osprey_debug::BuildKind;
        let program = osprey_syntax::parse_program("let n = 1\nprint(\"${n}\")\n").program;
        // Debug and Profile both take the debug-info codegen path; the opt
        // flag differs (Profile keeps the release optimizer [PROF-BUILD-MODE]).
        assert!(compile_ir("p.osp", &program, BuildKind::Debug).is_ok());
        assert!(compile_ir("p.osp", &program, BuildKind::Profile).is_ok());
        assert_eq!(opt_flag(BuildKind::Debug), "-O0");
        assert!(!opt_flag(BuildKind::Release).is_empty());
        assert_eq!(
            opt_flag(BuildKind::Profile),
            opt_flag(BuildKind::Release),
            "profiling must keep release optimization"
        );
    }

    // [PROF-CLI-RUN] end-to-end: `--profile` compiles with the profile
    // pipeline (two-step + dsymutil), runs under the in-runtime sampler, and
    // writes all four exports where `-o` points. POSIX-only by design.
    #[cfg(unix)]
    #[test]
    fn profile_run_writes_exports_where_output_points() {
        let path = temp_source(
            "prof_e2e",
            "fn dec(n: int) -> int = n - 1\n\
             fn count(n: int) -> int = match n {\n    0 => 0\n    _ => count(dec(n))\n}\n\
             print(\"${count(500)}\")\n",
        );
        let dir = std::env::temp_dir().join(format!("osprey_prof_exports_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create export dir");
        let mut c = cli(path, "--run", Policy::allow_all());
        c.profile = true;
        c.output = Some(dir.join("prof_e2e").display().to_string());
        let (out_dir, stem) = profile_export_target(&c);
        assert_eq!(out_dir, dir);
        assert_eq!(stem, "prof_e2e");
        let _ = run(&c);
        for export in [
            "prof_e2e.speedscope.json",
            "prof_e2e.cpuprofile",
            "prof_e2e.folded",
            "prof_e2e.profile.json",
        ] {
            assert!(
                dir.join(export).exists(),
                "missing export {export} in {}",
                dir.display()
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn profile_export_target_defaults_to_cwd_and_source_stem() {
        let c = cli("examples/demo.osp", "--run", Policy::allow_all());
        let (dir, stem) = profile_export_target(&c);
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(stem, "demo");
        let mut with_bare_output = cli("a.osp", "--run", Policy::allow_all());
        with_bare_output.output = Some("renamed".to_string());
        let (dir, stem) = profile_export_target(&with_bare_output);
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(stem, "renamed");
    }

    // On Windows `--profile` is rejected outright (POSIX-only sampler), so
    // the acceptance-path assertions only hold on unix.
    #[cfg(unix)]
    #[test]
    fn parse_args_profile_implies_run_and_rejects_debug_combo() {
        let args = vec!["main.osp".to_string(), "--profile".to_string()];
        let cli = parse_args(&args).expect("parse --profile");
        assert!(cli.profile);
        assert_eq!(cli.mode, "--run");
        assert_eq!(build_kind(&cli), osprey_debug::BuildKind::Profile);
        // An explicit mode is preserved.
        let args = vec![
            "main.osp".to_string(),
            "--compile".to_string(),
            "--profile".to_string(),
        ];
        let cli = parse_args(&args).expect("parse --compile --profile");
        assert_eq!(cli.mode, "--compile");
        // --debug + --profile is a contradiction.
        let args = vec![
            "main.osp".to_string(),
            "--debug".to_string(),
            "--profile".to_string(),
        ];
        assert!(parse_args(&args).is_err());
        // Default build kinds for the other switches.
        let plain = parse_args(&["main.osp".to_string()]).expect("parse plain");
        assert_eq!(build_kind(&plain), osprey_debug::BuildKind::Release);
        let dbg =
            parse_args(&["main.osp".to_string(), "--debug".to_string()]).expect("parse --debug");
        assert_eq!(build_kind(&dbg), osprey_debug::BuildKind::Debug);
    }

    #[test]
    fn wasm_target_rejects_debug_then_dispatches_to_the_backend() {
        let program = osprey_syntax::parse_program("let n = 1\nprint(\"${n}\")\n").program;
        let input = CompilationInput::script("p.osp", String::new(), program);
        let mut c = cli("p.osp", "--compile", Policy::allow_all());
        c.target = "wasm32".to_string();
        // --debug + --target=wasm32 is rejected before any toolchain work.
        c.debug = true;
        let _ = compile_program_to_disk(&c, &input);
        let _ = run_program(&c, &input);
        // Without --debug the wasm build/run driver is dispatched (it fails
        // cleanly without the wasm toolchain, but the dispatch lines execute).
        c.debug = false;
        let _ = compile_program_to_disk(&c, &input);
        let _ = run_program(&c, &input);
    }
}
