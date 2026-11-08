//! Real-engine `osprey hunt` gates over `src/osprey/tests/hunt/` [SYMEX-CLI],
//! [SYMEX-CRASH-GATE]. Every test needs KLEE and the KLEE runtime bitcode, so
//! each is ignored by default and run by hand through `make _test_hunt`,
//! `make _test_hunt_clean` and `make _test_hunt_crashes` (plan 0034).
//!
//! An expectation file lists one finding per line as `<class> <file>:<line>`,
//! or holds the single word `none`; blank lines and `#` comments are ignored.
//! Only the crash gate compares replay verdicts: it is the one that replays.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

type Checked<T = ()> = Result<T, Box<dyn std::error::Error>>;

const HUNT_DIR: &str = "src/osprey/tests/hunt";
const BACKENDS: [&str; 2] = ["gc", "arc"];
/// Known bugs `_test_hunt` leaves out until their fix lands: `clean` is #306.
const KNOWN_BUGS: [&str; 1] = ["clean"];
/// The pinned deterministic settings: the default 64-byte input, 30 seconds.
const PINNED: [&str; 3] = ["--no-replay", "--max-time", "30"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// The exit code and stdout of `osprey <args>` run from the repo root with
/// `stdin` fed in; a run that outlives `deadline` is killed and an Error.
fn osprey(args: &[&str], stdin: &[u8], deadline: Duration) -> Checked<(i32, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args(args)
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    std::io::Write::write_all(&mut child.stdin.take().ok_or("no stdin pipe")?, stdin)?;
    let started = Instant::now();
    while child.try_wait()?.is_none() {
        if started.elapsed() > deadline {
            child.kill()?;
            return Err(format!("osprey {args:?} outlived {deadline:?}").into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    let code = output.status.code().ok_or("osprey died on a signal")?;
    Ok((code, String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// `osprey hunt <program> --memory=<memory> --json --quiet <extra>`: exit code and report.
fn hunt(program: &str, memory: &str, extra: &[&str], deadline: Duration) -> Checked<(i32, Value)> {
    let memory_flag = format!("--memory={memory}");
    let args = [&["hunt", program, &memory_flag, "--json", "--quiet"], extra].concat();
    let (code, stdout) = osprey(&args, b"", deadline)?;
    let report = serde_json::from_str(&stdout)
        .map_err(|error| format!("{program} ({memory}): no report ({error}): {stdout}"))?;
    Ok((code, report))
}

fn expected_findings(path: &Path) -> Checked<BTreeSet<String>> {
    let text = std::fs::read_to_string(path)?;
    let lines: BTreeSet<String> = text
        .lines()
        .filter_map(|line| line.split('#').next())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    Ok(if lines.len() == 1 && lines.contains("none") {
        BTreeSet::new()
    } else {
        lines
    })
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    field(value, key).as_str().unwrap_or("")
}

fn finding_key(finding: &Value) -> String {
    let class = text(finding, "class");
    match field(finding, "file").as_str() {
        Some(file) => {
            let name = Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file);
            format!("{class} {name}:{}", field(finding, "line"))
        }
        None => match text(finding, "runtimeFrame") {
            "" => format!("{class} {}", text(finding, "test")),
            frame => format!("{class} {frame}"),
        },
    }
}

fn findings(report: &Value) -> Checked<&Vec<Value>> {
    Ok(field(report, "findings")
        .as_array()
        .ok_or("findings is not a list")?)
}

fn actual_findings(report: &Value) -> Checked<BTreeSet<String>> {
    Ok(findings(report)?.iter().map(finding_key).collect())
}

fn explored(report: &Value, key: &str) -> Checked<u64> {
    let count = field(field(report, "explored"), key).as_u64();
    Ok(count.ok_or_else(|| format!("explored.{key} is not an integer"))?)
}

/// A complete, non-empty exploration.
fn validate(label: &str, report: &Value) -> Checked {
    let (paths, completed) = (explored(report, "paths")?, explored(report, "completed")?);
    assert!(
        paths > 0 && completed <= paths,
        "{label}: invalid or empty exploration"
    );
    let incomplete = field(report, "incomplete")
        .as_array()
        .ok_or("incomplete is not a list")?;
    assert!(
        incomplete.is_empty(),
        "{label}: incomplete exploration {incomplete:?}"
    );
    let found = !findings(report)?.is_empty();
    assert!(
        found || completed == paths,
        "{label}: {completed} of {paths} paths, no finding"
    );
    Ok(())
}

/// Hunts `<stem>.osp` and checks its findings and exit status against `<stem>.expected`.
fn check_stem(stem: &str, memory: &str) -> Checked {
    let (program, label) = (
        format!("{HUNT_DIR}/{stem}.osp"),
        format!("{stem} ({memory})"),
    );
    let (code, report) = hunt(&program, memory, &PINNED, Duration::from_secs(150))?;
    assert_eq!(
        text(&report, "memory"),
        memory,
        "{label}: report names the wrong backend"
    );
    validate(&label, &report)?;
    let expected = expected_findings(&repo_root().join(format!("{HUNT_DIR}/{stem}.expected")))?;
    assert_eq!(actual_findings(&report)?, expected, "{label}: findings");
    assert_eq!(
        code,
        i32::from(!findings(&report)?.is_empty()),
        "{label}: exit status"
    );
    Ok(())
}

fn deterministic_stems() -> Checked<Vec<String>> {
    let mut stems = Vec::new();
    for entry in std::fs::read_dir(repo_root().join(HUNT_DIR))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        match name.strip_suffix(".osp") {
            Some(stem) if !KNOWN_BUGS.contains(&stem) => stems.push(stem.to_string()),
            _ => {}
        }
    }
    stems.sort();
    Ok(stems)
}

/// crash.osp among them holds the default 64-byte input to 30 seconds.
#[test]
#[ignore = "needs KLEE"]
fn every_deterministic_hunt_program_matches_its_expectation_on_both_backends() -> Checked {
    let stems = deterministic_stems()?;
    let pinned = ["crash", "json_empty", "process", "record_update_list"];
    assert_eq!(stems, pinned, "the deterministic hunt programs changed");
    for memory in BACKENDS {
        for stem in &stems {
            check_stem(stem, memory)?;
        }
    }
    Ok(())
}

/// Known bug #306: red until clean.osp finishes a complete exploration.
#[test]
#[ignore = "needs KLEE"]
fn clean_finishes_a_complete_exploration_on_both_backends() -> Checked {
    BACKENDS
        .iter()
        .try_for_each(|memory| check_stem("clean", memory))
}

fn crash_cases() -> Checked<Vec<Value>> {
    let source = std::fs::read_to_string(repo_root().join(format!("{HUNT_DIR}/crash-cases.json")))?;
    let cases: Value = serde_json::from_str(&source)?;
    Ok(cases
        .as_array()
        .ok_or("crash-cases.json is not a list")?
        .clone())
}

/// One planted fault: found at the planted site, on a run that also has a
/// normal path, and every finding replays natively as an allowed signal.
fn check_planted(case: &Value, memory: &str) -> Checked {
    let label = format!("{} ({memory})", text(case, "name"));
    let extra = ["--max-time", "8", "--input-bytes", "1"];
    let (code, report) = hunt(
        text(case, "program"),
        memory,
        &extra,
        Duration::from_secs(10),
    )?;
    validate(&label, &report)?;
    assert!(
        explored(&report, "completed")? >= 1,
        "{label}: no normal path"
    );
    assert_eq!(code, 1, "{label}: expected a bounded native crash (exit 1)");
    let expected = expected_findings(&repo_root().join(text(case, "expected")))?;
    assert_eq!(
        actual_findings(&report)?,
        expected,
        "{label}: planted fault site"
    );
    let planted = field(case, "cases")
        .as_array()
        .ok_or("cases is not a list")?;
    assert_eq!(
        findings(&report)?.len(),
        planted.len(),
        "{label}: planted inputs missed"
    );
    findings(&report)?
        .iter()
        .try_for_each(|finding| check_replay(&label, finding, planted))
}

fn check_replay(label: &str, finding: &Value, planted: &[Value]) -> Checked {
    let stdin = text(finding, "stdin");
    let case = planted.iter().find(|item| text(item, "stdin") == stdin);
    let case = case.ok_or_else(|| format!("{label}: unexpected trigger input {stdin:?}"))?;
    assert_eq!(
        text(finding, "replay"),
        "reproduced",
        "{label}: finding did not reproduce"
    );
    let signals = field(case, "signals")
        .as_array()
        .ok_or("signals is not a list")?;
    let signal = field(finding, "replayDetail");
    assert!(signals.contains(signal), "{label}: signal {signal}");
    if let Some(file) = case.get("file") {
        let files = field(finding, "files")
            .as_array()
            .ok_or("finding has no files")?;
        assert!(
            files.contains(file),
            "{label}: triggering file input missing"
        );
    }
    Ok(())
}

/// The benign input `Z` runs natively to the program's normal output.
fn check_benign(case: &Value, memory: &str) -> Checked {
    let (name, memory_flag) = (text(case, "name"), format!("--memory={memory}"));
    let args = [text(case, "program"), "--run", &memory_flag];
    let run = osprey(&args, b"Z", Duration::from_secs(10))?;
    let expected = if matches!(name, "invalid_free" | "double_free") {
        "safe\n"
    } else {
        "result 0\n"
    };
    assert_eq!(
        run,
        (0, expected.to_string()),
        "{name} ({memory}): benign input Z"
    );
    Ok(())
}

#[test]
#[ignore = "needs KLEE"]
fn every_planted_crash_is_found_reproduced_and_benign_on_both_backends() -> Checked {
    let cases = crash_cases()?;
    assert_eq!(
        cases.len(),
        15,
        "crash-cases.json pins fifteen planted programs"
    );
    for memory in BACKENDS {
        for case in &cases {
            check_planted(case, memory)?;
            check_benign(case, memory)?;
        }
    }
    Ok(())
}
