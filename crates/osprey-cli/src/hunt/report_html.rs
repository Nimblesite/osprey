//! Offline, escaped, plain-English Hunt reports [SYMEX-REPORT-HTML].
use super::findings::{Class, Finding, Replay};
use super::{report::Report, report_explanation as explain};
use crate::html_escape::{attr, text};

const TOKENS: &str = include_str!("../../../../website/src/css/variables.css");
const STYLE: &str = include_str!("report.css");
const DOCS: &str = "https://ospreylang.dev/spec/symbolicexecution/";

pub(super) fn render(report: &Report<'_>, json: &str) -> String {
    let (title, summary) = explain::overview(report);
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>Osprey Hunt · {title}</title><style>{TOKENS}\n{STYLE}</style></head><body>
<a class="skip" href="#main">Skip to findings</a><main id="main">
<header><a class="brand" href="https://ospreylang.dev/">Osprey <span>Hunt</span></a><a href="#for-ai">For AI ↓</a></header>
<section class="hero" aria-labelledby="outcome"><p class="eyebrow">Analysis report</p>
<h1 id="outcome">{title}</h1><p class="lead">{summary}</p><p class="program">{program}</p>{stats}<p class="counts-note">Paths are different possible executions. Normal exits exclude paths that reached a finding or were interrupted. A detected crash is a finding, not a missed test.</p></section>
<section aria-labelledby="findings"><h2 id="findings">What happened</h2>{findings}{unfinished}</section>
{ai}<footer>Symbolic execution by KLEE. <a href="{DOCS}">Osprey Hunt reference</a> · <a href="https://klee-se.org/docs/">KLEE documentation</a></footer>
</main></body></html>"##,
        program = text(report.program),
        stats = stats(report),
        findings = findings(report),
        unfinished = unfinished(report),
        ai = ai_section(report, json)
    )
}

/// Normal exits and explored paths are separate counts [SYMEX-REPORT-PATHS].
fn stats(report: &Report<'_>) -> String {
    let confirmed = report
        .scan
        .findings
        .iter()
        .filter(|f| matches!(f.replay, Replay::Reproduced(_)))
        .count();
    let blocked = report
        .scan
        .findings
        .iter()
        .filter(|f| f.class == Class::Unmodelled)
        .count();
    let paths = &report.exploration;
    format!(
        r#"<dl class="stats"><div><dt>Crashes reproduced</dt><dd>{confirmed}</dd></div>
<div><dt>Operations not supported</dt><dd>{blocked}</dd></div>
<div><dt>Paths explored</dt><dd>{}</dd></div>
<div><dt>Normal exits</dt><dd>{}</dd></div></dl>"#,
        paths.paths, paths.completed
    )
}

fn findings(report: &Report<'_>) -> String {
    if report.scan.findings.is_empty() {
        return "<div class=card><p>No failure was reported in the paths Hunt explored. Check the completion limits before drawing conclusions.</p></div>".to_string();
    }
    report
        .scan
        .findings
        .iter()
        .enumerate()
        .map(|(index, finding)| finding_card(report, index, finding))
        .collect()
}

fn finding_card(report: &Report<'_>, index: usize, finding: &Finding) -> String {
    let tone = if matches!(finding.replay, Replay::Reproduced(_)) {
        "failure"
    } else {
        "notice"
    };
    format!(
        r#"<article class="card {tone}"><p class="eyebrow">Finding {}</p><h3>{}</h3>
<p class="verdict">{}</p>{}<h4>Where it happened</h4>{}<h4>What to do next</h4><p>{}</p>{}</article>"#,
        index + 1,
        explain::title(finding),
        explain::verdict(finding),
        operation(finding),
        source(report, finding),
        explain::next_step(finding),
        inputs(finding)
    )
}

fn operation(finding: &Finding) -> String {
    if finding.class == Class::Unmodelled {
        format!("<p>{}</p>", text(&explain::operation(finding)))
    } else {
        String::new()
    }
}

fn source(report: &Report<'_>, finding: &Finding) -> String {
    let Some(location) = &finding.location else {
        return "<p>The engine could not identify an Osprey source line. See For AI for its runtime location.</p>".to_string();
    };
    let code = report
        .context
        .as_ref()
        .and_then(|value| value.get("sources"))
        .and_then(|value| value.get(&location.file))
        .and_then(|value| value.get("content"))
        .and_then(serde_json::Value::as_str);
    let excerpt = code.map_or_else(
        || "<p>Source text is unavailable on this machine.</p>".to_string(),
        |code| excerpt(code, location.line),
    );
    format!(
        "<p class=location><code>{}:{}</code></p>{excerpt}",
        text(&location.file),
        location.line
    )
}

fn excerpt(code: &str, line: u32) -> String {
    let lines: String = code
        .lines()
        .enumerate()
        .filter(|(index, _)| {
            let number = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
            number.abs_diff(line) <= 2
        })
        .map(|(index, content)| source_line(index, content, line))
        .collect();
    format!("<pre class=source tabindex=0><code>{lines}</code></pre>")
}

fn source_line(index: usize, content: &str, line: u32) -> String {
    let selected = usize::try_from(line).is_ok_and(|line| line == index + 1);
    let class = if selected { "selected" } else { "" };
    format!(
        "<span class=\"line {class}\"><span class=number>{}</span>{}</span>",
        index + 1,
        text(content)
    )
}

fn file_input(path: &str, bytes: &[u8]) -> String {
    format!(
        "<p>File: <code>{}</code></p><pre tabindex=0>{}</pre>",
        text(path),
        text(&bytes.escape_ascii().to_string())
    )
}

fn inputs(finding: &Finding) -> String {
    if finding.class == Class::Unmodelled {
        return String::new();
    }
    let files: String = finding
        .files
        .iter()
        .map(|(path, bytes)| file_input(path, bytes))
        .collect();
    format!("<h4>Input that reached this finding</h4><p>Standard input, with special bytes escaped:</p><pre tabindex=0>{}</pre>{files}",
        text(&finding.stdin.escape_ascii().to_string()))
}

fn unfinished(report: &Report<'_>) -> String {
    if report.scan.incomplete.is_empty() {
        return String::new();
    }
    format!("<aside class=card aria-labelledby=unfinished><h3 id=unfinished>{} paths were not completed</h3><p>A time limit, resource limit or engine stop prevented Hunt from finishing these paths. This does not demonstrate a bug.</p><p>Try fewer symbolic input bytes or increase the relevant budget. Exact stop reasons are in For AI.</p></aside>", report.scan.incomplete.len())
}

fn ai_section(report: &Report<'_>, json: &str) -> String {
    let download = if report.options.reports.json {
        "<a href=report.json download>Download JSON context</a>"
    } else {
        ""
    };
    format!(
        r#"<details id="for-ai" class="card"><summary>For AI <span>Full investigation context</span></summary>
<p>Copy the JSON below when asking an AI or a maintainer to investigate. It includes exact engine arguments, raw diagnostics, source text, inputs, replay verdicts and analysis limits. Local paths and source contents stay in these report files.</p>
<p>{download}</p><pre class="ai" tabindex="0" aria-label="Complete Hunt JSON context">{}</pre></details>"#,
        attr(json)
    )
}
