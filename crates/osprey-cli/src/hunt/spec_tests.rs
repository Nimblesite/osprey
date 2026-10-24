//! Spec-table permutations for `osprey hunt`: every bound, table row and
//! format in docs/specs/tooling/SymbolicExecution.md driven through the code
//! that implements it, with the inputs the engine and users actually produce.
//! This file holds the command line [SYMEX-CLI]; the engine command and module
//! are in `spec_engine_tests.rs`, ktest framing in `spec_ktest_tests.rs`, and
//! findings, reports and replay in `spec_findings_tests.rs`.

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::options::{self, args, Options, HUNT_USAGE};
use std::collections::BTreeSet;

fn parsed(list: &[&str]) -> Result<Options, String> {
    options::parse(&args(list))
}

/// `parsed` over a command line written as one string.
fn parsed_line(line: &str) -> Result<Options, String> {
    parsed(&line.split_whitespace().collect::<Vec<_>>())
}

/// Every ordering of `items` (Heap's algorithm), for order-independence checks.
pub(super) fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    let mut items = items.to_vec();
    let mut counters = vec![0; items.len()];
    let mut out = vec![items.clone()];
    let mut i = 0;
    while i < items.len() {
        if counters[i] < i {
            items.swap(if i % 2 == 0 { 0 } else { counters[i] }, i);
            out.push(items.clone());
            counters[i] += 1;
            i = 0;
        } else {
            counters[i] = 0;
            i += 1;
        }
    }
    out
}

#[test]
fn the_permutation_helper_yields_every_ordering_once() {
    let all = permutations(&[0, 1, 2, 3, 4, 5]);
    assert_eq!(all.len(), 720);
    assert_eq!(all.iter().collect::<BTreeSet<_>>().len(), 720);
}

// ---- [SYMEX-CLI] -----------------------------------------------------------

/// `(flag, value, what it parses to)`: each numeric flag at, inside and past
/// its documented range — `--max-time` at least 1, `--max-memory` at least
/// 256, `--input-bytes` 0 to 4096 (the mock libc's stream cap).
const NUMERIC_CASES: &[(&str, &str, Option<u64>)] = &[
    ("--max-time", "1", Some(1)),
    ("--max-time", "60", Some(60)),
    ("--max-time", "007", Some(7)),
    ("--max-time", "+5", Some(5)),
    ("--max-time", "18446744073709551615", Some(u64::MAX)),
    ("--max-time", "0", None),
    ("--max-time", "00", None),
    ("--max-time", "18446744073709551616", None),
    ("--max-time", "-1", None),
    ("--max-time", "1.5", None),
    ("--max-time", "5s", None),
    ("--max-time", "0x10", None),
    ("--max-time", "1e3", None),
    ("--max-time", "", None),
    ("--max-time", " 5", None),
    ("--max-time", "5 ", None),
    ("--max-time", "five", None),
    ("--max-time", "--json", None),
    ("--max-time", "٣", None),
    ("--max-memory", "256", Some(256)),
    ("--max-memory", "2000", Some(2000)),
    ("--max-memory", "1048576", Some(1_048_576)),
    ("--max-memory", "255", None),
    ("--max-memory", "0", None),
    ("--max-memory", "1", None),
    ("--max-memory", "-256", None),
    ("--max-memory", "2GB", None),
    ("--input-bytes", "0", Some(0)),
    ("--input-bytes", "1", Some(1)),
    ("--input-bytes", "64", Some(64)),
    ("--input-bytes", "4095", Some(4095)),
    ("--input-bytes", "4096", Some(4096)),
    ("--input-bytes", "4097", None),
    ("--input-bytes", "8192", None),
    ("--input-bytes", "65536", None),
    ("--input-bytes", "-1", None),
    ("--input-bytes", "18446744073709551615", None),
];

fn numeric_field(options: &Options, flag: &str) -> u64 {
    match flag {
        "--max-time" => options.max_time,
        "--max-memory" => options.max_memory,
        _ => options.input_bytes,
    }
}

#[test]
fn numeric_flags_accept_exactly_their_documented_range() {
    for &(flag, text, expected) in NUMERIC_CASES {
        for order in [
            vec!["demo.osp", flag, text],
            vec![flag, text, "demo.osp"],
            vec!["--json", flag, text, "--quiet", "demo.osp"],
        ] {
            let result = parsed(&order);
            let read = result.as_ref().ok().map(|o| numeric_field(o, flag));
            assert_eq!(read, expected, "{order:?}: {result:?}");
            if expected.is_none() {
                let error = result.expect_err("out of range");
                assert!(
                    error.contains(flag) && error.contains(&format!("not {text}")),
                    "{error}"
                );
            }
        }
    }
}

#[test]
fn a_range_error_states_the_documented_bounds() {
    for (flag, range) in [
        ("--max-time", "from 1 to 18446744073709551615"),
        ("--max-memory", "from 256 to 18446744073709551615"),
        ("--input-bytes", "from 0 to 4096"),
    ] {
        let error = parsed(&["a.osp", flag, "nope"]).expect_err("not a number");
        assert!(error.contains(range), "{error}");
        assert!(error.starts_with("osprey hunt: "), "{error}");
    }
}

#[test]
fn every_value_flag_at_the_end_requires_a_value() {
    for flag in ["--max-time", "--max-memory", "--input-bytes", "--out"] {
        for before in [
            vec!["a.osp"],
            vec!["a.osp", "--json"],
            vec!["--quiet", "a.osp"],
        ] {
            let mut list = before;
            list.push(flag);
            let error = parsed(&list).expect_err("missing value");
            assert!(
                error.contains(&format!("{flag} requires a value")),
                "{error}"
            );
            assert!(error.ends_with(HUNT_USAGE), "{error}");
        }
    }
}

/// The spec's synopsis, flag by flag, in any order: the same options.
#[test]
fn every_ordering_of_every_flag_reads_the_same() {
    let groups: [&[&str]; 8] = [
        &["demo.osp"],
        &["--json"],
        &["--no-replay"],
        &["--quiet"],
        &["--max-time", "9"],
        &["--max-memory", "300"],
        &["--input-bytes", "32"],
        &["--out", "runs dir"],
    ];
    let expected = Options {
        path: "demo.osp".into(),
        max_time: 9,
        max_memory: 300,
        input_bytes: 32,
        out: Some("runs dir".into()),
        json: true,
        reports: options::ReportFormats {
            html: true,
            json: true,
        },
        replay: false,
        quiet: true,
    };
    let orders = permutations(&groups);
    assert_eq!(orders.len(), 40_320);
    for order in orders {
        let flat: Vec<&str> = order
            .iter()
            .flat_map(|group| group.iter().copied())
            .collect();
        assert_eq!(parsed(&flat).as_ref(), Ok(&expected), "{flat:?}");
    }
}

/// Every subset of the boolean flags, each in two positions.
#[test]
fn every_subset_of_switches_sets_exactly_those_switches() {
    let switches = ["--json", "--no-replay", "--quiet"];
    for mask in 0u8..8 {
        let chosen: Vec<&str> = (0..3)
            .filter(|bit| mask & (1 << *bit) != 0)
            .map(|bit| switches[bit])
            .collect();
        for path_first in [true, false] {
            let mut list = chosen.clone();
            if path_first {
                list.insert(0, "p.ospml");
            } else {
                list.push("p.ospml");
            }
            let options = parsed(&list).expect("switches");
            assert_eq!(options.json, mask & 1 != 0, "{list:?}");
            assert_eq!(options.replay, mask & 2 == 0, "{list:?}");
            assert_eq!(options.quiet, mask & 4 != 0, "{list:?}");
            assert_eq!(
                (options.max_time, options.max_memory, options.input_bytes),
                (60, 2000, 64)
            );
        }
    }
}

#[test]
fn a_repeated_flag_keeps_its_last_value_and_switches_are_idempotent() {
    let options = parsed_line(
        "--max-time 5 a.osp --max-time 7 --json --json --out x --out y --input-bytes 4096 --input-bytes 0",
    )
    .expect("repeats");
    assert_eq!(options.max_time, 7);
    assert_eq!(options.input_bytes, 0);
    assert_eq!(options.out, Some("y".into()));
    assert!(options.json);
    assert!(parsed(&["a.osp", "--max-time", "5", "--max-time", "0"]).is_err());
}

/// Designed-but-unbuilt and malformed spellings are refused, never ignored:
/// `--fn`/`--all-fns` [SYMEX-HARNESS-TYPED], `--replay-host` [SYMEX-HOSTS],
/// `--seed-file` [SYMEX-REGRESSION], `--json` with a path, `=` forms.
#[test]
fn unknown_and_unbuilt_flags_are_refused_by_name() {
    let refused = "--fn --all-fns --replay-host=windows --replay-host --seed-file \
                   --json=report.json --max-time=5 --input-bytes=8 --out=dir --posix-runtime \
                   --libc=uclibc --external-calls=all --Json -- ---json";
    for flag in refused.split_whitespace() {
        for list in [vec!["a.osp", flag], vec![flag, "a.osp"]] {
            let error = parsed(&list).expect_err("unknown flag");
            assert!(error.contains(&format!("unknown option {flag}")), "{error}");
            assert!(error.ends_with(HUNT_USAGE), "{error}");
        }
    }
}

#[test]
fn one_program_only_and_it_is_required() {
    for list in [
        vec!["a.osp", "b.osp"],
        vec!["a.osp", "--json", "b.osp"],
        vec!["a.osp", "--json", "report.json"],
        vec!["-q", "a.osp"],
    ] {
        let error = parsed(&list).expect_err("second path");
        assert!(error.contains("unexpected argument"), "{list:?}: {error}");
    }
    for list in [
        vec![],
        vec!["--json"],
        vec!["--quiet", "--no-replay", "--max-time", "3"],
    ] {
        let error = parsed(&list).expect_err("no path");
        assert!(error.contains("a source file is required"), "{error}");
    }
}

/// `<file>` is a `.osp`, an `.ospml`, a project root or its manifest, passed
/// through untouched.
#[test]
fn every_program_form_is_kept_verbatim() {
    for path in [
        "demo.osp",
        "demo.ospml",
        "project",
        "project/osprey.toml",
        "my dir/with spaces.osp",
        "./rel/../x.osp",
        "/abs/path.osp",
        "C:\\Users\\me\\x.ospml",
        "ünïcode.osp",
    ] {
        assert_eq!(parsed(&[path]).map(|o| o.path), Ok(path.to_string()));
    }
}

#[test]
fn the_usage_line_is_the_spec_synopsis() {
    assert!(HUNT_USAGE.starts_with("usage: osprey hunt <file>"));
    for part in [
        "[--max-time <seconds>]",
        "[--max-memory <mb>]",
        "[--input-bytes <n>]",
        "[--out <dir>]",
        "[--json]",
        "[--no-replay]",
        "[--quiet]",
    ] {
        assert!(HUNT_USAGE.contains(part), "{part}");
    }
}

#[test]
fn help_wins_wherever_it_appears_and_bad_input_is_usage_error() {
    for list in [
        vec!["--help"],
        vec!["-h"],
        vec!["a.osp", "--help"],
        vec!["--bogus", "-h"],
        vec!["a.osp", "b.osp", "--help"],
        vec!["--max-time", "0", "--help"],
    ] {
        assert_eq!(
            super::run(&args(&list)),
            std::process::ExitCode::SUCCESS,
            "{list:?}"
        );
    }
    for list in [
        vec![],
        vec!["--max-time"],
        vec!["a.osp", "--input-bytes", "4097"],
        vec!["a.osp", "--max-memory", "255"],
        vec!["a.osp", "--fn", "main"],
        vec!["a.osp", "b.osp"],
    ] {
        assert_eq!(
            super::run(&args(&list)),
            std::process::ExitCode::from(2),
            "{list:?}"
        );
    }
}
