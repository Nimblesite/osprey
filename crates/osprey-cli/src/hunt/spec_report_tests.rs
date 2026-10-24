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

/// `--json` carries exactly the spec's fields, at every nesting level.
#[test]
fn the_json_report_has_exactly_the_spec_fields() {
    let options = options::parse(&[
        "p.osp".to_string(),
        "--max-time".into(),
        "9".into(),
        "--input-bytes".into(),
        "0".into(),
    ])
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
            "incomplete"
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
    let options = options::parse(&["p.osp".to_string()]).expect("options");
    let exploration = Exploration::default();
    for (verdict, status, detail, text) in verdicts {
        let mut finding = sample(Class::Memory, Some(("p.osp", 3)), None, "m");
        finding.replay = verdict.clone();
        let scan = Scan {
            findings: vec![finding],
            incomplete: Vec::new(),
        };
        let report = Report {
            program: "p.osp",
            options: &options,
            exploration: &exploration,
            scan: &scan,
        };
        let value = report::json(&report);
        assert_eq!(value["findings"][0]["replay"], status);
        assert_eq!(value["findings"][0]["replayDetail"], detail);
        assert_eq!(report::replay_text(&verdict), text);
        assert!(report::text(&report).contains(&format!("replay   {text}")));
    }
}

/// Every byte reaches the report escaped, so a finding's input is printable.
#[test]
fn input_bytes_are_escaped_in_text_and_json() {
    let bytes = [
        0u8, 0x7f, 0xff, b'"', b'\\', b'\n', b'\t', b'\'', b'a', b' ',
    ];
    let escaped = "\\x00\\x7f\\xff\\\"\\\\\\n\\t\\'a ";
    let options = options::parse(&["p.osp".to_string()]).expect("options");
    let exploration = Exploration::default();
    let mut finding = sample(Class::Memory, Some(("p.osp", 3)), Some("r.c:1 in f"), "m");
    finding.stdin = bytes.to_vec();
    finding.files = vec![("d/x.bin".into(), bytes.to_vec())];
    let scan = Scan {
        findings: vec![finding],
        incomplete: Vec::new(),
    };
    let report = Report {
        program: "p.osp",
        options: &options,
        exploration: &exploration,
        scan: &scan,
    };
    let value = report::json(&report);
    assert_eq!(value["findings"][0]["stdin"], escaped);
    assert_eq!(value["findings"][0]["files"][0]["bytes"], escaped);
    let text = report::text(&report);
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
    let options = options::parse(&["p.osp".to_string()]).expect("options");
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
