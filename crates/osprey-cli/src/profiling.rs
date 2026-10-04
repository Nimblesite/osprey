//! Profile execution and exports [PROF-CLI-RUN].
use crate::invocation::Cli;
use crate::native::{build_input, child_exit_code, scratch_stem, stem_of};
use crate::project::CompilationInput;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The `--run --profile` pipeline [PROF-CLI-RUN]: profile-build the program,
/// run it with the in-runtime sampler active [PROF-ACTIVATE-ENV], then
/// symbolize, export, and print the terminal report. The program's own exit
/// code is preserved; a post-processing failure warns but never masks the run.
pub(super) fn execute_profiled(cli: &Cli, input: &CompilationInput) -> Result<u8, ExitCode> {
    let exe = profile_executable(cli, input)?;
    let raw = std::env::temp_dir().join(format!("{}.osprof.json", scratch_stem(&cli.path)));
    let status = Command::new(&exe)
        .env("OSPREY_PROFILE", &raw)
        .status()
        .map_err(|error| {
            eprintln!("error: could not run {}: {error}", exe.display());
            ExitCode::FAILURE
        })?;
    report_profile(cli, &exe, &raw);
    let _ = std::fs::remove_file(&raw);
    Ok(child_exit_code(status))
}

fn profile_executable(cli: &Cli, input: &CompilationInput) -> Result<PathBuf, ExitCode> {
    let exe = std::env::temp_dir().join(format!("{}.out", scratch_stem(input.display_path())));
    build_input(input, &exe, &cli.memory, osprey_debug::BuildKind::Profile)?;
    Ok(exe)
}

/// Post-process a raw profile into the exports + terminal report; failures are
/// reported to stderr without failing the run.
pub(super) fn report_profile(cli: &Cli, exe: &Path, raw: &Path) {
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
pub(super) fn profile_export_target(cli: &Cli) -> (PathBuf, String) {
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
