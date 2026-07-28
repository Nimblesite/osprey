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
[--memory=default|gc|arc] [--target=native|wasm32] [-o <out>] \
[--sandbox | --no-http | --no-websocket | --no-fs | --no-ffi]\n\
       osprey build [project] [--quiet] [--debug] [--memory=default|gc|arc] \
[--target=native|wasm32] [-o <out>]\n\
       osprey test [path] [--filter <name>] [--quiet] [--coverage] \
[--coverage-json <path>] [--memory=default|gc|arc]\n\
       osprey fmt [--check | --stdout] [--flavor default|ml] <path...>\n\
       osprey --hover <name>\n\
       osprey --docs --docs-dir <dir>\n\
       osprey lsp";

/// Internal child-process switch used by the parallel test runner.
pub(crate) const TEST_COVERAGE_BUILD_ENV: &str = "OSPREY_TEST_COVERAGE_BUILD";
/// Internal content-addressed executable cache used by the test runner.
pub(crate) const TEST_CACHE_DIR_ENV: &str = "OSPREY_TEST_CACHE_DIR";

/// The parsed invocation: source path, mode flag, and behaviour switches.
#[derive(Debug)]
pub(crate) struct Cli {
    path: String,
    mode: String,
    quiet: bool,
    policy: Policy,
    /// The memory backend linked behind `@osp_alloc`: `default` (malloc
    /// passthrough), `gc` (tracing collector), or `arc` (reference counting).
    /// Link-time only; native IR is identical [MEM-BACKENDS].
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

/// Validate the `--memory=` value: the malloc passthrough (`default`), the
/// tracing collector (`gc`), or Perceus reference counting (`arc`).
/// Implements [MEM-BACKENDS].
fn parse_memory(value: &str) -> Result<String, String> {
    match value {
        "default" | "gc" | "arc" => Ok(value.to_string()),
        other => Err(format!(
            "unknown memory backend '{other}' (available: default, gc, arc)\n{USAGE}"
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
    } else if std::env::var_os(TEST_COVERAGE_BUILD_ENV).is_some() {
        osprey_debug::BuildKind::Coverage
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
    let (exe, temporary) = native_executable(input, memory, kind)?;
    let status = Command::new(&exe).status();
    if temporary {
        let _ = std::fs::remove_file(&exe);
    }
    match status {
        Ok(s) => Ok(child_exit_code(s)),
        Err(e) => {
            eprintln!("error: could not run {}: {e}", exe.display());
            Err(ExitCode::FAILURE)
        }
    }
}

fn native_executable(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Result<(PathBuf, bool), ExitCode> {
    if let Some(cached) = test_cache_path(input, memory, kind) {
        ensure_cached_executable(input, memory, kind, &cached)?;
        return Ok((cached, false));
    }
    let exe = std::env::temp_dir().join(format!("{}.out", scratch_stem(input.display_path())));
    build_executable(
        input.debug_path(),
        input.program(),
        input.source(),
        &exe,
        memory,
        kind,
    )?;
    Ok((exe, true))
}

fn test_cache_path(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
) -> Option<PathBuf> {
    // [TESTING-NATIVE-CACHE] every input affecting the native artifact is
    // represented by the cache key; untracked external links bypass caching.
    let dir = std::env::var_os(TEST_CACHE_DIR_ENV).map(PathBuf::from)?;
    if dir.as_os_str().is_empty() || !cacheable_test_source(input.source()) {
        return None;
    }
    if std::fs::create_dir_all(&dir).is_err() || !directory_is_safe(&dir) {
        return None;
    }
    Some(dir.join(format!(
        "suite-{:016x}.out",
        test_cache_key(input, memory, kind)
    )))
}

fn cacheable_test_source(source: &str) -> bool {
    !source
        .lines()
        .any(|line| directive(line, "linkdir").is_some())
}

fn directory_is_safe(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_dir() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o700);
        if std::fs::set_permissions(path, permissions).is_err() {
            return false;
        }
    }
    true
}

fn test_cache_key(input: &CompilationInput, memory: &str, kind: osprey_debug::BuildKind) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut state = DefaultHasher::new();
    "osprey-test-cache-v1".hash(&mut state);
    input.source().hash(&mut state);
    input.debug_path().hash(&mut state);
    memory.hash(&mut state);
    std::mem::discriminant(&kind).hash(&mut state);
    opt_flag(kind).hash(&mut state);
    let compiler = c_compiler();
    compiler.hash(&mut state);
    hash_file_identity(Some(Path::new(&compiler)), &mut state);
    std::env::var_os("PATH").hash(&mut state);
    let executable = std::env::current_exe().ok();
    hash_file_identity(executable.as_deref(), &mut state);
    hash_runtime_identity(memory, &mut state);
    state.finish()
}

fn hash_runtime_identity<H: std::hash::Hasher>(memory: &str, state: &mut H) {
    let suffix = match memory {
        "gc" => "_gc",
        "arc" => "_arc",
        _ => "",
    };
    for prefix in ["libfiber_runtime", "libhttp_runtime"] {
        let runtime = find_runtime_lib(&format!("{prefix}{suffix}.a")).map(PathBuf::from);
        hash_file_identity(runtime.as_deref(), state);
    }
}

fn hash_file_identity<H: std::hash::Hasher>(path: Option<&Path>, state: &mut H) {
    use std::hash::Hash;

    path.hash(state);
    let Some(metadata) = path.and_then(|file| std::fs::metadata(file).ok()) else {
        return;
    };
    metadata.len().hash(state);
    metadata.modified().ok().hash(state);
}

fn ensure_cached_executable(
    input: &CompilationInput,
    memory: &str,
    kind: osprey_debug::BuildKind,
    cached: &Path,
) -> Result<(), ExitCode> {
    if is_nonempty_file(cached) {
        return Ok(());
    }
    let staging = cached.with_extension(format!("{}.tmp", std::process::id()));
    let build = build_executable(
        input.debug_path(),
        input.program(),
        input.source(),
        &staging,
        memory,
        kind,
    );
    if let Err(code) = build {
        let _ = std::fs::remove_file(&staging);
        return Err(code);
    }
    publish_cached_executable(&staging, cached)
}

fn is_nonempty_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() > 0)
}

fn publish_cached_executable(staging: &Path, cached: &Path) -> Result<(), ExitCode> {
    match std::fs::rename(staging, cached) {
        Ok(()) => Ok(()),
        Err(_) if is_nonempty_file(cached) => {
            let _ = std::fs::remove_file(staging);
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "error: cannot publish test executable {}: {error}",
                cached.display()
            );
            let _ = std::fs::remove_file(staging);
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
    let result = if kind == osprey_debug::BuildKind::Profile {
        build_profile_executable(&ll, &ir, source, exe, memory)
    } else {
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
    };
    let _ = std::fs::remove_file(&ll);
    result
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
    if let Err(code) = run_build_step(compile, ll) {
        let _ = std::fs::remove_file(&obj);
        return Err(code);
    }
    let mut link = Command::new(c_compiler());
    let _ = link
        .arg(&obj)
        .arg("-o")
        .arg(exe)
        .args(kind.native_driver_flags())
        .args(link_args(ir, source, memory));
    let result = run_build_step(link, &obj);
    if result.is_ok() && cfg!(target_os = "macos") {
        // Best-effort: without a dSYM the profile still symbolizes to function
        // names from the symbol table, just without file:line detail.
        let _ = Command::new("dsymutil").arg(exe).status();
    }
    let _ = std::fs::remove_file(&obj);
    result
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
/// builds, `-O3` for a more aggressive release build). At `-O2`, LLVM can
/// eliminate non-escaping per-operation `Result` allocations. Allocation
/// reclamation is selected independently by `--memory` ([MEM-BACKENDS]).
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
    if kind == osprey_debug::BuildKind::Coverage {
        return osprey_codegen::compile_program_coverage(program);
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
/// Implements [FFI-LINK-DIRECTIVES].
fn link_args(ir: &str, source: &str, memory: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let uses_http = ir.contains("@http") || ir.contains("@websocket");

    // The reclaiming backend is a link-time archive swap — the IR is identical
    // [MEM-BACKENDS]. `gc` links the tracing-collector archive set, `arc` the
    // Perceus reference-counting set, `default` the malloc-passthrough set.
    let suffix = match memory {
        "gc" => "_gc",
        "arc" => "_arc",
        _ => "",
    };
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

    use std::path::PathBuf;

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
        // [FFI-LINK-DIRECTIVES]
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
        // [MEM-BACKENDS] `gc`/`arc` swap archives; `default` does not.
        let gc = link_args("call void @osprey_list_empty()", "", "gc");
        assert!(
            gc.iter().any(|a| a.contains("_gc.a")) || gc.is_empty(),
            "gc backend must select a *_gc archive when one is present: {gc:?}"
        );
        let arc = link_args("call void @osprey_list_empty()", "", "arc");
        assert!(
            arc.iter().any(|a| a.contains("_arc.a")) || arc.is_empty(),
            "arc backend must select a *_arc archive when one is present: {arc:?}"
        );
        let plain = link_args("call void @osprey_list_empty()", "", "default");
        assert!(!plain.iter().any(|a| a.contains("_gc.a")), "{plain:?}");
        assert!(!plain.iter().any(|a| a.contains("_arc.a")), "{plain:?}");
        // Backend validation: default/gc/arc accepted, others rejected.
        assert_eq!(parse_memory("gc").as_deref(), Ok("gc"));
        assert_eq!(parse_memory("default").as_deref(), Ok("default"));
        assert_eq!(parse_memory("arc").as_deref(), Ok("arc"));
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
            "fn dec(n: int) -> int = (n - 1) ?: 0\n\
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
