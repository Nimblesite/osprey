//! Native replay permuted against [SYMEX-REPLAY]
//! (docs/specs/tooling/SymbolicExecution.md): every exit byte, every escaping
//! file path, every fatal signal, chosen exit statuses, nested files, raw
//! stdin, and a fresh scratch directory per finding.

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::findings::{Class, Replay};
use super::replay;
use super::spec_findings_tests::{sample, Dir};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

/// Every exit byte is the program's own choice and stays klee-only, 128 and
/// above included; every signal reproduces, under its name or its number.
#[cfg(unix)]
#[test]
fn every_exit_byte_maps_to_one_verdict() {
    for code in 0..=u8::MAX {
        assert_eq!(
            replay::outcome(replay::raw_status(i32::from(code), 0)),
            Replay::KleeOnly,
            "exit {code}"
        );
    }
    for signal in 1..=31 {
        match replay::outcome(replay::raw_status(0, signal)) {
            Replay::Reproduced(name) => assert!(
                name.starts_with("SIG") || name == format!("signal {signal}"),
                "{signal}: {name}"
            ),
            other => panic!("signal {signal} gave {other:?}"),
        }
    }
}

/// A recorded file outside the scratch directory is refused before anything
/// runs: the executable here does not exist, so reaching it would be an error.
#[test]
fn every_escaping_file_path_is_refused_before_the_program_runs() {
    let mut escapes = vec![
        "../x",
        "a/../../x",
        "./../x",
        "a/../b",
        "..",
        "../../../../etc/passwd",
    ];
    if cfg!(unix) {
        escapes.extend(["/tmp/x", "/etc/passwd"]);
    }
    if cfg!(windows) {
        escapes.extend(["C:\\x", "C:/Windows/x", "\\\\server\\share\\x"]);
    }
    let dir = Dir::new("escape");
    let workdir = dir.0.join("work");
    for path in escapes {
        let mut finding = sample(Class::Memory, None, None, "m");
        finding.files = vec![
            ("ok.txt".into(), b"fine".to_vec()),
            (path.into(), b"bad".to_vec()),
        ];
        let verdict = replay::run_one(Path::new("/nonexistent/program"), &workdir, &finding)
            .expect("refused, not run");
        assert!(
            matches!(&verdict, Replay::Skipped(reason) if reason.contains(&format!("file {path} is outside"))),
            "{path}: {verdict:?}"
        );
    }
}

/// Unmodelled findings only exist under KLEE and are skipped, never run —
/// including when every finding is unmodelled. Pins a defect: when nothing is
/// replayable `replay` returns early and leaves them `not-run`, so the report
/// disagrees with a mixed run, which marks the same finding skipped.
#[test]
fn unmodelled_findings_are_skipped_even_when_nothing_else_is_replayed() {
    let source = "print(\"hi\")\n";
    let parsed = osprey_syntax::parse_program(source);
    let input =
        crate::project::CompilationInput::script("hunt.osp", source.to_string(), parsed.program);
    let mut findings = vec![sample(
        Class::Unmodelled,
        Some(("hunt.osp", 1)),
        None,
        "unmodelled: fork",
    )];
    replay::replay(&input, &mut findings, "gc").expect("nothing to build");
    assert!(
        matches!(&findings[0].replay, Replay::Skipped(reason) if reason.contains("unmodelled")),
        "{:?}",
        findings[0].replay
    );
}

#[cfg(unix)]
fn script(dir: &Dir, name: &str, body: &str) -> PathBuf {
    let path = dir.0.join(name);
    write_executable_fixture(&path, format!("#!/bin/sh\n{body}\n").as_bytes()).expect("script");
    path
}

// A parent-held writable descriptor can reach a concurrent fork and cause ETXTBSY.
#[cfg(unix)]
pub(super) fn write_executable_fixture(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let child = std::process::Command::new("/usr/bin/tee")
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()?;
    finish_fixture_write(child, bytes)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(unix)]
fn finish_fixture_write(mut child: std::process::Child, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let written = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("fixture writer stdin missing"))
        .and_then(|mut stdin| stdin.write_all(bytes));
    let status = child.wait();
    written?;
    if status?.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("fixture writer failed"))
    }
}

#[cfg(target_os = "linux")]
#[test]
fn fixture_writer_does_not_enter_the_parallel_parent_descriptor_table() -> std::io::Result<()> {
    let dir = Dir::new("fixture-writer-descriptors");
    let path = dir.0.join("held-writer");
    assert!(std::process::Command::new("mkfifo")
        .arg(&path)
        .status()?
        .success());
    let inherited = observe_fixture_writer(&dir, &"x".repeat(2 * 1024 * 1024))?;
    assert!(
        inherited.is_empty(),
        "a parallel fork could inherit fixture writers: {inherited:?}"
    );
    assert_one_shot_fixture(&dir)
}

#[cfg(target_os = "linux")]
fn assert_one_shot_fixture(dir: &Dir) -> std::io::Result<()> {
    let path = script(dir, "one-shot", "printf 'fixture-ok\\n'");
    let output = std::process::Command::new(path).output()?;
    assert!(output.status.success());
    assert_eq!(output.stdout, b"fixture-ok\n");
    assert!(output.stderr.is_empty());
    Ok(())
}

#[cfg(target_os = "linux")]
fn observe_fixture_writer(dir: &Dir, body: &str) -> std::io::Result<Vec<PathBuf>> {
    use std::io::Read;
    let path = dir.0.join("held-writer");
    std::thread::scope(|scope| {
        let writer = scope.spawn(|| script(dir, "held-writer", body));
        let mut reader = std::fs::File::open(&path)?;
        let inherited = writable_fixture_descriptors(&path);
        let mut received = Vec::new();
        let read = reader.read_to_end(&mut received);
        let joined = writer
            .join()
            .map_err(|_panic_payload| std::io::Error::other("fixture writer panicked"));
        assert_eq!(read?, received.len());
        assert_eq!(joined?, path);
        assert_eq!(received, format!("#!/bin/sh\n{body}\n").as_bytes());
        inherited
    })
}

#[cfg(target_os = "linux")]
fn writable_fixture_descriptors(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::read_dir("/proc/self/fd")?.try_fold(Vec::new(), |mut descriptors, entry| {
        let entry = entry?;
        if writable_fixture_descriptor(&entry, path)? {
            descriptors.push(entry.path());
        }
        Ok(descriptors)
    })
}

#[cfg(target_os = "linux")]
fn writable_fixture_descriptor(entry: &std::fs::DirEntry, path: &Path) -> std::io::Result<bool> {
    let target = match std::fs::read_link(entry.path()) {
        Ok(target) => target,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if target != path {
        return Ok(false);
    }
    let info = std::fs::read_to_string(Path::new("/proc/self/fdinfo").join(entry.file_name()))?;
    let flags = info
        .lines()
        .find_map(|line| line.strip_prefix("flags:"))
        .ok_or_else(|| std::io::Error::other("descriptor flags missing"))?;
    let flags = u32::from_str_radix(flags.trim(), 8).map_err(std::io::Error::other)?;
    Ok(flags & 0o3 != 0)
}

#[cfg(unix)]
fn stdin_file_inputs() -> super::ktest::Ktest {
    super::ktest::parse(&super::ktest::encode(&[
        ("osprey.hunt.files.v2", &2i32.to_le_bytes()),
        ("stdin", b"S\n"),
        ("stdin.len", &2i32.to_le_bytes()),
        ("file.ok:stdin", &1i32.to_le_bytes()),
        ("file.len:stdin", &1i32.to_le_bytes()),
        ("file.data:stdin", b"F"),
    ]))
    .expect("independent stdin and file streams")
}

#[cfg(unix)]
#[test]
fn stdin_and_a_file_named_stdin_reproduce_independently() {
    let dir = Dir::new("stdin-file");
    let exe = script(&dir, "probe", "IFS= read -r value; [ \"$value\" = S ] && [ \"$(cat stdin)\" = F ] && kill -ABRT $$; exit 0");
    let inputs = stdin_file_inputs();
    let mut finding = sample(Class::Memory, None, None, "m");
    finding.stdin = inputs.stream("stdin").expect("stdin");
    finding.files = inputs.files();
    assert_eq!(finding.stdin, b"S\n");
    assert_eq!(finding.files, vec![("stdin".into(), b"F".to_vec())]);
    assert_eq!(
        replay::run_one(&exe, &dir.0.join("work"), &finding).expect("replay"),
        Replay::Reproduced("SIGABRT".into())
    );
}

/// The verdict of replaying a memory finding against `exe` in `dir`.
#[cfg(unix)]
fn verdict_of(dir: &Dir, exe: &Path) -> Replay {
    replay::run_one(
        exe,
        &dir.0.join("work"),
        &sample(Class::Memory, None, None, "m"),
    )
    .expect("replay")
}

/// Each fatal signal the native program dies of is reported by name.
#[cfg(unix)]
#[test]
fn every_fatal_signal_reproduces_by_name() {
    let dir = Dir::new("signals");
    for signal in ["SEGV", "ABRT", "FPE", "ILL", "TRAP", "BUS"] {
        let exe = script(&dir, signal, &format!("kill -{signal} $$"));
        let verdict = verdict_of(&dir, &exe);
        assert_eq!(
            verdict,
            Replay::Reproduced(format!("SIG{signal}")),
            "{signal}"
        );
    }
}

#[cfg(unix)]
fn assert_klee_only_exits(name: &str, statuses: &[u8]) {
    let dir = Dir::new(name);
    for status in statuses {
        let exe = script(&dir, &format!("exit{status}"), &format!("exit {status}"));
        assert_eq!(verdict_of(&dir, &exe), Replay::KleeOnly, "exit {status}");
    }
}

/// A clean exit with any status the program chose is `klee-only`.
#[cfg(unix)]
#[test]
fn every_chosen_exit_status_up_to_128_is_klee_only() {
    assert_klee_only_exits("statuses-low", &[0, 1, 2, 3, 42, 127, 128]);
}

/// Pins a defect: "a clean exit — including a non-zero `main` the program
/// chose itself — is `klee-only`", but `child_exit_code` folds a signal into
/// `128 + n`, so a program that exits 139 is reported as a reproduced SIGSEGV.
#[cfg(unix)]
#[test]
fn a_chosen_exit_status_above_128_is_still_klee_only() {
    assert_klee_only_exits("statuses-high", &[129, 134, 139, 255]);
}

/// Nested recorded files land at their relative paths and stdin carries
/// every byte, including NUL and the high half.
#[cfg(unix)]
#[test]
fn nested_files_and_raw_stdin_reach_the_native_program() {
    let dir = Dir::new("nested");
    let exe = script(
        &dir,
        "check",
        "[ \"$(cat deep/er/a.txt)\" = deep ] && [ \"$(od -An -tx1 | tr -d ' \\n')\" = 00ff0a41 ] && kill -SEGV $$\nexit 0",
    );
    let mut finding = sample(Class::Memory, None, None, "m");
    finding.files = vec![
        ("deep/er/a.txt".into(), b"deep".to_vec()),
        ("top.txt".into(), Vec::new()),
    ];
    finding.stdin = vec![0, 0xff, b'\n', b'A'];
    let work = dir.0.join("work");
    assert_eq!(
        replay::run_one(&exe, &work, &finding).expect("replay"),
        Replay::Reproduced("SIGSEGV".into())
    );
    assert!(
        work.join("top.txt").exists(),
        "an empty recorded file is still created"
    );
    finding.stdin = vec![0, 0xff, b'\n', b'B'];
    assert_eq!(
        replay::run_one(&exe, &work, &finding).expect("replay"),
        Replay::KleeOnly
    );
}

/// Each replay starts from an empty scratch directory: a file one finding
/// wrote is gone for the next.
#[cfg(unix)]
#[test]
fn each_replay_starts_from_a_clean_directory() {
    let dir = Dir::new("fresh");
    let exe = script(&dir, "probe", "[ -e stale.txt ] && kill -SEGV $$\nexit 0");
    let work = dir.0.join("work");
    let mut first = sample(Class::Memory, None, None, "m");
    first.files = vec![("stale.txt".into(), b"x".to_vec())];
    assert_eq!(
        replay::run_one(&exe, &work, &first).expect("replay"),
        Replay::Reproduced("SIGSEGV".into())
    );
    let second = sample(Class::Memory, None, None, "m");
    assert_eq!(
        replay::run_one(&exe, &work, &second).expect("replay"),
        Replay::KleeOnly
    );
}

/// A program that exits without reading stdin is not a replay failure.
#[cfg(unix)]
#[test]
fn a_program_that_ignores_a_large_stdin_still_replays() {
    let dir = Dir::new("ignores");
    let exe = script(&dir, "quick", "exit 0");
    let mut finding = sample(Class::Memory, None, None, "m");
    finding.stdin = vec![b'x'; 1 << 20];
    assert_eq!(
        replay::run_one(&exe, &dir.0.join("work"), &finding).expect("replay"),
        Replay::KleeOnly
    );
}

#[cfg(unix)]
fn crash_replay_script(dir: &Dir) -> PathBuf {
    script(dir, "replay", "read -r value; printf '%s\\n' \"$value\" >> \"$0.calls\"; [ \"$value\" = crash ] && kill -ABRT $$; exit 0")
}

#[cfg(unix)]
fn assert_confirmed_witness(kept: &[super::findings::Finding]) {
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].stdin, b"crash\n");
    assert_eq!(kept[0].replay, Replay::Reproduced("SIGABRT".into()));
}

#[cfg(unix)]
#[test]
fn a_later_reproduced_witness_survives_deduplication() {
    let dir = Dir::new("later-witness");
    let exe = crash_replay_script(&dir);
    let mut first = sample(Class::Memory, Some(("same.osp", 4)), None, "same fault");
    first.stdin = b"quiet\n".to_vec();
    let mut second = first.clone();
    second.stdin = b"crash\n".to_vec();
    for finding in [&mut first, &mut second] {
        finding.replay = replay::run_one(&exe, &dir.0.join("work"), finding).expect("replay");
    }
    assert_eq!(first.replay, Replay::KleeOnly);
    assert_eq!(second.replay, Replay::Reproduced("SIGABRT".into()));
    let kept = super::findings::dedupe(vec![first, second]);
    assert_confirmed_witness(&kept);
}

#[cfg(unix)]
fn same_site_witnesses(dir: &Dir) -> Vec<super::findings::Finding> {
    let report = b"Error: same fault\nStack: \n\t#0 in main() at same.osp:4\n";
    for (n, input) in [b"quiet\n".as_slice(), b"crash\n", b"unneeded\n"]
        .iter()
        .enumerate()
    {
        dir.write(&format!("test{n:06}.ptr.err"), report);
        dir.write(
            &format!("test{n:06}.ktest"),
            &super::ktest::encode(&[("stdin", input)]),
        );
    }
    super::findings::scan_all(&dir.0)
        .expect("all witnesses")
        .findings
}

#[cfg(unix)]
#[test]
fn scan_replays_until_a_witness_reproduces_and_keeps_its_exact_input() {
    let dir = Dir::new("scan-witnesses");
    let exe = crash_replay_script(&dir);
    let mut witnesses = same_site_witnesses(&dir);
    assert_eq!(witnesses.len(), 3);
    let no_replay = super::findings::scan(&dir.0).expect("no replay");
    assert_eq!(no_replay.findings.len(), 1);
    replay::replay_all(&exe, &dir.0.join("work"), &mut witnesses).expect("replay all");
    assert_eq!(witnesses[0].replay, Replay::KleeOnly);
    assert_eq!(witnesses[1].replay, Replay::Reproduced("SIGABRT".into()));
    assert_eq!(witnesses[2].replay, Replay::NotRun);
    let calls = std::fs::read(exe.with_file_name("replay.calls")).expect("calls");
    assert_eq!(calls, b"quiet\ncrash\n");
    let kept = super::findings::dedupe(witnesses);
    assert_confirmed_witness(&kept);
    assert_eq!(kept[0].test, "test000001");
}
