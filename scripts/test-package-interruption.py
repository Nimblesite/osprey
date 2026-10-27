"""Pin manifest integrity when an Add process is interrupted during relock."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import subprocess
import tempfile
from threading import Event, Thread
import unittest

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = '[project]\nname="interruption-proof"\nsource_roots=["src"]\nentry="src/main.osp"\n'
LISTING = json.dumps({"packages": [{"key": "@demo/one", "releases": [
    {"epoch": 1, "ordinal": 1, "digest": "sha256:" + "a" * 64}]}]}).encode()


def handler_for(relocking, release):
    class Handler(BaseHTTPRequestHandler):
        requests_seen = 0

        def do_GET(self):
            Handler.requests_seen += 1
            if Handler.requests_seen == 2:
                relocking.set()
                release.wait(20)
                return
            self.send_response(200)
            self.send_header("Content-Length", str(len(LISTING)))
            self.end_headers()
            self.wfile.write(LISTING)

        def log_message(self, *_args):
            pass
    return Handler


@contextmanager
def registry():
    relocking, release = Event(), Event()
    server = HTTPServer(("127.0.0.1", 0), handler_for(relocking, release))
    worker = Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        yield server.server_port, relocking
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        worker.join()


def prepare(directory):
    (directory / "src").mkdir()
    (directory / "src/main.osp").write_text('fn main() = print("ok")\n')
    manifest = directory / "osprey.toml"
    manifest.write_text(MANIFEST)
    return manifest


def interrupt_add(directory, port, relocking):
    binary = str(Path(os.environ.get("OSPREY_PACKAGE_CLIENT", ROOT / "target/osprey-pkg")).resolve())
    with subprocess.Popen([binary], cwd=directory, stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as child:
        try:
            child.stdin.write(f"--json add @demo/one --registry http://127.0.0.1:{port} --project .\n")
            child.stdin.flush()
            if not relocking.wait(15):
                raise AssertionError("Add did not reach its relock request")
        finally:
            child.kill()
            child.communicate()


class PackageInterruptionTests(unittest.TestCase):
    def test_interruption_during_relock_preserves_manifest(self):
        with tempfile.TemporaryDirectory(prefix="osprey-interruption-") as path:
            directory = Path(path)
            manifest = prepare(directory)
            with registry() as (port, relocking):
                interrupt_add(directory, port, relocking)
            self.assertEqual(manifest.read_text(), MANIFEST,
                             "Interrupted Add must not leave a dependency without a lock")
            self.assertFalse((directory / "osprey.lock").exists())


if __name__ == "__main__":
    unittest.main()
