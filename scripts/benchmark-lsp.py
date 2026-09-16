#!/usr/bin/env python3
"""Measure complete LSP edit refreshes, including all open project siblings.

Run after building the compiler:
  python3 scripts/benchmark-lsp.py --compiler target/release/osprey
  python3 scripts/benchmark-lsp.py --project examples/projects/modules

A request queued after each edit measures completion of the server's serial
notification handler, even when identical diagnostic publishes are coalesced.
Results are JSON; synthetic projects and unsaved edits leave source files intact.
"""

import argparse
import json
from pathlib import Path
import platform
import queue
import shutil
import statistics
import subprocess
import tempfile
import threading
import time
import tomllib


ROOT = Path(__file__).resolve().parent.parent
EXTENSIONS = {".osp", ".ospml"}


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/release/osprey")
    parser.add_argument("--project", type=Path, help="Default: temporary 10-file project")
    parser.add_argument("--counts", nargs="+", type=int, default=[1, 5, 10])
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=180)
    args = parser.parse_args()
    if args.iterations < 1 or any(count < 1 for count in args.counts):
        parser.error("iterations and open-file counts must be positive")
    return args


def synthetic_project(root):
    (root / "src").mkdir()
    (root / "osprey.toml").write_text(
        '[project]\nname = "lsp-benchmark"\nsource_roots = ["src"]\n'
        'default_namespace = "benchmark"\nentry = "src/main.ospml"\n'
    )
    (root / "src/main.ospml").write_text('print (helper1 41)\n')
    for index in range(1, 10):
        (root / f"src/helper{index}.ospml").write_text(
            f"helper{index} : int -> int\nhelper{index} value = value + {index} ?: 0\n"
        )
    return root


def project_files(root):
    project = tomllib.loads((root / "osprey.toml").read_text())["project"]
    entry = (root / project["entry"]).resolve()
    files = sorted({
        path.resolve() for source in project.get("source_roots", ["src"])
        for path in (root / source).rglob("*") if path.suffix in EXTENSIONS
        and not any(part.startswith(".") or part == "target"
                    for part in path.relative_to(root).parts)
    })
    return [entry] + [path for path in files if path != entry]


def snapshot_project(root, original):
    original = original.resolve()
    shutil.copyfile(original / "osprey.toml", root / "osprey.toml")
    for source in project_files(original):
        target = root / source.relative_to(original)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    return root


def read_frame(stream):
    headers = {}
    while line := stream.readline():
        if line == b"\r\n":
            size = int(headers[b"content-length"])
            payload = stream.read(size)
            if len(payload) != size:
                raise EOFError("Incomplete LSP response")
            return json.loads(payload)
        name, value = line.split(b":", 1)
        headers[name.lower()] = value.strip()
    raise EOFError("LSP closed stdout")


def receive(stream, incoming):
    try:
        while True:
            incoming.put(read_frame(stream))
    except (EOFError, ValueError, KeyError) as error:
        incoming.put(error)


def send(process, method, params, request_id=None):
    message = {"jsonrpc": "2.0", "method": method, "params": params}
    if request_id is not None:
        message["id"] = request_id
    payload = json.dumps(message).encode()
    process.stdin.write(f"Content-Length: {len(payload)}\r\n\r\n".encode() + payload)
    process.stdin.flush()


def barrier(process, incoming, request_id, timeout):
    send(process, "osprey/benchmarkBarrier", {}, request_id)
    deadline = time.monotonic() + timeout
    diagnostics = []
    while True:
        message = incoming.get(timeout=max(0, deadline - time.monotonic()))
        if isinstance(message, Exception):
            raise message
        if message.get("id") == request_id:
            if message.get("error", {}).get("code") != -32601:
                raise RuntimeError(f"Unexpected barrier response: {message}")
            return diagnostics
        if message.get("method") == "textDocument/publishDiagnostics":
            diagnostics.append(message["params"])


def open_documents(process, incoming, files, timeout):
    for index, path in enumerate(files):
        send(process, "textDocument/didOpen", {"textDocument": {
            "uri": path.as_uri(), "languageId": "osprey", "version": 1,
            "text": path.read_text(),
        }})
        reports = barrier(process, incoming, index + 10, timeout)
        errors = [diagnostic for report in reports for diagnostic in report["diagnostics"]
                  if diagnostic.get("severity") == 1]
        if errors:
            raise RuntimeError(f"Benchmark project has errors: {errors}")


def measure_edits(process, incoming, path, args):
    samples = []
    original = path.read_text()
    for index in range(args.iterations + 1):
        started = time.perf_counter()
        send(process, "textDocument/didChange", {
            "textDocument": {"uri": path.as_uri(), "version": index + 2},
            "contentChanges": [{"text": original + f"\n// benchmark edit {index}\n"}],
        })
        reports = barrier(process, incoming, 1000 + index, args.timeout)
        elapsed = (time.perf_counter() - started) * 1000
        if index:  # One warm-up edit is excluded.
            samples.append({"milliseconds": round(elapsed, 3), "publishes": len(reports)})
    return samples


def run_server(args, files):
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen([str(args.compiler.resolve()), "lsp"], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=errors)
        incoming = queue.Queue()
        threading.Thread(target=receive, args=(process.stdout, incoming), daemon=True).start()
        try:
            send(process, "initialize", {"capabilities": {}}, 1)
            incoming.get(timeout=args.timeout)
            send(process, "initialized", {})
            open_documents(process, incoming, files, args.timeout)
            return measure_edits(process, incoming, files[0], args)
        finally:
            process.kill()
            process.wait()


def benchmark(args, root):
    files = project_files(root.resolve())
    for count in args.counts:
        if count > len(files):
            raise ValueError(f"Requested {count} open files; project has {len(files)}")
        samples = run_server(args, files[:count])
        times = [sample["milliseconds"] for sample in samples]
        print(json.dumps({
            "project": str(args.project or "synthetic-10-file"), "files": len(files),
            "open_files": count, "compiler": str(args.compiler.resolve()),
            "platform": platform.platform(), "samples": samples,
            "median_ms": round(statistics.median(times), 3), "max_ms": max(times),
        }), flush=True)


def main():
    args = arguments()
    with tempfile.TemporaryDirectory(prefix="osprey-lsp-benchmark-") as directory:
        root = (snapshot_project(Path(directory), args.project) if args.project
                else synthetic_project(Path(directory)))
        benchmark(args, root)


if __name__ == "__main__":
    main()
