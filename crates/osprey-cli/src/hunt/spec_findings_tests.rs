//! Findings, the report and replay, permuted against the tables in
//! docs/specs/tooling/SymbolicExecution.md: every KLEE suffix, every stack
//! frame shape, every dedupe key, and every input pairing [SYMEX-FINDINGS]. The report and exit codes are in
//! `spec_report_tests.rs`, native replay in `spec_replay_tests.rs`.

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::findings::{self, Class, Finding, Frame, Location, Replay, Scan};
use super::ktest;
use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::PathBuf;

/// One stack to parse, the user frame it must pick and the runtime frame.
type LocationCase<'a> = (
    &'a [(&'a str, u32)],
    Option<(&'a str, u32)>,
    Option<&'a str>,
);
/// A ktest body (or none), the stdin bytes it must yield and the stream count.
type StdinCase<'a> = (Option<&'a [u8]>, &'a [u8], usize);

/// The spec's class table, row by row.
const SUFFIXES: [(&str, Class); 22] = [
    ("ptr", Class::Memory),
    ("free", Class::Memory),
    ("read_only", Class::Memory),
    ("bad_vector_access", Class::Memory),
    ("invalid_load", Class::Memory),
    ("abort", Class::Abort),
    ("assert", Class::Abort),
    ("report_error", Class::Abort),
    ("div", Class::Arith),
    ("overshift", Class::Arith),
    ("overflow", Class::Arith),
    ("implicit_truncation", Class::Ub),
    ("implicit_conversion", Class::Ub),
    ("unreachable_call", Class::Ub),
    ("missing_return", Class::Ub),
    ("nullable_attribute", Class::Ub),
    ("function_type_mismatch", Class::Ub),
    ("invalid_builtin", Class::Ub),
    ("invalid_object_size", Class::Ub),
    ("unmodelled", Class::Unmodelled),
    ("external", Class::Unmodelled),
    ("exec", Class::Unmodelled),
];

const CLASSES: [Class; 5] = [
    Class::Memory,
    Class::Abort,
    Class::Arith,
    Class::Ub,
    Class::Unmodelled,
];

/// A scratch directory removed on drop.
pub(super) struct Dir(pub(super) PathBuf);

impl Dir {
    pub(super) fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("osprey-hunt-spec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch dir");
        Self(path)
    }

    pub(super) fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, bytes).expect("fixture");
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn report_at(file: &str, line: u32, message: &str) -> String {
    format!("Error: {message}\nStack: \n\t#000000001 in osp_rt() at /rt/list_runtime.c:9\n\t#000000002 in main() at {file}:{line}\n")
}

pub(super) fn sample(
    class: Class,
    location: Option<(&str, u32)>,
    frame: Option<&str>,
    message: &str,
) -> Finding {
    Finding {
        test: format!("test-{}-{message}", class.name()),
        class,
        suffix: "ptr".into(),
        message: message.into(),
        location: location.map(|(file, line)| Location {
            file: file.into(),
            line,
        }),
        runtime_frame: frame.map(Into::into),
        stdin: Vec::new(),
        files: Vec::new(),
        replay: Replay::NotRun,
    }
}

// ---- classification --------------------------------------------------------

#[test]
fn every_suffix_in_the_spec_table_has_its_class() {
    for (suffix, class) in SUFFIXES {
        assert_eq!(Class::classify(suffix), Some(class), "{suffix}");
    }
    let names: BTreeSet<&str> = CLASSES.iter().map(|class| class.name()).collect();
    assert_eq!(
        names,
        BTreeSet::from(["memory", "abort", "arith", "ub", "unmodelled"])
    );
}

/// `.early`, `model.err`, `solver.err`, other spellings and a plain `user.err`
/// are not findings.
#[test]
fn everything_else_is_not_a_finding() {
    for suffix in [
        "model",
        "solver",
        "user",
        "early",
        "exec_error",
        "",
        "PTR",
        "Ptr",
        " ptr",
        "ptr ",
        "ptr.err",
        "err",
        "div_zero",
        "overshift_check",
        "unmodeled",
        "external_call",
        "abort2",
        "readonly",
        "free.ptr",
    ] {
        assert_eq!(Class::classify(suffix), None, "{suffix:?}");
    }
}

/// One `.err` per suffix, each at its own Osprey line: one finding each, in
/// test order, with its suffix and class.
#[test]
fn a_run_with_every_suffix_yields_every_class_once_per_line() {
    let dir = Dir::new("suffixes");
    for (n, (suffix, _)) in SUFFIXES.iter().enumerate() {
        let line = u32::try_from(n + 1).unwrap_or_default();
        dir.write(
            &format!("test{n:06}.{suffix}.err"),
            report_at("p.osp", line, suffix).as_bytes(),
        );
    }
    let scan = findings::scan(&dir.0).expect("scan");
    assert_eq!(scan.findings.len(), SUFFIXES.len());
    assert_eq!(scan.incomplete, Vec::new());
    for (n, (finding, (suffix, class))) in scan.findings.iter().zip(SUFFIXES).enumerate() {
        assert_eq!(finding.test, format!("test{n:06}"));
        assert_eq!((finding.suffix.as_str(), finding.class), (suffix, class));
        assert_eq!(
            finding.location.as_ref().map(|l| l.line),
            u32::try_from(n + 1).ok()
        );
        assert_eq!(finding.message, suffix);
    }
}

/// Files that are not `test<N>.<suffix>.err` or `.early` are KLEE's
/// bookkeeping and never findings.
#[test]
fn bookkeeping_files_are_ignored() {
    let dir = Dir::new("bookkeeping");
    for name in [
        "info",
        "run.stats",
        "run.istats",
        "assembly.ll",
        "messages.txt",
        "warnings.txt",
        "test000001.ktest",
        "foo.ptr.err",
        "test000002.err",
        "test000003.ptr.err.bak",
        "Test000004.ptr.err",
        "test000005.ptr",
        "ptr.err",
    ] {
        dir.write(name, report_at("p.osp", 1, "x").as_bytes());
    }
    let scan = findings::scan(&dir.0).expect("scan");
    assert_eq!(scan, Scan::default());
}

/// `.early` files and unknown suffixes are "incomplete" with their reason.
#[test]
fn incomplete_paths_carry_their_reason() {
    let dir = Dir::new("early");
    let reasons = [
        ("test000001.early", "max-time", "max-time"),
        (
            "test000002.early",
            "  Execution halting.\n\n",
            "Execution halting.",
        ),
        ("test000003.early", "", ""),
        (
            "test000004.model.err",
            "Error: concretized symbolic size\n",
            "concretized symbolic size",
        ),
        (
            "test000005.solver.err",
            "Error: Query timed out (resolve).\n",
            "Query timed out (resolve).",
        ),
        (
            "test000006.user.err",
            "Error: invalid klee_assume call\n",
            "invalid klee_assume call",
        ),
        ("test000007.a.b.err", "garbage", "unknown error"),
    ];
    for (name, text, _) in reasons {
        dir.write(name, text.as_bytes());
    }
    let scan = findings::scan(&dir.0).expect("scan");
    assert!(scan.findings.is_empty(), "{:?}", scan.findings);
    let got: Vec<(&str, &str)> = scan
        .incomplete
        .iter()
        .map(|i| (i.test.as_str(), i.reason.as_str()))
        .collect();
    let want: Vec<(&str, &str)> = reasons
        .iter()
        .map(|(name, _, reason)| (name.split('.').next().unwrap_or_default(), *reason))
        .collect();
    assert_eq!(got, want);
}

/// A `user.err` is unmodelled only when KLEE says the call left the module,
/// and only then is it renamed after the functions from its messages.
#[test]
fn external_calls_are_named_from_messages_and_warnings() {
    let refused = "Error: external calls disallowed (in particular, ...)\nStack: \n\t#1 in main() at p.osp:4\n";
    for (messages, warnings, expected) in [
        ("", "", "external calls disallowed (in particular, ...)"),
        (
            "KLEE: WARNING: Disallowed call to external function: zeta\n",
            "KLEE: WARNING: Disallowed call to external function:   alpha  \nKLEE: WARNING: Disallowed call to external function: zeta\n",
            "external call outside the module: alpha, zeta",
        ),
        ("", "KLEE: WARNING: Disallowed call to external function: __osprey_coro_new\n", "external call outside the module: __osprey_coro_new"),
        ("KLEE: WARNING: undefined reference to function: f\n", "", "external calls disallowed (in particular, ...)"),
    ] {
        let dir = Dir::new("externals");
        dir.write("test000001.user.err", refused.as_bytes());
        dir.write("test000002.unmodelled.err", b"Error: unmodelled: fork\n");
        dir.write("messages.txt", messages.as_bytes());
        dir.write("warnings.txt", warnings.as_bytes());
        let scan = findings::scan(&dir.0).expect("scan");
        assert_eq!(scan.findings.len(), 2);
        assert_eq!(scan.findings[0].class, Class::Unmodelled);
        assert_eq!(scan.findings[0].suffix, "user");
        assert_eq!(scan.findings[0].message, expected);
        assert_eq!(scan.findings[1].message, "unmodelled: fork", "only KLEE's refusal is renamed");
    }
}

// ---- stack frames and locations ---------------------------------------------

/// `(stack line, the frame it is)`: KLEE's shapes, Windows paths, spaces,
/// `at` inside names, and every malformed variant.
#[test]
fn every_stack_line_shape_parses_or_is_skipped() {
    let frame = |function: &str, file: &str, line: u32| {
        Some(Frame {
            function: function.into(),
            file: file.into(),
            line,
        })
    };
    let cases = [
        (
            "\t#000000001 in main() at prog.osp:3",
            frame("main", "prog.osp", 3),
        ),
        (
            "#1 in f(i64, ptr) at /a b/c.c:10",
            frame("f(i64, ptr)", "/a b/c.c", 10),
        ),
        ("  #0 in g at x.ospml:7", frame("g", "x.ospml", 7)),
        (
            "#0 in h() at C:\\src\\x.osp:12",
            frame("h", "C:\\src\\x.osp", 12),
        ),
        ("#0 in f at g at x.osp:2", frame("f at g", "x.osp", 2)),
        (
            "#0 in k() at x.osp:4294967295",
            frame("k", "x.osp", u32::MAX),
        ),
        ("#0 in h() at x.osp", None),
        ("#0 in h() at x.osp:", None),
        ("#0 in h() at x.osp:-1", None),
        ("#0 in h() at x.osp:1.5", None),
        ("#0 in h() at x.osp:4294967296", None),
        ("#0 h() at x.osp:1", None),
        ("in h() at x.osp:1", None),
        ("#0 in h()", None),
        ("", None),
    ];
    for (line, expected) in cases {
        let (_, frames) = findings::parse_err(line);
        assert_eq!(frames.first().cloned(), expected, "{line:?}");
    }
}

#[test]
fn the_message_is_the_first_error_line_trimmed() {
    for (text, message) in [
        ("Error: one\nError: two\n", "one"),
        ("noise\nError:   spaced   \n", "spaced"),
        ("error: lower\n", "unknown error"),
        (" Error: indented\n", "unknown error"),
        ("Error:no space\n", "unknown error"),
        ("", "unknown error"),
        ("Error: \n", ""),
    ] {
        assert_eq!(findings::parse_err(text).0, message, "{text:?}");
    }
}

/// The Osprey location is the first `.osp`/`.ospml`/`.py` frame not in the prelude;
/// the runtime frame is the first `.c` frame.
#[test]
fn the_location_is_the_top_user_frame_and_the_runtime_frame_the_top_c_frame() {
    let prelude = osprey_types::PRELUDE_LINE_BASE;
    let stack = |frames: &[(&str, u32)]| {
        let mut lines = String::from("Error: e\nStack: \n");
        for (n, (file, line)) in frames.iter().enumerate() {
            let _ = writeln!(lines, "\t#{n} in f{n}() at {file}:{line}");
        }
        lines
    };
    let cases: [LocationCase<'_>; 10] = [
        (&[("a.osp", 1)], Some(("a.osp", 1)), None),
        (&[("main.py", 7)], Some(("main.py", 7)), None),
        (
            &[("r.c", 5), ("a.osp", 2), ("b.osp", 3)],
            Some(("a.osp", 2)),
            Some("r.c:5 in f0"),
        ),
        (
            &[("a.ospml", 9), ("r.c", 5)],
            Some(("a.ospml", 9)),
            Some("r.c:5 in f1"),
        ),
        (
            &[("p.osp", prelude + 1), ("a.osp", 4)],
            Some(("a.osp", 4)),
            None,
        ),
        (
            &[("p.osp", prelude), ("a.osp", 4)],
            Some(("p.osp", prelude)),
            None,
        ),
        (&[("p.osp", u32::MAX)], None, None),
        (
            &[("A.OSP", 1), ("x.h", 2), ("noext", 3), ("x.osp.c", 4)],
            None,
            Some("x.osp.c:4 in f3"),
        ),
        (&[("r.c", 1), ("s.c", 2)], None, Some("r.c:1 in f0")),
        (&[], None, None),
    ];
    for (frames, location, runtime) in cases {
        let found = findings::finding("test1", "ptr", Class::Memory, &stack(frames), None);
        let want = location.map(|(file, line)| Location {
            file: file.into(),
            line,
        });
        assert_eq!(found.location, want, "{frames:?}");
        assert_eq!(found.runtime_frame.as_deref(), runtime, "{frames:?}");
    }
}

// ---- inputs ----------------------------------------------------------------

#[test]
fn a_finding_takes_stdin_and_files_from_its_ktest_or_none() {
    let full = ktest::encode(&[
        ("stdin", b"42\nrest"),
        ("stdin.len", &3i32.to_le_bytes()),
        ("cfg/a.txt", b"abc"),
        ("cfg/a.txt.ok", &1i32.to_le_bytes()),
        ("entropy.0", &[7; 8]),
        ("clock.0", &[0; 8]),
    ]);
    let cases: [StdinCase<'_>; 4] = [
        (Some(full.as_slice()), b"42\n", 1),
        (None, b"", 0),
        (Some(&b"garbage"[..]), b"", 0),
        (Some(&full[..full.len() - 1]), b"", 0),
    ];
    for (ktest_bytes, stdin, files) in cases {
        let dir = Dir::new("inputs");
        dir.write(
            "test000001.ptr.err",
            report_at("p.osp", 2, "boom").as_bytes(),
        );
        if let Some(bytes) = ktest_bytes {
            dir.write("test000001.ktest", bytes);
        }
        let scan = findings::scan(&dir.0).expect("scan");
        assert_eq!(
            scan.findings.len(),
            1,
            "a bad ktest still leaves the finding"
        );
        assert_eq!(scan.findings[0].stdin, stdin);
        assert_eq!(scan.findings[0].files.len(), files);
    }
}

// ---- dedupe ------------------------------------------------------------------

/// Every class at every location, each reached twice: one finding per
/// (class, place), the first path kept.
#[test]
fn dedupe_keeps_one_per_class_and_place_over_the_full_matrix() {
    let places = [("a.osp", 1), ("a.osp", 2), ("b.osp", 1)];
    let mut all = Vec::new();
    for round in ["first", "second"] {
        for class in CLASSES {
            for place in places {
                let frame = format!("r.c:{round} in f");
                all.push(sample(class, Some(place), Some(frame.as_str()), round));
            }
        }
    }
    let kept = findings::dedupe(all);
    assert_eq!(kept.len(), CLASSES.len() * places.len());
    assert!(kept.iter().all(|finding| finding.message == "first"));
}

/// The key falls back from location to runtime frame to message.
#[test]
fn the_dedupe_key_falls_back_in_spec_order() {
    let abort = |at: Option<(&str, u32)>, frame: Option<&str>, message: &str| {
        sample(Class::Abort, at, frame, message)
    };
    let a1 = Some(("a.osp", 1));
    let pairs = [
        (abort(a1, Some("x"), "m1"), abort(a1, Some("y"), "m2"), 1),
        (
            abort(None, Some("r.c:1 in f"), "m1"),
            abort(None, Some("r.c:1 in f"), "m2"),
            1,
        ),
        (
            abort(None, Some("r.c:1 in f"), "m"),
            abort(None, Some("r.c:2 in f"), "m"),
            2,
        ),
        (abort(None, None, "m"), abort(None, None, "m"), 1),
        (abort(None, None, "m"), abort(None, None, "n"), 2),
        (abort(a1, None, "m"), sample(Class::Arith, a1, None, "m"), 2),
        (
            abort(a1, None, "m"),
            abort(Some(("b.osp", 1)), None, "m"),
            2,
        ),
    ];
    for (first, second, count) in pairs {
        assert_eq!(
            findings::dedupe(vec![first.clone(), second.clone()]).len(),
            count,
            "{first:?} / {second:?}"
        );
        assert_eq!(findings::dedupe(vec![second, first]).len(), count);
    }
}
