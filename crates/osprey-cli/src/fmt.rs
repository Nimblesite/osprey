//! `osprey fmt` — the source formatter command.
//!
//! Formats `.osp` (Default flavor) and `.ospml` (ML flavor) sources, picking the
//! flavor from each file's extension/marker unless `--flavor` overrides it. By
//! default files are rewritten in place; `--check` reports which files would
//! change (and exits non-zero) without touching them, `--stdout` prints the
//! formatted text instead of writing, and a single `-` path formats stdin to
//! stdout. The heavy lifting — and the meaning-preserving guarantee — lives in
//! the shared `osprey-fmt` crate, so the CLI and the editor format identically.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use osprey_syntax::Flavor;

const USAGE: &str = "usage: osprey fmt [--check | --stdout] [--flavor default|ml] [--quiet] \
<path...>\n       osprey fmt -   (format stdin to stdout)";

/// The parsed `fmt` invocation.
#[derive(Debug, Default)]
struct FmtArgs {
    paths: Vec<String>,
    check: bool,
    stdout: bool,
    quiet: bool,
    flavor: Option<Flavor>,
}

/// What happened across all processed files, collapsed into an exit code.
#[derive(Debug, Default)]
struct Tally {
    had_error: bool,
    needs_format: bool,
}

/// Entry point for `osprey fmt`; `args` excludes the `fmt` subcommand word.
pub fn run(args: &[String]) -> ExitCode {
    let parsed = match parse(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    if parsed.paths == ["-"] {
        return format_stdin(&parsed);
    }
    let mut tally = Tally::default();
    for path in collect_files(&parsed.paths) {
        process_file(&path, &parsed, &mut tally);
    }
    exit_code(&tally)
}

/// Parse `fmt`'s flags and positional paths.
fn parse(args: &[String]) -> Result<FmtArgs, String> {
    let mut out = FmtArgs::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--check" => out.check = true,
            "--stdout" => out.stdout = true,
            "--quiet" => out.quiet = true,
            "--flavor" => {
                let value = it
                    .next()
                    .ok_or_else(|| format!("--flavor requires a value (default|ml)\n{USAGE}"))?;
                out.flavor = Some(super::parse_flavor(value)?);
            }
            flag if flag.starts_with("--flavor=") => {
                out.flavor = Some(super::parse_flavor(
                    flag.strip_prefix("--flavor=").unwrap_or_default(),
                )?);
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}\n{USAGE}")),
            path => out.paths.push(path.to_owned()),
        }
    }
    if out.paths.is_empty() {
        return Err(USAGE.to_owned());
    }
    Ok(out)
}

/// Expand the given paths into a flat list of source files, recursing into
/// directories for `.osp` and `.ospml` files.
fn collect_files(paths: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        let candidate = PathBuf::from(path);
        if candidate.is_dir() {
            collect_dir(&candidate, &mut out);
        } else {
            out.push(candidate);
        }
    }
    out
}

fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dir(&path, out);
        } else if is_osprey_source(&path) {
            out.push(path);
        }
    }
}

fn is_osprey_source(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "osp" || e == "ospml")
}

/// Format a single file according to the parsed options, recording the outcome.
fn process_file(path: &Path, args: &FmtArgs, tally: &mut Tally) {
    let display = path.display();
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("error: cannot read {display}: {e}");
            tally.had_error = true;
            return;
        }
    };
    match format(&source, &path.to_string_lossy(), args.flavor) {
        Ok(formatted) => emit(&formatted, &source, path, args, tally),
        Err(errors) => {
            for error in errors {
                eprintln!("{display}: {error}");
            }
            tally.had_error = true;
        }
    }
}

/// Format `source`, choosing the flavor from the explicit flag or the path.
fn format(source: &str, key: &str, flavor: Option<Flavor>) -> Result<String, Vec<String>> {
    match flavor {
        Some(flavor) => osprey_fmt::format_source(source, flavor),
        None => osprey_fmt::format_for_path(key, source),
    }
}

/// Apply the formatted text: print to stdout, list under `--check`, or rewrite
/// the file in place, updating the tally.
fn emit(formatted: &str, source: &str, path: &Path, args: &FmtArgs, tally: &mut Tally) {
    let display = path.display();
    if args.stdout {
        print!("{formatted}");
        return;
    }
    if formatted == source {
        return;
    }
    if args.check {
        println!("would reformat {display}");
        tally.needs_format = true;
        return;
    }
    match std::fs::write(path, formatted) {
        Ok(()) => {
            if !args.quiet {
                println!("formatted {display}");
            }
        }
        Err(e) => {
            eprintln!("error: cannot write {display}: {e}");
            tally.had_error = true;
        }
    }
}

/// Read stdin, format it (defaulting to the Default flavor), and print it.
fn format_stdin(args: &FmtArgs) -> ExitCode {
    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("error: cannot read stdin: {e}");
        return ExitCode::from(2);
    }
    match osprey_fmt::format_source(&source, args.flavor.unwrap_or(Flavor::Default)) {
        Ok(formatted) => {
            print!("{formatted}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("stdin: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn exit_code(tally: &Tally) -> ExitCode {
    if tally.had_error || tally.needs_format {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
