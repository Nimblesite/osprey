//! The hunt report and exit codes permuted against
//! docs/specs/tooling/SymbolicExecution.md: every JSON field at every level,
//! every replay verdict, escaped input bytes, the place fallback and every
//! mix of verdicts [SYMEX-FINDINGS] [SYMEX-CLI].

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::engine::Exploration;
use super::findings::{Class, Finding, Incomplete, Replay, Scan};
use super::options;
use super::report::{self, Report};
use super::spec_findings_tests::sample;
use std::collections::BTreeSet;
use std::process::ExitCode;

fn keys(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(ToString::to_string).collect()
}

/// The report over one finding under default options, as JSON and as text.
fn rendered(finding: Finding) -> (serde_json::Value, String) {
    let options = options::parse(&options::args(&["p.osp"])).expect("options");
    let exploration = Exploration::default();
    let scan = Scan {
        findings: vec![finding],
        incomplete: Vec::new(),
    };
    let report = Report {
        program: "p.osp",
        options: &options,
        exploration: &exploration,
        scan: &scan,
        context: None,
    };
    (report::json(&report), report::text(&report))
}

/// `--json` carries exactly the spec's fields, at every nesting level.
#[test]
fn the_json_report_has_exactly_the_spec_fields() {
    let options = options::parse(&options::args(&[
        "p.osp",
        "--max-time",
        "9",
        "--input-bytes",
        "0",
    ]))
    .expect("options");
    let exploration = Exploration {
        paths: 2,
        completed: 1,
        instructions: 3,
        elapsed: String::new(),
    };
    let mut finding = sample(Class::Ub, None, None, "m");
    finding.files = vec![("in.txt".into(), b"\0\xff".to_vec())];
    let scan = Scan {
        findings: vec![finding],
        incomplete: vec![Incomplete {
            test: "t".into(),
            reason: "r".into(),
        }],
    };
    let value = report::json(&Report {
        program: "p.osp",
        options: &options,
        exploration: &exploration,
        scan: &scan,
        context: None,
    });
    assert_eq!(
        keys(&value),
        set(&[
            "program",
            "engine",
            "budgetSeconds",
            "inputBytes",
            "explored",
            "findings",
            "incomplete",
            "context"
        ])
    );
    assert_eq!(
        keys(&value["explored"]),
        set(&["paths", "completed", "instructions", "elapsed"])
    );
    assert_eq!(
        keys(&value["findings"][0]),
        set(&[
            "test",
            "class",
            "suffix",
            "message",
            "file",
            "line",
            "runtimeFrame",
            "stdin",
            "files",
            "replay",
            "replayDetail"
        ])
    );
    assert_eq!(
        keys(&value["findings"][0]["files"][0]),
        set(&["path", "bytes"])
    );
    assert_eq!(keys(&value["incomplete"][0]), set(&["test", "reason"]));
    assert_eq!(
        (
            value["budgetSeconds"].as_u64(),
            value["inputBytes"].as_u64()
        ),
        (Some(9), Some(0))
    );
    assert!(value["findings"][0]["file"].is_null() && value["findings"][0]["line"].is_null());
    assert!(value["findings"][0]["runtimeFrame"].is_null());
    assert_eq!(value["findings"][0]["files"][0]["bytes"], "\\x00\\xff");
    assert_eq!(value["engine"], "KLEE");
}

#[test]
fn every_replay_verdict_has_its_json_status_and_text() {
    let verdicts = [
        (Replay::NotRun, "not-run", "", "not run"),
        (
            Replay::Reproduced("SIGSEGV".into()),
            "reproduced",
            "SIGSEGV",
            "reproduced (SIGSEGV)",
        ),
        (
            Replay::KleeOnly,
            "klee-only",
            "",
            "klee-only (the native build ran clean; usually mock over-approximation)",
        ),
        (
            Replay::Skipped("why not".into()),
            "skipped",
            "why not",
            "skipped (why not)",
        ),
    ];
    for (verdict, status, detail, text) in verdicts {
        let mut finding = sample(Class::Memory, Some(("p.osp", 3)), None, "m");
        finding.replay = verdict.clone();
        let (value, rendered_text) = rendered(finding);
        assert_eq!(value["findings"][0]["replay"], status);
        assert_eq!(value["findings"][0]["replayDetail"], detail);
        assert_eq!(report::replay_text(&verdict), text);
        assert!(rendered_text.contains(&format!("replay   {text}")));
    }
}

/// Every byte reaches the report escaped, so a finding's input is printable.
#[test]
fn input_bytes_are_escaped_in_text_and_json() {
    let bytes = [
        0u8, 0x7f, 0xff, b'"', b'\\', b'\n', b'\t', b'\'', b'a', b' ',
    ];
    let escaped = "\\x00\\x7f\\xff\\\"\\\\\\n\\t\\'a ";
    let mut finding = sample(Class::Memory, Some(("p.osp", 3)), Some("r.c:1 in f"), "m");
    finding.stdin = bytes.to_vec();
    finding.files = vec![("d/x.bin".into(), bytes.to_vec())];
    let (value, text) = rendered(finding);
    assert_eq!(value["findings"][0]["stdin"], escaped);
    assert_eq!(value["findings"][0]["files"][0]["bytes"], escaped);
    assert!(text.contains(&format!("stdin    \"{escaped}\"")), "{text}");
    assert!(
        text.contains(&format!("file     d/x.bin = \"{escaped}\"")),
        "{text}"
    );
}

/// The text names the place by location, else runtime frame, else test; the
/// runtime line appears only under a located finding.
#[test]
fn the_text_place_falls_back_like_the_dedupe_key() {
    let options = options::parse(&options::args(&["p.osp"])).expect("options");
    let exploration = Exploration::default();
    for (location, frame, place, runtime_line) in [
        (Some(("p.osp", 3)), Some("r.c:1 in f"), "p.osp:3", true),
        (Some(("p.osp", 3)), None, "p.osp:3", false),
        (None, Some("r.c:1 in f"), "r.c:1 in f", false),
        (None, None, "test-memory-m", false),
    ] {
        let scan = Scan {
            findings: vec![sample(Class::Memory, location, frame, "m")],
            incomplete: Vec::new(),
        };
        let text = report::text(&Report {
            program: "p.osp",
            options: &options,
            exploration: &exploration,
            scan: &scan,
            context: None,
        });
        assert!(text.contains(&format!("  memory     {place}\n")), "{text}");
        assert_eq!(text.contains("runtime  "), runtime_line, "{text}");
        assert!(text.contains("findings: 1\n"), "{text}");
    }
}

// ---- exit codes ----------------------------------------------------------------

/// Every subset of replay verdicts, with and without replay: `1` when one
/// reproduced, or when any finding exists without replay; `0` otherwise.
#[test]
fn the_exit_code_matches_the_spec_for_every_mix_of_verdicts() {
    let verdicts = [
        Replay::NotRun,
        Replay::Reproduced("SIGABRT".into()),
        Replay::KleeOnly,
        Replay::Skipped("s".into()),
    ];
    for mask in 0u8..16 {
        let findings: Vec<Finding> = verdicts
            .iter()
            .enumerate()
            .filter(|(bit, _)| mask & (1 << *bit) != 0)
            .map(|(_, verdict)| Finding {
                replay: verdict.clone(),
                ..sample(Class::Abort, None, None, "m")
            })
            .collect();
        let reproduced = mask & 2 != 0;
        let scan = Scan {
            findings,
            incomplete: Vec::new(),
        };
        for replayed in [true, false] {
            let failing = if replayed { reproduced } else { mask != 0 };
            let want = if failing {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
            assert_eq!(
                report::exit_code(&scan, replayed),
                want,
                "mask {mask:04b} replayed {replayed}"
            );
        }
    }
}

#[test]
fn html_verdicts_distinguish_reproduced_unconfirmed_and_engine_only_findings() {
    for (class, verdict, expected) in [
        (
            Class::Memory,
            Replay::Reproduced("SIGSEGV".into()),
            "Reproduced in a normal run",
        ),
        (
            Class::Memory,
            Replay::KleeOnly,
            "Not reproduced in a normal run",
        ),
        (
            Class::Arith,
            Replay::NotRun,
            "Not confirmed in a normal run",
        ),
        (
            Class::Unmodelled,
            Replay::Skipped("unsupported".into()),
            "no program bug demonstrated",
        ),
    ] {
        let mut finding = sample(class, Some(("p.osp", 3)), None, "message");
        finding.replay = verdict;
        assert!(super::report_explanation::verdict(&finding).contains(expected));
    }
}

#[test]
fn saved_reports_fail_loudly_when_the_output_directory_is_not_writable() {
    let dir = super::spec_findings_tests::Dir::new("report-write-error");
    dir.write("blocked", b"this is a file, not a directory");
    let path = dir.0.join("blocked").display().to_string();
    let options = options::parse(&options::args(&["p.osp", "--out", &path])).expect("options");
    let scan = Scan::default();
    let report = Report {
        program: "p.osp",
        options: &options,
        exploration: &Exploration::default(),
        scan: &scan,
        context: None,
    };
    let error =
        super::report_files::write(&report, "{}").expect_err("must not swallow write error");
    assert!(error.contains("cannot save Hunt report"), "{error}");
}

fn html_for_scan(scan: &Scan, completed: u64) -> String {
    let mut options = options::parse(&options::args(&["p.osp"])).expect("options");
    options.reports.json = false;
    super::report_html::render(
        &Report {
            program: "p.osp",
            options: &options,
            exploration: &Exploration {
                completed,
                ..Exploration::default()
            },
            scan,
            context: None,
        },
        "{}",
    )
}

#[test]
fn html_never_confuses_finished_unfinished_and_reproduced_results() {
    let mut scan = Scan::default();
    assert!(html_for_scan(&scan, 0).contains("No bugs found in an unfinished analysis"));
    assert!(html_for_scan(&scan, 1).contains("No bugs found in the explored paths"));
    let options = options::parse(&options::args(&["p.osp"])).expect("options");
    let partial = Report { program: "p.osp", options: &options,
        exploration: &Exploration { paths: 3, completed: 1, ..Exploration::default() }, scan: &scan, context: None };
    assert!(super::report_html::render(&partial, "{}").contains("No bugs found in an unfinished analysis"));
    scan.incomplete.push(Incomplete {
        test: "cut".into(),
        reason: "time".into(),
    });
    assert!(html_for_scan(&scan, 1).contains("1 paths were not completed"));
    scan.findings
        .push(sample(Class::Memory, None, None, "invalid"));
    assert!(html_for_scan(&scan, 1).contains("Findings need investigation"));
    if let Some(finding) = scan.findings.first_mut() {
        finding.replay = Replay::Reproduced("SIGSEGV".into());
    }
    let html = html_for_scan(&scan, 1);
    assert!(html.contains("A crash was reproduced"));
    assert!(html.contains("could not identify an Osprey source line"));
    assert!(!html.contains("Download JSON context"));
}
