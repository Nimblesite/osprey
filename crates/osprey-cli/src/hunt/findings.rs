//! Findings [SYMEX-FINDINGS] (docs/specs/tooling/SymbolicExecution.md):
//! KLEE's `test<N>.<suffix>.err` reports, classified, located in Osprey
//! source through the debug metadata, paired with their `.ktest` inputs and
//! deduplicated by class and top Osprey frame.

use super::ktest::{self, Ktest};
use osprey_types::is_prelude_line;
use std::path::Path;

/// What kind of defect a KLEE error suffix denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    Memory,
    Abort,
    Arith,
    Ub,
    Unmodelled,
}

impl Class {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Abort => "abort",
            Self::Arith => "arith",
            Self::Ub => "ub",
            Self::Unmodelled => "unmodelled",
        }
    }

    /// KLEE's own suffixes (TerminationTypes.h, `klee_div_zero_check.c`,
    /// `klee_overshift_check.c`); anything else is not a finding.
    pub(crate) fn classify(suffix: &str) -> Option<Self> {
        match suffix {
            "ptr" | "free" | "read_only" | "bad_vector_access" | "invalid_load" => {
                Some(Self::Memory)
            }
            "abort" | "assert" | "report_error" => Some(Self::Abort),
            "div" | "overshift" | "overflow" => Some(Self::Arith),
            "implicit_truncation"
            | "implicit_conversion"
            | "unreachable_call"
            | "missing_return"
            | "nullable_attribute"
            | "function_type_mismatch"
            | "invalid_builtin"
            | "invalid_object_size" => Some(Self::Ub),
            "unmodelled" | "external" | "exec" => Some(Self::Unmodelled),
            _ => None,
        }
    }
}

/// An Osprey source position from the debug metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Location {
    pub(crate) file: String,
    pub(crate) line: u32,
}

/// What the native run of the finding's input did [SYMEX-REPLAY].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Replay {
    NotRun,
    Reproduced(String),
    KleeOnly,
    Skipped(String),
}

/// One deduplicated defect report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) test: String,
    pub(crate) class: Class,
    pub(crate) suffix: String,
    pub(crate) message: String,
    pub(crate) location: Option<Location>,
    pub(crate) runtime_frame: Option<String>,
    pub(crate) stdin: Vec<u8>,
    pub(crate) files: Vec<(String, Vec<u8>)>,
    pub(crate) replay: Replay,
}

/// A path KLEE gave up on: not a bug, but not a clean bill either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Incomplete {
    pub(crate) test: String,
    pub(crate) reason: String,
}

/// Everything read from one KLEE output directory.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Scan {
    pub(crate) findings: Vec<Finding>,
    pub(crate) incomplete: Vec<Incomplete>,
}

/// One frame of a KLEE stack line: `#000000024 in main() at t.c:5`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) function: String,
    pub(crate) file: String,
    pub(crate) line: u32,
}

pub(crate) fn scan(directory: &Path) -> Result<Scan, String> {
    let mut names = std::fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    let klee_said = ["messages.txt", "warnings.txt"]
        .iter()
        .map(|file| std::fs::read_to_string(directory.join(file)).unwrap_or_default())
        .collect::<String>();
    let externals = external_calls(&klee_said);
    let mut scan = Scan::default();
    for name in names {
        record(&mut scan, directory, &name, &externals)?;
    }
    scan.findings = dedupe(scan.findings);
    Ok(scan)
}

/// The functions KLEE refused to call outside the module, from its
/// `messages.txt` and `warnings.txt` (`KLEE: WARNING: Disallowed call to
/// external function: f`); the `.err` report itself does not name them.
pub(crate) fn external_calls(messages: &str) -> Vec<String> {
    let mut names: Vec<String> = messages
        .lines()
        .filter_map(|line| line.split("Disallowed call to external function: ").nth(1))
        .map(|name| name.trim().to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn record(
    scan: &mut Scan,
    directory: &Path,
    name: &str,
    externals: &[String],
) -> Result<(), String> {
    let read = |name: &str| {
        std::fs::read_to_string(directory.join(name))
            .map_err(|error| format!("cannot read {name}: {error}"))
    };
    if let Some(test) = name.strip_suffix(".early") {
        let reason = read(name)?.trim().to_string();
        scan.incomplete.push(Incomplete {
            test: test.to_string(),
            reason,
        });
        return Ok(());
    }
    let Some((test, suffix)) = split_error_name(name) else {
        return Ok(());
    };
    let report = read(name)?;
    match Class::classify(suffix).or_else(|| classify_user_error(suffix, &report)) {
        Some(class) => {
            let inputs = inputs(directory, test);
            let mut found = finding(test, suffix, class, &report, inputs.as_ref());
            if inputs.is_none() {
                found.replay = Replay::Skipped("concrete KTEST input is missing or invalid; no input was invented for native replay".to_string());
            }
            name_externals(&mut found, externals);
            scan.findings.push(found);
        }
        None => scan.incomplete.push(Incomplete {
            test: test.to_string(),
            reason: parse_err(&report).0,
        }),
    }
    Ok(())
}

/// KLEE files a call it may not make under `--external-calls=none` as a
/// `user` error; that is the engine's model stopping, not the program's bug.
fn classify_user_error(suffix: &str, report: &str) -> Option<Class> {
    (suffix == "user" && parse_err(report).0.starts_with("external calls disallowed"))
        .then_some(Class::Unmodelled)
}

/// KLEE's "external calls disallowed" names no function; the names come from
/// its messages, so the report says what the module lacks.
fn name_externals(finding: &mut Finding, externals: &[String]) {
    if finding.message.starts_with("external calls disallowed") && !externals.is_empty() {
        finding.message = format!("external call outside the module: {}", externals.join(", "));
    }
}

/// `test000001.ptr.err` → (`test000001`, `ptr`).
fn split_error_name(name: &str) -> Option<(&str, &str)> {
    let stem = name.strip_suffix(".err")?;
    let (test, suffix) = stem.split_once('.')?;
    test.starts_with("test").then_some((test, suffix))
}

fn inputs(directory: &Path, test: &str) -> Option<Ktest> {
    let bytes = std::fs::read(directory.join(format!("{test}.ktest"))).ok()?;
    ktest::parse(&bytes).ok()
}

/// The `Error:` line and the stack frames of a `.err` report.
pub(crate) fn parse_err(text: &str) -> (String, Vec<Frame>) {
    let message = text
        .lines()
        .find_map(|line| line.strip_prefix("Error: "))
        .unwrap_or("unknown error")
        .trim()
        .to_string();
    let frames = text.lines().filter_map(frame).collect();
    (message, frames)
}

fn frame(line: &str) -> Option<Frame> {
    let rest = line.trim_start().strip_prefix('#')?;
    let (_, rest) = rest.split_once(" in ")?;
    let (function, rest) = rest.rsplit_once(" at ")?;
    let (file, line) = rest.trim().rsplit_once(':')?;
    Some(Frame {
        function: function.trim_end_matches("()").to_string(),
        file: file.to_string(),
        line: line.parse().ok()?,
    })
}

pub(crate) fn finding(
    test: &str,
    suffix: &str,
    class: Class,
    report: &str,
    inputs: Option<&Ktest>,
) -> Finding {
    let (message, frames) = parse_err(report);
    let osprey = frames.iter().find(|frame| is_user_source(frame));
    let runtime = frames.iter().find(|frame| extension(&frame.file) == "c");
    Finding {
        test: test.to_string(),
        class,
        suffix: suffix.to_string(),
        message,
        location: osprey.map(|frame| Location {
            file: frame.file.clone(),
            line: frame.line,
        }),
        runtime_frame: runtime
            .map(|frame| format!("{}:{} in {}", frame.file, frame.line, frame.function)),
        stdin: inputs
            .and_then(|ktest| ktest.stream("stdin"))
            .unwrap_or_default(),
        files: inputs.map(Ktest::files).unwrap_or_default(),
        replay: Replay::NotRun,
    }
}

/// An Osprey frame in the user's own code: prelude functions carry prelude
/// line numbers and are reported through their caller.
fn is_user_source(frame: &Frame) -> bool {
    matches!(extension(&frame.file), "osp" | "ospml" | "py") && !is_prelude_line(frame.line)
}

fn extension(file: &str) -> &str {
    file.rsplit_once('.').map_or("", |(_, extension)| extension)
}

/// One finding per class and top Osprey frame (or top C frame, or message,
/// when there is none), keeping the first path that reached it.
pub(crate) fn dedupe(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen = std::collections::BTreeSet::new();
    findings
        .into_iter()
        .filter(|finding| seen.insert((finding.class, dedupe_key(finding))))
        .collect()
}

fn dedupe_key(finding: &Finding) -> String {
    place(finding, &finding.message)
}

/// Where a finding is: its Osprey `file:line`, else its runtime frame, else
/// `fallback` [SYMEX-FINDINGS].
pub(crate) fn place(finding: &Finding, fallback: &str) -> String {
    match (&finding.location, &finding.runtime_frame) {
        (Some(location), _) => format!("{}:{}", location.file, location.line),
        (None, Some(frame)) => frame.clone(),
        (None, None) => fallback.to_string(),
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]
mod tests {
    use super::*;

    const PTR_ERR: &str = "Error: memory error: out of bound pointer\n\
File: /src/runtime/list_runtime.c\nLine: 88\nassembly.ll line: 4012\nState: 7\nStack: \n\
\t#000000031 in osp_list_get(ptr, i64) at /src/runtime/list_runtime.c:88\n\
\t#000000024 in pick(i64) at index.osp:5\n\
\t#000000019 in main() at index.osp:12\n\
Info: \n\taddress: 94323213\n";

    fn put(dir: &Path, name: &str, bytes: impl AsRef<[u8]>) {
        std::fs::write(dir.join(name), bytes).expect(name);
    }

    fn write_fixture(dir: &Path) {
        std::fs::create_dir_all(dir).expect("fixture dir");
        put(dir, "test000001.ptr.err", PTR_ERR);
        let ktest = ktest::encode(&[("stdin", b"9\nxx"), ("stdin.len", &2i32.to_le_bytes())]);
        put(dir, "test000001.ktest", ktest);
        put(dir, "test000002.ptr.err", PTR_ERR);
        put(dir, "test000003.early", "max-time\n");
        put(dir, "test000004.solver.err", "Error: solver timeout\n");
        put(dir, "test000005.unmodelled.err", "Error: unmodelled: fork\nStack: \n\t#000000001 in spawn() at /src/runtime/system_runtime.c:40\n");
        put(dir, "test000006.user.err", "Error: external calls disallowed (in particular, external calls to functions prefixed with klee_ are not allowed)\nStack: \n\t#000000001 in main() at index.osp:41\n");
        put(
            dir,
            "test000007.user.err",
            "Error: invalid klee_assume call (provably false)\n",
        );
        put(
            dir,
            "messages.txt",
            "KLEE: WARNING: undefined reference to function: __osprey_coro_resume\n",
        );
        put(dir, "warnings.txt", "KLEE: WARNING: Disallowed call to external function: __osprey_coro_new\nKLEE: WARNING: Disallowed call to external function: __osprey_coro_new\n");
        put(dir, "info", "KLEE: done: explored paths = 3\n");
    }

    #[test]
    fn err_reports_yield_message_and_frames() {
        let (message, frames) = parse_err(PTR_ERR);
        assert_eq!(message, "memory error: out of bound pointer");
        assert_eq!(frames.len(), 3);
        assert_eq!(
            frames[1],
            Frame {
                function: "pick(i64)".into(),
                file: "index.osp".into(),
                line: 5
            }
        );
        assert_eq!(parse_err("garbage").0, "unknown error");
    }

    #[test]
    fn every_klee_suffix_has_a_class() {
        assert_eq!(Class::classify("ptr"), Some(Class::Memory));
        assert_eq!(Class::classify("assert"), Some(Class::Abort));
        assert_eq!(Class::classify("div"), Some(Class::Arith));
        assert_eq!(Class::classify("missing_return"), Some(Class::Ub));
        assert_eq!(Class::classify("external"), Some(Class::Unmodelled));
        assert_eq!(Class::classify("model"), None);
        assert_eq!(Class::Ub.name(), "ub");
    }

    #[test]
    fn scanning_a_run_directory_classifies_dedupes_and_pairs_inputs() {
        let dir = std::env::temp_dir().join(format!("osprey-hunt-scan-{}", std::process::id()));
        write_fixture(&dir);
        let result = scan(&dir).expect("scan");
        assert_eq!(result.findings.len(), 3, "{:?}", result.findings);
        let first = &result.findings[0];
        assert_eq!(first.class, Class::Memory);
        assert_eq!(
            first.location,
            Some(Location {
                file: "index.osp".into(),
                line: 5
            })
        );
        assert_eq!(
            first.runtime_frame.as_deref(),
            Some("/src/runtime/list_runtime.c:88 in osp_list_get(ptr, i64)")
        );
        assert_eq!(first.stdin, b"9\n");
        assert_eq!(result.findings[1].class, Class::Unmodelled);
        assert_eq!(result.findings[1].location, None);
        assert_eq!(result.findings[2].class, Class::Unmodelled);
        assert_eq!(
            result.findings[2].location,
            Some(Location {
                file: "index.osp".into(),
                line: 41
            })
        );
        assert_eq!(
            result.findings[2].message,
            "external call outside the module: __osprey_coro_new"
        );
        assert_eq!(external_calls(""), Vec::<String>::new());
        assert_eq!(result.incomplete.len(), 3);
        assert_eq!(result.incomplete[0].reason, "max-time");
        assert_eq!(result.incomplete[1].reason, "solver timeout");
        assert_eq!(
            result.incomplete[2].reason,
            "invalid klee_assume call (provably false)"
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(scan(&dir).is_err());
    }

    #[test]
    fn findings_without_frames_dedupe_on_the_message() {
        let a = finding(
            "test1",
            "abort",
            Class::Abort,
            "Error: abort failure\n",
            None,
        );
        let b = finding(
            "test2",
            "abort",
            Class::Abort,
            "Error: abort failure\n",
            None,
        );
        let c = finding("test3", "abort", Class::Abort, "Error: other\n", None);
        assert_eq!(dedupe(vec![a.clone(), b, c.clone()]), vec![a, c]);
    }
}
