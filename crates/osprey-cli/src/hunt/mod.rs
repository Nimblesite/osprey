//! `osprey hunt` — symbolic bug hunting for Osprey programs, powered by KLEE
//! [SYMEX-CLI] (docs/specs/tooling/SymbolicExecution.md). The program is
//! lowered to one LLVM module with debug locations [SYMEX-CODEGEN], linked
//! against the mock C library bitcode [SYMEX-RUNTIME-BITCODE] and explored
//! natively by KLEE on this host with symbolic stdin and files. Every KLEE
//! error becomes a classified finding at an Osprey `file:line`
//! [SYMEX-FINDINGS], replayed on the ordinary native build [SYMEX-REPLAY].

mod engine;
mod findings;
mod ktest;
mod module;
mod options;
mod process;
mod replay;
mod report;
mod report_explanation;
mod report_files;
mod report_html;
mod toolchain;

use crate::invocation::parse_args;
use crate::native::scratch_stem;
use crate::pipeline::{load_input, report_type_errors};
use crate::project::CompilationInput;
use options::Options;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub(crate) fn run(args: &[String]) -> ExitCode {
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", options::HUNT_USAGE);
        return ExitCode::SUCCESS;
    }
    let options = match options::parse(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match hunt(&options) {
        Ok((rendered, code)) => {
            print!("{rendered}");
            code
        }
        Err(code) => code,
    }
}

/// The rendered report and the exit code [SYMEX-CLI]: `2` for a usage or
/// toolchain problem, `1` for a program that does not compile or a finding
/// that reproduced (or any finding without replay), `0` otherwise.
fn hunt(options: &Options) -> Result<(String, ExitCode), ExitCode> {
    let toolchain = toolchain::locate().map_err(|message| tool_error(&message))?;
    let input = load(options)?;
    let module = module::symbolic_module(&input, options.input_bytes).map_err(|error| {
        eprintln!("{}: {error}", input.display_path());
        ExitCode::FAILURE
    })?;
    let paths = RunPaths::new(options, input.display_path());
    paths
        .prepare(&module)
        .map_err(|message| tool_error(&message))?;
    let _cleanup = Cleanup(&paths);
    announce(options, input.display_path());
    let arguments = engine::arguments(
        &toolchain.runtime,
        &paths.out_dir,
        (options.max_time, options.max_memory),
        &paths.module,
    );
    let ending = engine::run(&toolchain.klee, &arguments, &paths.stderr, options.max_time)
        .map_err(|message| tool_error(&message))?;
    let info = std::fs::read_to_string(paths.out_dir.join("info")).unwrap_or_default();
    let exploration = engine::exploration(&info);
    let scanner = if options.replay {
        findings::scan_all
    } else {
        findings::scan
    };
    let mut scan = scanner(&paths.out_dir).map_err(|message| tool_error(&message))?;
    relocate(&input, &mut scan.findings);
    if ending == engine::Ending::Halted {
        scan.incomplete.push(findings::Incomplete {
            test: "engine".to_string(),
            reason: engine::HALTED_REASON.to_string(),
        });
    }
    if options.replay {
        replay::replay(&input, &mut scan.findings).map_err(|message| tool_error(&message))?;
    }
    scan.findings = findings::dedupe(scan.findings);
    let mut report = report::Report {
        program: input.display_path(),
        options,
        exploration: &exploration,
        scan: &scan,
        context: None,
    };
    report.context = Some(
        report_files::context(
            &report,
            &toolchain,
            &arguments,
            &paths.out_dir,
            &paths.stderr,
        )
        .map_err(|message| tool_error(&message))?,
    );
    let json = serde_json::to_string_pretty(&report::json(&report))
        .map_err(|error| tool_error(&error.to_string()))?;
    let saved = report_files::write(&report, &json).map_err(|message| tool_error(&message))?;
    let rendered = render(&report, &paths, &json, &saved);
    Ok((rendered, report::exit_code(&scan, options.replay)))
}

/// A project's modules share one line numbering in the symbolic module, so a
/// finding's line is mapped back to the module file it was written in, with
/// that file's own line number [SYMEX-FINDINGS]. A script's names stand.
fn relocate(input: &CompilationInput, findings: &mut [findings::Finding]) {
    if !input.is_project() {
        return;
    }
    for location in findings
        .iter_mut()
        .filter_map(|finding| finding.location.as_mut())
    {
        let position = osprey_ast::Position {
            line: location.line,
            column: 0,
        };
        let (file, line, _) = input.location(position);
        location.file = file;
        location.line = line;
    }
}

fn tool_error(message: &str) -> ExitCode {
    eprintln!("osprey hunt: {message}");
    ExitCode::from(2)
}

fn load(options: &Options) -> Result<CompilationInput, ExitCode> {
    let cli =
        parse_args(std::slice::from_ref(&options.path)).map_err(|message| tool_error(&message))?;
    let input = load_input(&cli)?;
    if report_type_errors(&input) != 0 {
        return Err(ExitCode::FAILURE);
    }
    Ok(input)
}

fn announce(options: &Options, program: &str) {
    if !options.quiet && !options.json {
        eprintln!(
            "hunting {program} with KLEE for up to {}s ({} symbolic bytes per stream)...",
            options.max_time, options.input_bytes
        );
    }
}

fn render(
    report: &report::Report<'_>,
    paths: &RunPaths,
    json: &str,
    saved: &report_files::Saved,
) -> String {
    if report.options.json {
        return format!("{json}\n");
    }
    let mut text = report::text(report);
    if paths.keep {
        let _ = writeln!(text, "KLEE output kept in {}", paths.out_dir.display());
    }
    for (label, path) in [("HTML report", &saved.html), ("JSON report", &saved.json)] {
        if let Some(path) = path {
            let _ = writeln!(text, "{label}: {}", path.display());
        }
    }
    text
}

/// Where the module and KLEE's output directory live for one run.
#[derive(Debug)]
struct RunPaths {
    module: PathBuf,
    /// Where the engine's stderr is captured; a pipe could fill and stall it.
    stderr: PathBuf,
    out_dir: PathBuf,
    /// `--out` was given, so the directory outlives the command.
    keep: bool,
}

impl RunPaths {
    fn new(options: &Options, program: &str) -> Self {
        let stem = scratch_stem(program);
        let temp = std::env::temp_dir();
        Self {
            module: temp.join(format!("{stem}-hunt.ll")),
            stderr: temp.join(format!("{stem}-hunt.stderr")),
            out_dir: options
                .out
                .clone()
                .unwrap_or_else(|| temp.join(format!("{stem}-hunt-klee"))),
            keep: options.out.is_some(),
        }
    }

    /// KLEE refuses an existing output directory, so a previous run's is
    /// cleared — and only a previous run's, never an unrelated directory.
    fn prepare(&self, module: &str) -> Result<(), String> {
        clear_run_directory(&self.out_dir)?;
        std::fs::write(&self.module, module)
            .map_err(|error| format!("cannot write {}: {error}", self.module.display()))
    }

    fn finish(&self) {
        let _ = std::fs::remove_file(&self.module);
        let _ = std::fs::remove_file(&self.stderr);
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.out_dir);
        }
    }
}

/// Removes the run's temporary files however `hunt` returns, so a failed
/// engine or replay leaves nothing behind in the temp directory.
struct Cleanup<'a>(&'a RunPaths);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

fn clear_run_directory(directory: &Path) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    let is_empty = std::fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_none());
    if !is_empty && !directory.join("info").exists() {
        return Err(format!(
            "refusing to clear {}: it is not a KLEE output directory",
            directory.display()
        ));
    }
    std::fs::remove_dir_all(directory)
        .map_err(|error| format!("cannot clear {}: {error}", directory.display()))
}

#[cfg(test)]
#[path = "spec_engine_tests.rs"]
mod spec_engine_tests;
#[cfg(test)]
#[path = "spec_findings_tests.rs"]
mod spec_findings_tests;
#[cfg(test)]
#[path = "spec_ktest_tests.rs"]
mod spec_ktest_tests;
#[cfg(test)]
#[path = "spec_replay_tests.rs"]
mod spec_replay_tests;
#[cfg(test)]
#[path = "spec_report_tests.rs"]
mod spec_report_tests;
#[cfg(test)]
#[path = "spec_tests.rs"]
mod spec_tests;
#[cfg(test)]
#[path = "hunt_tests.rs"]
mod tests;
