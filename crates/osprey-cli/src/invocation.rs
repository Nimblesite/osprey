//! Invocation support for the compiler driver.

use crate::sandbox::Policy;
use osprey_syntax::Flavor;

pub(crate) const USAGE: &str =
    "usage: osprey <file-or-project> [--check | --ast | --llvm | --compile | --run | \
--symbols | --list-tests | --doctests | --deps] [--quiet] [--debug] [--profile] [--flavor default|ml] \
[--memory=default|gc|arc] [--target=native|wasm32|ios|ios-sim|android-arm64|android-x64] [--entry-only] [-o <out>] \
[--sandbox | --no-http | --no-websocket | --no-fs | --no-ffi]\n\
       osprey build [project] [--quiet] [--debug] [--memory=default|gc|arc] \
[--target=native|wasm32|ios|ios-sim|android-arm64|android-x64] [-o <out>]\n\
       osprey test [path] [--filter <name>] [--quiet] [--coverage] \
[--coverage-json <path>] [--memory=default|gc|arc]\n\
       osprey fmt [--check | --stdout] [--flavor default|ml] <path...>\n\
       osprey --hover <name>\n\
       osprey --docs [<file-or-project> | --source <file-or-project>] --docs-dir <dir> \
[--docs-format markdown|html] [--docs-theme osprey|midnight|paper] \
[--docs-page <markdown-file-or-directory>]... [--docs-css <stylesheet>]...\n\
       osprey lsp";

/// Internal child-process switch used by the parallel test runner.
pub(crate) const TEST_COVERAGE_BUILD_ENV: &str = "OSPREY_TEST_COVERAGE_BUILD";
/// Internal content-addressed executable cache used by the test runner.
pub(crate) const TEST_CACHE_DIR_ENV: &str = "OSPREY_TEST_CACHE_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MobileExports {
    All,
    EntryOnly,
}

impl MobileExports {
    pub(super) fn entry_only(self) -> bool {
        self == Self::EntryOnly
    }
}

/// The parsed invocation: source path, mode flag, and behaviour switches.
#[derive(Debug)]
pub(crate) struct Cli {
    pub(super) path: String,
    pub(super) mode: String,
    pub(super) quiet: bool,
    pub(super) policy: Policy,
    /// The memory backend linked behind `@osp_alloc`: `default` (malloc
    /// passthrough), `gc` (tracing collector), or `arc` (reference counting).
    /// Link-time only; native IR is identical [MEM-BACKENDS].
    pub(super) memory: String,
    /// Host executable, WebAssembly, or iPhone C ABI archive.
    /// [WASM-TARGET] [IOS-TARGET]
    pub(super) target: String,
    /// Mobile archive exposes only `osprey_main` to its host.
    pub(super) exports: MobileExports,
    /// Explicit output artifact path (`-o`); defaults to the source stem.
    pub(super) output: Option<String>,
    /// Emit source-level debug metadata and link a debugger-friendly binary.
    pub(super) debug: bool,
    /// Profile the run [PROF-CLI-RUN]: build with line tables + frame pointers
    /// at full optimization, sample via the in-runtime profiler, then export
    /// and report (docs/specs/0028-Profiler.md).
    pub(super) profile: bool,
    /// Explicit source flavor from `--flavor`; `None` when unset, so flavor
    /// resolution falls through to the marker/extension precedence
    /// ([FLAVOR-SELECT], docs/specs/0023-LanguageFlavors.md).
    pub(super) flavor: Option<Flavor>,
}

/// Parse the argument list: the first non-flag is the source path; mode flags
/// select the action (last one wins); the rest toggle behaviour.
pub(super) fn parse_args(args: &[String]) -> Result<Cli, String> {
    let (project_build, args) = match args.split_first() {
        Some((first, rest)) if first == "build" => (true, rest),
        _ => (false, args),
    };
    let mut cli = default_invocation(project_build);
    let (path, mode_chosen) = parse_options(&mut cli, args, project_build)?;
    cli.path = invocation_path(path, project_build)?;
    cli.exports = mobile_exports(cli.exports.entry_only(), &cli.target)?;
    apply_profile_rules(&mut cli, mode_chosen)?;
    Ok(cli)
}

fn parse_options(
    cli: &mut Cli,
    args: &[String],
    project_build: bool,
) -> Result<(Option<String>, bool), String> {
    let mut path = None;
    let mut mode_chosen = project_build;
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        if is_mode(argument) {
            select_mode(cli, argument, project_build)?;
            mode_chosen = true;
        } else {
            parse_option(cli, &mut path, argument, &mut arguments)?;
        }
    }
    Ok((path, mode_chosen))
}

fn default_invocation(project_build: bool) -> Cli {
    let mode = if project_build {
        "--compile"
    } else {
        "--check"
    };
    Cli {
        path: String::new(),
        mode: mode.into(),
        quiet: false,
        policy: Policy::allow_all(),
        memory: "default".into(),
        target: "native".into(),
        exports: MobileExports::All,
        output: None,
        debug: false,
        profile: false,
        flavor: None,
    }
}

fn invocation_path(path: Option<String>, project_build: bool) -> Result<String, String> {
    match path {
        Some(path) => Ok(path),
        None if project_build => Ok(".".into()),
        None => Err(USAGE.into()),
    }
}

fn is_mode(argument: &str) -> bool {
    matches!(
        argument,
        "--ast"
            | "--check"
            | "--llvm"
            | "--compile"
            | "--run"
            | "--symbols"
            | "--list-tests"
            | "--hover"
            | "--deps"
            | "--doctests"
    )
}

fn select_mode(cli: &mut Cli, argument: &str, project_build: bool) -> Result<(), String> {
    if project_build {
        return Err(format!(
            "`osprey build` does not accept mode flag {argument}\n{USAGE}"
        ));
    }
    cli.mode = argument.into();
    Ok(())
}

fn parse_option(
    cli: &mut Cli,
    path: &mut Option<String>,
    argument: &str,
    rest: &mut std::slice::Iter<'_, String>,
) -> Result<(), String> {
    match argument {
        "--quiet" => cli.quiet = true,
        "--debug" => cli.debug = true,
        "--profile" => cli.profile = true,
        "--entry-only" => cli.exports = MobileExports::EntryOnly,
        "--sandbox" => cli.policy = Policy::sandbox(),
        "--no-http" => cli.policy.http = false,
        "--no-websocket" => cli.policy.websocket = false,
        "--no-fs" => cli.policy.fs = false,
        "--no-ffi" => cli.policy.ffi = false,
        _ => return parse_value_option(cli, path, argument, rest),
    }
    Ok(())
}

fn parse_value_option(
    cli: &mut Cli,
    path: &mut Option<String>,
    argument: &str,
    rest: &mut std::slice::Iter<'_, String>,
) -> Result<(), String> {
    match argument {
        "-o" => cli.output = Some(required_value(rest, "-o requires a path")?.into()),
        "--flavor" => {
            cli.flavor = Some(parse_flavor(required_value(
                rest,
                "--flavor requires a value (default|ml)",
            )?)?);
        }
        flag if flag.starts_with("--") => return parse_named_value(cli, flag),
        _ if path.is_none() => *path = Some(argument.into()),
        _ => return Err(format!("unexpected argument {argument}\n{USAGE}")),
    }
    Ok(())
}

fn required_value<'a>(
    rest: &mut std::slice::Iter<'a, String>,
    error: &str,
) -> Result<&'a str, String> {
    rest.next()
        .map(String::as_str)
        .ok_or_else(|| format!("{error}\n{USAGE}"))
}

fn parse_named_value(cli: &mut Cli, flag: &str) -> Result<(), String> {
    match flag.split_once('=') {
        Some(("--flavor", value)) => cli.flavor = Some(parse_flavor(value)?),
        Some(("--memory", value)) => cli.memory = parse_memory(value)?,
        Some(("--target", value)) => cli.target = parse_target(value)?,
        _ => return Err(format!("unknown flag {flag}\n{USAGE}")),
    }
    Ok(())
}

pub(super) fn mobile_exports(entry_only: bool, target: &str) -> Result<MobileExports, String> {
    if !entry_only {
        return Ok(MobileExports::All);
    }
    match target {
        "ios" | "ios-sim" | "android-arm64" | "android-x64" => Ok(MobileExports::EntryOnly),
        _ => Err("--entry-only requires a mobile C ABI target".to_string()),
    }
}

/// Enforce the `--profile` interaction rules [PROF-CLI-RUN]: it conflicts with
/// `--debug` (profiling needs optimized code, debugging needs `-O0`), and a
/// bare `--profile` means "run it and profile it" — unless a mode was chosen
/// explicitly (or this is `osprey build`, whose mode is fixed).
pub(super) fn apply_profile_rules(cli: &mut Cli, mode_chosen: bool) -> Result<(), String> {
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

/// Validate the executable, WebAssembly and iPhone archive targets.
/// [WASM-TARGET] [IOS-TARGET-TRIPLE]
pub(super) fn parse_target(value: &str) -> Result<String, String> {
    match value {
        "native" | "wasm32" | "ios" | "ios-sim" | "android-arm64" | "android-x64" => Ok(value.to_string()),
        other => Err(format!(
            "unknown target '{other}' (available: native, wasm32, ios, ios-sim, android-arm64, android-x64)\n{USAGE}"
        )),
    }
}

/// Validate the `--memory=` value: the malloc passthrough (`default`), the
/// tracing collector (`gc`), or Perceus reference counting (`arc`).
/// Implements [MEM-BACKENDS].
pub(super) fn parse_memory(value: &str) -> Result<String, String> {
    match value {
        "default" | "gc" | "arc" => Ok(value.to_string()),
        other => Err(format!(
            "unknown memory backend '{other}' (available: default, gc, arc)\n{USAGE}"
        )),
    }
}

/// Validate a `--flavor` / marker value into a [`Flavor`]. [FLAVOR-SELECT]
pub(super) fn parse_flavor(value: &str) -> Result<Flavor, String> {
    value.parse().map_err(|e| format!("{e}\n{USAGE}"))
}
