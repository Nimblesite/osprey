//! Plain-English verdicts shared by the HTML overview and finding cards.
use super::findings::{Class, Finding, Replay};
use super::report::Report;

pub(super) fn overview(report: &Report<'_>) -> (&'static str, &'static str) {
    let findings = &report.scan.findings;
    if findings
        .iter()
        .any(|f| matches!(f.replay, Replay::Reproduced(_)))
    {
        ("A crash was reproduced", "Hunt found input that also fails when your program runs normally. Start with the reproduced finding below.")
    } else if findings.iter().any(|f| f.class != Class::Unmodelled) {
        ("Findings need investigation", "The analysis reported a possible failure. Read its normal-run check before treating it as a confirmed bug.")
    } else if !findings.is_empty() {
        ("Analysis blocked by an unsupported operation", "Hunt reached an operation it cannot simulate. No program bug was demonstrated, and analysis could not continue down that path.")
    } else if !report.scan.incomplete.is_empty() || report.exploration.completed == 0 {
        ("No bugs found in an unfinished analysis", "Hunt stopped before completing the analysis. Unexplored behavior may still contain bugs.")
    } else {
        ("No bugs found in the explored paths", "The explored paths completed without a reported failure. This is limited to the chosen input size and the operations the engine can model; it is not proof of correctness.")
    }
}

pub(super) fn title(finding: &Finding) -> &'static str {
    match finding.class {
        Class::Unmodelled => "Hunt could not analyse this operation",
        Class::Memory => "An invalid memory access was reported",
        Class::Abort => "Execution stopped at an assertion or abort",
        Class::Arith => "An arithmetic operation failed",
        Class::Ub => "An unsafe runtime operation was reported",
    }
}

pub(super) fn verdict(finding: &Finding) -> &'static str {
    assessment(finding).0
}

pub(super) fn next_step(finding: &Finding) -> &'static str {
    assessment(finding).1
}

fn assessment(finding: &Finding) -> (&'static str, &'static str) {
    if finding.class == Class::Unmodelled {
        return ("Engine limitation · no program bug demonstrated", "Test this operation normally. Hunt needs an engine model for it before it can explore behavior beyond this point. Increasing the time budget will not add that support.");
    }
    match finding.replay {
        Replay::Reproduced(_) => ("Reproduced in a normal run", "Inspect this source location with the input below. A crash in safe Osprey code can indicate a compiler or runtime defect; include the For AI context when reporting it."),
        Replay::KleeOnly => ("Not reproduced in a normal run", "The normal program did not reproduce the failure. Investigate the engine model and input assumptions before changing your program."),
        Replay::NotRun => ("Not confirmed in a normal run", "Run Hunt again without --no-replay to check whether the same input also fails in a normal build."),
        Replay::Skipped(_) => ("Normal-run check unavailable", "Read the replay reason in For AI. Re-run Hunt to obtain valid recorded inputs, or check the program manually when native replay is unavailable."),
    }
}

pub(super) fn operation(finding: &Finding) -> String {
    let operation = finding
        .message
        .strip_prefix("unmodelled: ")
        .unwrap_or(&finding.message);
    match operation {
        "pipe" | "fork" | "execve" => {
            format!("Starting a process needs {operation}, which Hunt cannot simulate.")
        }
        _ => format!("The engine reported: {operation}"),
    }
}
