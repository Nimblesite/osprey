//! Actual Linux process launches for the native replay retry boundary [SYMEX-REPLAY].

use super::{spawn, start};
use crate::hunt::process::wait_bounded;
use crate::hunt::spec_findings_tests::Dir;
use crate::hunt::spec_replay_tests::write_executable_fixture;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::thread::ScopedJoinHandle;
use std::time::{Duration, Instant};

const SPAWN_TEST_BOUND: Duration = Duration::from_secs(3);
const PROBE_ENV: &str = "OSPREY_TEST_BUSY_SPAWN_PROBE";
const PROBE_DONE: &str = "spawn probe passed";
const BUSY_WATCHER: &str = r"
import ctypes, os, select, struct, sys
lib = ctypes.CDLL(None, use_errno=True)
fd = lib.inotify_init1(os.O_CLOEXEC | os.O_NONBLOCK)
assert fd >= 0, ctypes.get_errno()
assert lib.inotify_add_watch(fd, os.fsencode(sys.argv[1]), 0x30) >= 0, ctypes.get_errno()
def closes(data):
    offset, count = 0, 0
    while offset < len(data):
        _, mask, _, length = struct.unpack_from('iIII', data, offset)
        assert not mask & 0x4000, 'inotify queue overflow'
        count += bool(mask & 0x10)
        offset += 16 + length
    return count
def drain():
    count = 0
    while True:
        try:
            count += closes(os.read(fd, 65536))
        except BlockingIOError:
            return count
mode = sys.argv[2]
assert mode in ('release', 'count'), 'unknown watcher mode'
print('ready', flush=True)
if mode == 'release':
    while drain() == 0:
        assert select.select([fd], [], [], 5)[0], 'no executable close event'
    print('busy', flush=True)
else:
    count = 0
    while True:
        ready = select.select([fd, 0], [], [], 5)[0]
        assert ready, 'no launch or completion event'
        count += drain()
        if 0 in ready:
            assert os.read(0, 4096) == b'finish\n', 'missing completion request'
            count += drain()
            print(count, flush=True)
            break
os.close(fd)
";

#[test]
fn temporarily_busy_executable_starts_after_original_writer_closes() -> io::Result<()> {
    bounded_probe(
        "temporarily_busy_executable_starts_after_original_writer_closes",
        transient_busy_probe,
    )
}

fn transient_busy_probe() -> io::Result<()> {
    let (scratch, path, writer) = busy_fixture("spawn-transient")?;
    let (mut watcher, mut notices) = watch_busy(&path, "release")?;
    notice(&mut notices, "ready")?;
    let outcome = std::thread::scope(|scope| {
        let started = scope.spawn(|| spawn(&path, &scratch.0));
        release_after_event(writer, &mut notices, started)
    });
    assert!(watcher.wait()?.success());
    assert!(outcome?.success());
    Ok(())
}

#[test]
fn permanently_busy_executable_fails_within_spawn_budget() -> io::Result<()> {
    bounded_probe(
        "permanently_busy_executable_fails_within_spawn_budget",
        permanent_busy_probe,
    )
}

fn bounded_probe(case: &str, probe: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    let name = format!("hunt::replay::spawn_tests::{case}");
    if std::env::var(PROBE_ENV).ok().as_deref() == Some(name.as_str()) {
        probe()?;
        println!("{PROBE_DONE}:{name}");
        Ok(())
    } else {
        run_bounded_probe(&name)
    }
}

fn run_bounded_probe(name: &str) -> io::Result<()> {
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture"])
        .env(PROBE_ENV, name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let status = wait_bounded(&mut child, SPAWN_TEST_BOUND).map_err(io::Error::other)?;
    let output = child.wait_with_output()?;
    assert!(status.is_some_and(|value| value.success()), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!("{PROBE_DONE}:{name}")),
        "{output:?}"
    );
    Ok(())
}

fn permanent_busy_probe() -> io::Result<()> {
    let (scratch, path, _writer) = busy_fixture("spawn-permanent")?;
    let (watcher, mut notices) = watch_busy(&path, "count")?;
    notice(&mut notices, "ready")?;
    let before = Instant::now();
    let error = launch_failure(spawn(&path, &scratch.0))?;
    let elapsed = before.elapsed();
    assert!(elapsed < SPAWN_TEST_BOUND, "busy spawn took {elapsed:?}");
    assert_eq!(
        error,
        launch_error(&path, &io::Error::from_raw_os_error(26))
    );
    assert_eq!(finish_count(watcher, &mut notices)?, 51);
    Ok(())
}

#[test]
fn ordinary_spawn_errors_preserve_the_original_os_error() -> io::Result<()> {
    bounded_probe(
        "ordinary_spawn_errors_preserve_the_original_os_error",
        ordinary_error_probe,
    )
}

fn ordinary_error_probe() -> io::Result<()> {
    let scratch = Dir::new("spawn-missing");
    let path = scratch.0.join("missing");
    let expected = launch_failure(start(&path, &scratch.0))?;
    let actual = launch_failure(spawn(&path, &scratch.0))?;
    assert_eq!(expected.kind(), io::ErrorKind::NotFound);
    assert_eq!(actual, launch_error(&path, &expected));
    bad_format_is_single_attempt(&scratch)
}

fn bad_format_is_single_attempt(scratch: &Dir) -> io::Result<()> {
    let path = scratch.0.join("bad-format");
    write_executable_fixture(&path, b"not an executable\n")?;
    let (watcher, mut notices) = watch_busy(&path, "count")?;
    notice(&mut notices, "ready")?;
    let error = launch_failure(spawn(&path, &scratch.0))?;
    assert_eq!(error, launch_error(&path, &io::Error::from_raw_os_error(8)));
    assert_eq!(finish_count(watcher, &mut notices)?, 1);
    Ok(())
}

fn launch_error(path: &Path, error: &io::Error) -> String {
    format!("could not run {}: {error}", path.display())
}

fn launch_failure<E>(result: Result<Child, E>) -> io::Result<E> {
    match result {
        Err(error) => Ok(error),
        Ok(mut child) => {
            let status = child.wait()?;
            Err(io::Error::other(format!(
                "unexpected executable status: {status}"
            )))
        }
    }
}

fn busy_fixture(name: &str) -> io::Result<(Dir, PathBuf, File)> {
    let scratch = Dir::new(name);
    let path = scratch.0.join("probe");
    write_executable_fixture(&path, b"#!/bin/sh\nexit 0\n")?;
    let writer = std::fs::OpenOptions::new().write(true).open(&path)?;
    Ok((scratch, path, writer))
}

fn watch_busy(path: &Path, mode: &str) -> io::Result<(Child, BufReader<ChildStdout>)> {
    let mut watcher = Command::new("python3")
        .args(["-c", BUSY_WATCHER])
        .arg(path)
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = watcher
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("busy watcher stdout missing"))?;
    Ok((watcher, BufReader::new(stdout)))
}

fn finish_count(mut watcher: Child, notices: &mut impl BufRead) -> io::Result<usize> {
    let sent = watcher
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("busy watcher stdin missing"))
        .and_then(|mut stdin| stdin.write_all(b"finish\n"));
    let mut line = String::new();
    let read = notices.read_line(&mut line);
    let status = watcher.wait();
    sent?;
    let _count = read?;
    assert!(status?.success());
    line.trim_end().parse().map_err(io::Error::other)
}

fn notice(reader: &mut impl BufRead, expected: &str) -> io::Result<()> {
    let mut line = String::new();
    let count = reader.read_line(&mut line)?;
    if count != 0 && line.trim_end() == expected {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "expected {expected}, got {line:?}"
        )))
    }
}

fn release_after_event(
    writer: File,
    notices: &mut impl BufRead,
    started: ScopedJoinHandle<'_, Result<Child, String>>,
) -> io::Result<ExitStatus> {
    let observed = notice(notices, "busy");
    drop(writer);
    let exited = started
        .join()
        .map_err(|_panic_payload| io::Error::other("spawn worker panicked"))
        .and_then(|result| result.map_err(io::Error::other))
        .and_then(|mut child| child.wait());
    observed?;
    exited
}
