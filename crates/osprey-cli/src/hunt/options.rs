//! `osprey hunt` command line [SYMEX-CLI]
//! (docs/specs/tooling/SymbolicExecution.md).

use std::path::PathBuf;

pub(crate) const HUNT_USAGE: &str =
    "usage: osprey hunt <file> [--max-time <seconds>] [--max-memory <mb>] \
[--memory=gc|arc] [--input-bytes <n>] [--out <dir>] [--json] [--no-html] [--no-json] [--no-replay] [--quiet]";

/// Largest symbolic byte stream the mock libc holds (`OSP_HUNT_STREAM_MAX`).
const INPUT_BYTES_MAX: u64 = 4096;

/// What one hunt explores and how it reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    pub(crate) path: String,
    /// One backend shared by symbolic analysis and native replay.
    pub(crate) memory: &'static str,
    /// KLEE's `--max-time` budget in seconds, enforced with `--watchdog`.
    pub(crate) max_time: u64,
    /// KLEE's `--max-memory` cap in megabytes; a long single path (a whole
    /// test suite) can need more than the default.
    pub(crate) max_memory: u64,
    /// Cap on every symbolic byte stream (stdin, each opened file).
    pub(crate) input_bytes: u64,
    /// Where KLEE's output directory goes; a temporary directory when unset.
    pub(crate) out: Option<PathBuf>,
    pub(crate) json: bool,
    pub(crate) reports: ReportFormats,
    /// Run each finding on the native build to tell `reproduced` from
    /// `klee-only` [SYMEX-REPLAY].
    pub(crate) replay: bool,
    pub(crate) quiet: bool,
}

/// Independently selected report files; stdout remains a separate option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReportFormats {
    pub(crate) html: bool,
    pub(crate) json: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        path: String::new(),
        memory: "gc",
        max_time: 60,
        max_memory: 2000,
        input_bytes: 64,
        out: None,
        json: false,
        reports: ReportFormats {
            html: true,
            json: true,
        },
        replay: true,
        quiet: false,
    };
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        parse_one(&mut options, argument, &mut arguments)?;
    }
    if options.path.is_empty() {
        return Err(format!(
            "osprey hunt: a source file is required\n{HUNT_USAGE}"
        ));
    }
    Ok(options)
}

fn parse_one(
    options: &mut Options,
    argument: &str,
    rest: &mut std::slice::Iter<'_, String>,
) -> Result<(), String> {
    match argument {
        "--memory" => options.memory = memory(value(argument, rest.next())?)?,
        flag if flag.starts_with("--memory=") => {
            options.memory = memory(flag.strip_prefix("--memory=").unwrap_or_default())?;
        }
        "--json" => options.json = true,
        "--no-html" => options.reports.html = false,
        "--no-json" => options.reports.json = false,
        "--no-replay" => options.replay = false,
        "--quiet" => options.quiet = true,
        "--max-time" => options.max_time = number(argument, rest.next(), 1, u64::MAX)?,
        "--max-memory" => options.max_memory = number(argument, rest.next(), 256, u64::MAX)?,
        "--input-bytes" => {
            options.input_bytes = number(argument, rest.next(), 0, INPUT_BYTES_MAX)?;
        }
        "--out" => options.out = Some(PathBuf::from(value(argument, rest.next())?)),
        flag if flag.starts_with("--") => {
            return Err(format!("osprey hunt: unknown option {flag}\n{HUNT_USAGE}"));
        }
        path if options.path.is_empty() => options.path = path.to_string(),
        extra => {
            return Err(format!(
                "osprey hunt: unexpected argument {extra}\n{HUNT_USAGE}"
            ))
        }
    }
    Ok(())
}

fn memory(name: &str) -> Result<&'static str, String> {
    match name {
        "gc" => Ok("gc"),
        "arc" => Ok("arc"),
        _ => Err(format!(
            "osprey hunt: --memory must be gc or arc, not {name}"
        )),
    }
}

fn value<'a>(flag: &str, next: Option<&'a String>) -> Result<&'a str, String> {
    next.map(String::as_str)
        .ok_or_else(|| format!("osprey hunt: {flag} requires a value\n{HUNT_USAGE}"))
}

fn number(flag: &str, next: Option<&String>, min: u64, max: u64) -> Result<u64, String> {
    let text = value(flag, next)?;
    text.parse::<u64>()
        .ok()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| {
            format!("osprey hunt: {flag} takes a whole number from {min} to {max}, not {text}")
        })
}

/// Owned arguments from literals, for tests of the parser and its callers.
#[cfg(test)]
pub(super) fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunt_selects_gc_or_arc_and_defaults_to_gc() {
        let gc = parse(&args(&["case.osp", "--memory=gc"])).expect("GC hunt");
        let arc = parse(&args(&["case.osp", "--memory=arc"])).expect("ARC hunt");
        assert_eq!(parse(&args(&["case.osp"])).expect("GC by default"), gc);
        assert_ne!(gc, arc, "the selected backends must remain distinct");
        for mode in ["gc", "arc"] {
            let flag = format!("--memory={mode}");
            assert_eq!(
                parse(&args(&["case.osp", "--memory", mode])),
                parse(&args(&["case.osp", &flag]))
            );
        }
        assert!(parse(&args(&["case.osp", "--memory=default"])).is_err());
    }

    #[test]
    fn bad_arguments_name_the_problem() {
        assert!(parse(&args(&[])).is_err_and(|e| e.contains("source file is required")));
        assert!(parse(&args(&["--max-time", "0", "a.osp"])).is_err_and(|e| e.contains("1 to")));
        assert!(
            parse(&args(&["a.osp", "--input-bytes", "9999"])).is_err_and(|e| e.contains("4096"))
        );
        assert!(parse(&args(&["a.osp", "--out"])).is_err_and(|e| e.contains("requires a value")));
        assert!(parse(&args(&["a.osp", "--bogus"])).is_err_and(|e| e.contains("unknown option")));
        assert!(parse(&args(&["a.osp", "b.osp"])).is_err_and(|e| e.contains("unexpected argument")));
    }
}
