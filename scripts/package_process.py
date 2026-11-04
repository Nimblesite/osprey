"""Bound package collectors and keep diagnostics while alive. [PKG-COVERAGE-GATE]

Defaults: 300 seconds, 1024 MiB summed POSIX process-group RSS, 16 MiB output.
Positive OSPREY_PACKAGE_TIMEOUT_SECONDS, OSPREY_PACKAGE_MAX_RSS_MB and
OSPREY_PACKAGE_MAX_OUTPUT_MB override these budgets. Each report has live
.stdout.log/.stderr.log and .process.jsonl (compiler hash, PID, RSS, CPU, cause).
Timeout/output bounds apply on every platform; process-group RSS/cleanup are POSIX.
"""
from contextlib import contextmanager
import hashlib
import json
import math
import os
import signal
import subprocess
import time
from threading import Event
from datetime import datetime, timezone
from pathlib import Path

INTERRUPTED = Event()
POLL_SECONDS = 0.1
DEFAULT_TIMEOUT = 300
DEFAULT_RSS_MB = 1024
DEFAULT_OUTPUT_MB = 16


@contextmanager
def interrupts():
    """Request worker cleanup on user interruption instead of orphaning child sessions."""
    INTERRUPTED.clear()
    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
    for sig in previous:
        signal.signal(sig, lambda _signum, _frame: INTERRUPTED.set())
    try:
        yield
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def limit(name, fallback):
    value = float(os.environ.get(name, fallback))
    if not math.isfinite(value) or value <= 0:
        raise ValueError(f"{name} must be a finite positive number")
    return value


def group_snapshot(pid):
    """Record only our process group; never inspect environment or command arguments."""
    if os.name != "posix":
        return []
    result = subprocess.run(["ps", "-axo", "pid=,ppid=,pgid=,rss=,pcpu=,comm="],
                            capture_output=True, text=True, timeout=1, check=True)
    rows = []
    for line in result.stdout.splitlines():
        parts = line.split(None, 5)
        if len(parts) == 6 and int(parts[2]) == pid:
            rows.append(dict(pid=int(parts[0]), parent=int(parts[1]), rss_bytes=int(parts[3]) * 1024,
                             cpu_percent=float(parts[4]), executable=parts[5]))
    return rows


def event(log, kind, **fields):
    log.write(json.dumps(dict(event=kind, utc=datetime.now(timezone.utc).isoformat(), **fields)) + "\n")
    log.flush()


def stop_group(process):
    """Kill our group even if its compiler parent already exited, then reap the parent."""
    if os.name == "posix":
        if process.poll() is not None and not group_snapshot(process.pid):
            return
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except PermissionError:
            if process.poll() is None or group_snapshot(process.pid):
                raise
    elif process.poll() is None:
        process.kill()
    process.wait(timeout=5)


def violation(process, started, limits, outputs, log):
    if INTERRUPTED.is_set():
        return "collector interrupted; process group stopped"
    rows = group_snapshot(process.pid)
    elapsed = time.monotonic() - started
    rss = sum(row["rss_bytes"] for row in rows)
    sizes = [os.fstat(stream.fileno()).st_size for stream in outputs]
    event(log, "sample", elapsed_seconds=elapsed, group_rss_bytes=rss, processes=rows,
          stdout_bytes=sizes[0], stderr_bytes=sizes[1])
    if rss > limits[1]:
        return f"memory limit exceeded: process group RSS {rss} > {limits[1]} bytes"
    if sum(sizes) > limits[2]:
        return f"output limit exceeded: {sum(sizes)} > {limits[2]} bytes"
    return f"timeout after {elapsed:.3f}s (limit {limits[0]}s)" if elapsed >= limits[0] else None


def supervise(process, limits, outputs, log):
    started = time.monotonic()
    event(log, "start", pid=process.pid, timeout_seconds=limits[0], max_rss_bytes=limits[1],
          max_output_bytes=limits[2], group_monitor=os.name == "posix")
    while process.poll() is None:
        problem = violation(process, started, limits, outputs, log)
        if problem:
            event(log, "refused", reason=problem)
            return problem
        time.sleep(POLL_SECONDS)
    size = sum(os.fstat(stream.fileno()).st_size for stream in outputs)
    problem = f"output limit exceeded: {size} > {limits[2]} bytes" if size > limits[2] else None
    cause = signal.strsignal(-process.returncode) if process.returncode < 0 else None
    event(log, "exit", returncode=process.returncode, signal=cause, elapsed_seconds=time.monotonic() - started, reason=problem)
    return problem


def output(path, maximum):
    """Read bounded exact output; a limit refusal never becomes valid truncated evidence."""
    with path.open("rb") as stream:
        return stream.read(int(maximum) + 1)


def run(command, root, report):
    if INTERRUPTED.is_set():
        raise ValueError("collection interrupted before launch")
    limits = (limit("OSPREY_PACKAGE_TIMEOUT_SECONDS", DEFAULT_TIMEOUT),
              limit("OSPREY_PACKAGE_MAX_RSS_MB", DEFAULT_RSS_MB) * 1024**2,
              limit("OSPREY_PACKAGE_MAX_OUTPUT_MB", DEFAULT_OUTPUT_MB) * 1024**2)
    paths = [Path(report).with_suffix(suffix) for suffix in (".stdout.log", ".stderr.log", ".process.jsonl")]
    with paths[0].open("wb") as stdout, paths[1].open("wb") as stderr, paths[2].open("w") as log:
        with Path(command[0]).open("rb") as binary:
            compiler_sha256 = hashlib.file_digest(binary, "sha256").hexdigest()
        event(log, "command", argv=command, cwd=str(root), compiler_sha256=compiler_sha256)
        process = subprocess.Popen(command, cwd=root, stdin=subprocess.DEVNULL, stdout=stdout,
                                   stderr=stderr, start_new_session=os.name == "posix")
        try:
            problem = supervise(process, limits, (stdout, stderr), log)
        except (OSError, subprocess.SubprocessError) as error:
            problem = f"cannot monitor collector: {error}"
            event(log, "refused", reason=problem)
        finally:
            stop_group(process)
        event(log, "cleanup", returncode=process.returncode)
    completed = subprocess.CompletedProcess(command, process.returncode, output(paths[0], limits[2]), output(paths[1], limits[2]))
    return completed, ValueError(f"{problem}; diagnostics {paths[2]}") if problem else None
