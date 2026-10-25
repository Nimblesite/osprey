//! The hunt report [SYMEX-FINDINGS] [SYMEX-CLI]: one text layout for the
//! terminal and the editor's output channel, one JSON shape for CI and the
//! editor's diagnostics. Both credit the engine, KLEE.

use super::engine::Exploration;
use super::findings::{Finding, Incomplete, Replay, Scan};
use super::options::Options;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::process::ExitCode;

/// Everything one hunt produced.
#[derive(Debug)]
pub(crate) struct Report<'a> {
    pub(crate) program: &'a str,
    pub(crate) options: &'a Options,
    pub(crate) exploration: &'a Exploration,
    pub(crate) scan: &'a Scan,
    /// Exact engine invocation, raw diagnostics and source context for investigation.
    pub(crate) context: Option<Value>,
}

pub(crate) fn text(report: &Report<'_>) -> String {
    let mut out = format!(
        "Osprey Hunt: {} (symbolic execution by KLEE)\n  budget {}s, {} input bytes per stream\n  explored {} paths ({} complete), {} instructions{}\n",
        report.program,
        report.options.max_time,
        report.options.input_bytes,
        report.exploration.paths,
        report.exploration.completed,
        report.exploration.instructions,
        elapsed(&report.exploration.elapsed),
    );
    if report.scan.findings.is_empty() {
        out.push_str("findings: none\n");
    } else {
        let _ = writeln!(out, "findings: {}", report.scan.findings.len());
        for finding in &report.scan.findings {
            out.push_str(&finding_text(finding));
        }
    }
    out.push_str(&incomplete_text(&report.scan.incomplete));
    out
}

fn elapsed(elapsed: &str) -> String {
    if elapsed.is_empty() {
        String::new()
    } else {
        format!(", {elapsed}")
    }
}

fn finding_text(finding: &Finding) -> String {
    let mut out = format!("  {:<10} {}\n", finding.class.name(), place(finding));
    let _ = writeln!(out, "             {}", finding.message);
    if finding.location.is_some() {
        if let Some(frame) = &finding.runtime_frame {
            let _ = writeln!(out, "             runtime  {frame}");
        }
    }
    let _ = writeln!(
        out,
        "             stdin    \"{}\"",
        finding.stdin.escape_ascii()
    );
    for (path, bytes) in &finding.files {
        let _ = writeln!(
            out,
            "             file     {path} = \"{}\"",
            bytes.escape_ascii()
        );
    }
    let _ = writeln!(
        out,
        "             replay   {}",
        replay_text(&finding.replay)
    );
    out
}

fn place(finding: &Finding) -> String {
    super::findings::place(finding, &finding.test)
}

pub(crate) fn replay_text(replay: &Replay) -> String {
    match replay {
        Replay::NotRun => "not run".to_string(),
        Replay::Reproduced(signal) => format!("reproduced ({signal})"),
        Replay::KleeOnly => {
            "klee-only (the native build ran clean; usually mock over-approximation)".to_string()
        }
        Replay::Skipped(reason) => format!("skipped ({reason})"),
    }
}

/// Paths KLEE gave up on, grouped by reason: hundreds of paths cut at the
/// time budget are one line, not a wall.
fn incomplete_text(incomplete: &[Incomplete]) -> String {
    if incomplete.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "incomplete: {} path(s) KLEE gave up on (a budget or limit, not a bug)\n",
        incomplete.len()
    );
    let mut reasons: Vec<(&str, usize)> = Vec::new();
    for item in incomplete {
        match reasons
            .iter_mut()
            .find(|(reason, _)| *reason == item.reason)
        {
            Some((_, count)) => *count += 1,
            None => reasons.push((&item.reason, 1)),
        }
    }
    for (reason, count) in reasons {
        let _ = writeln!(out, "  {count:>6}  {}", describe_reason(reason));
    }
    out
}

/// KLEE's `.early` texts in the user's terms.
fn describe_reason(reason: &str) -> &str {
    match reason {
        "Execution halting." => "cut at the time budget (raise --max-time to finish them)",
        other => other,
    }
}

pub(crate) fn json(report: &Report<'_>) -> Value {
    json!({
        "program": report.program,
        "context": report.context,
        "engine": "KLEE",
        "budgetSeconds": report.options.max_time,
        "inputBytes": report.options.input_bytes,
        "explored": {
            "paths": report.exploration.paths,
            "completed": report.exploration.completed,
            "instructions": report.exploration.instructions,
            "elapsed": report.exploration.elapsed,
        },
        "findings": report.scan.findings.iter().map(finding_json).collect::<Vec<_>>(),
        "incomplete": report.scan.incomplete.iter().map(|item| json!({
            "test": item.test, "reason": item.reason,
        })).collect::<Vec<_>>(),
    })
}

fn finding_json(finding: &Finding) -> Value {
    let (status, detail) = match &finding.replay {
        Replay::NotRun => ("not-run", String::new()),
        Replay::Reproduced(signal) => ("reproduced", signal.clone()),
        Replay::KleeOnly => ("klee-only", String::new()),
        Replay::Skipped(reason) => ("skipped", reason.clone()),
    };
    json!({
        "test": finding.test,
        "class": finding.class.name(),
        "suffix": finding.suffix,
        "message": finding.message,
        "file": finding.location.as_ref().map(|l| l.file.clone()),
        "line": finding.location.as_ref().map(|l| l.line),
        "runtimeFrame": finding.runtime_frame,
        "stdin": finding.stdin.escape_ascii().to_string(),
        "files": finding.files.iter().map(|(path, bytes)| json!({
            "path": path, "bytes": bytes.escape_ascii().to_string(),
        })).collect::<Vec<_>>(),
        "replay": status,
        "replayDetail": detail,
    })
}

/// `1` when a finding reproduced natively, or when any finding exists and
/// replay was not run; `0` otherwise [SYMEX-CLI].
pub(crate) fn exit_code(scan: &Scan, replayed: bool) -> ExitCode {
    let failing = scan
        .findings
        .iter()
        .any(|finding| !replayed || matches!(finding.replay, Replay::Reproduced(_)));
    if failing {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::super::findings::{Class, Location};
    use super::*;

    fn sample() -> (Options, Exploration, Scan) {
        let options = super::super::options::parse(&["index.osp".to_string()]).expect("options");
        let exploration = Exploration {
            paths: 3,
            completed: 2,
            instructions: 500,
            elapsed: "00:00:01".into(),
        };
        let finding = Finding {
            test: "test000001".into(),
            class: Class::Memory,
            suffix: "ptr".into(),
            message: "memory error: out of bound pointer".into(),
            location: Some(Location {
                file: "index.osp".into(),
                line: 5,
            }),
            runtime_frame: Some("list_runtime.c:88 in osp_list_get".into()),
            stdin: b"9\n".to_vec(),
            files: vec![("data.txt".into(), b"x".to_vec())],
            replay: Replay::Reproduced("SIGSEGV".into()),
        };
        let scan = Scan {
            findings: vec![finding],
            incomplete: vec![Incomplete {
                test: "test000002".into(),
                reason: "max-time".into(),
            }],
        };
        (options, exploration, scan)
    }

    fn build<'a>(parts: &'a (Options, Exploration, Scan), program: &'static str) -> Report<'a> {
        Report {
            program,
            options: &parts.0,
            exploration: &parts.1,
            scan: &parts.2,
            context: None,
        }
    }

    #[test]
    fn text_report_names_engine_place_input_and_replay() {
        let parts = sample();
        let text = text(&build(&parts, "index.osp"));
        crate::testkit::shows(
            &text,
            &[
                "symbolic execution by KLEE",
                "explored 3 paths (2 complete), 500 instructions, 00:00:01",
                "memory     index.osp:5",
                "runtime  list_runtime.c:88 in osp_list_get",
                "stdin    \"9\\n\"",
                "file     data.txt = \"x\"",
                "replay   reproduced (SIGSEGV)",
                "incomplete: 1 path(s)",
                "     1  max-time",
            ],
        );
    }

    #[test]
    fn a_clean_hunt_says_so() {
        let mut parts = sample();
        parts.2 = Scan::default();
        let text = text(&build(&parts, "ok.osp"));
        assert!(text.contains("findings: none") && !text.contains("incomplete"));
        assert_eq!(exit_code(&parts.2, true), ExitCode::SUCCESS);
    }

    #[test]
    fn incomplete_paths_are_grouped_by_reason() {
        let mut items: Vec<Incomplete> = (0..7)
            .map(|i| Incomplete {
                test: format!("test{i}"),
                reason: "Execution halting.".into(),
            })
            .collect();
        items.push(Incomplete {
            test: "test9".into(),
            reason: "solver timeout".into(),
        });
        let text = incomplete_text(&items);
        crate::testkit::shows(
            &text,
            &[
                "incomplete: 8 path(s)",
                "     7  cut at the time budget",
                "     1  solver timeout",
            ],
        );
    }
}
