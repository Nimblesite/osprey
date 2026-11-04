"""Pin manifest integrity when an Add process is interrupted during relock."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
import runpy
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch
from package_coverage_test_support import CoverageTestCase, own_report
from threading import Event, Thread
import unittest

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = '[project]\nname="interruption-proof"\nsource_roots=["src"]\nentry="src/main.osp"\n'
LISTING = json.dumps({"packages": [{"key": "@demo/one", "releases": [
    {"epoch": 1, "ordinal": 1, "digest": "sha256:" + "a" * 64}]}]}).encode()


def handler_for(relocking, release, listing):
    class Handler(BaseHTTPRequestHandler):
        requests_seen = 0

        def do_GET(self):
            Handler.requests_seen += 1
            if Handler.requests_seen == 2:
                relocking.set()
                release.wait(20)
                return
            self.send_response(200)
            self.send_header("Content-Length", str(len(listing)))
            self.end_headers()
            self.wfile.write(listing)

        def log_message(self, *_args):
            pass
    return Handler


@contextmanager
def registry(listing=LISTING):
    relocking, release = Event(), Event()
    server = HTTPServer(("127.0.0.1", 0), handler_for(relocking, release, listing))
    worker = Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        yield server.server_port, relocking
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        worker.join()


def prepare(directory, content=MANIFEST):
    (directory / "src").mkdir()
    (directory / "src/main.osp").write_text('fn main() = print("ok")\n')
    manifest = directory / "osprey.toml"
    manifest.write_bytes(content.encode())
    return manifest


def interrupt_add(directory, port, relocking, dependency="@demo/one"):
    binary = str(Path(os.environ.get("OSPREY_PACKAGE_CLIENT", ROOT / "target/osprey-pkg")).resolve())
    with subprocess.Popen([binary], cwd=directory, stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as child:
        try:
            child.stdin.write(f"--json add {dependency} --registry http://127.0.0.1:{port} --project .\n")
            child.stdin.flush()
            if not relocking.wait(15):
                raise AssertionError("Add did not reach its relock request")
        finally:
            child.kill()
            child.communicate()


class PackageInterruptionTests(unittest.TestCase):
    def test_interruption_during_relock_preserves_manifest(self):
        for content in [MANIFEST, "# Existing project — keep exact bytes\r\n" + MANIFEST.replace("\n", "\r\n")]:
            with self.subTest(manifest=content), tempfile.TemporaryDirectory(prefix="osprey-interruption-") as path:
                directory = Path(path)
                manifest = prepare(directory, content)
                with registry() as (port, relocking):
                    interrupt_add(directory, port, relocking)
                self.assertEqual(manifest.read_bytes(), content.encode(),
                                 "Interrupted Add must not leave a dependency without a lock")
                self.assertFalse((directory / "osprey.lock").exists())

    def test_interrupted_unpublished_local_add_preserves_manifest(self):
        with tempfile.TemporaryDirectory(prefix="osprey-local-interruption-") as path:
            directory = Path(path)
            manifest = prepare(directory)
            local = directory / "local"
            local.mkdir()
            prepare(local, MANIFEST + '[package]\nname="@demo/one"\nlicense="MIT"\nsummary="local"\nkeywords=["local"]\n')
            with registry(b'{"packages":[]}') as (port, relocking):
                interrupt_add(directory, port, relocking, "./local")
            self.assertEqual(manifest.read_bytes(), MANIFEST.encode(),
                             "Interrupted unpublished local Add must preserve the manifest")
            self.assertFalse((directory / "osprey.lock").exists())
            self.assertFalse((directory / "osprey.local.toml").exists())
            self.assertFalse((directory / "osprey.local.lock").exists())




class CollectorInterruptionTests(CoverageTestCase):
    def collector(self, prefix):
        fixture = self.simple_fixture()
        fixture.write_spec({"demo": {"report": own_report("demo", 4, 4)}})
        code = fixture.compiler.read_text()
        fixture.compiler.write_text(code.replace('args = sys.argv[1:]', prefix + '\nargs = sys.argv[1:]'))
        fixture.reports.mkdir(parents=True)
        return fixture

    def run_collector(self, fixture, **limits):
        checker = runpy.run_path(str(ROOT / "scripts/packages/package_coverage.py"))
        suite = fixture.root / "src/osprey/packages/demo/tests"
        env = {"OSPREY_PACKAGE_TIMEOUT_SECONDS": "5", "FAKE_PACKAGE_SPEC": str(fixture.root / "spec.json"), **limits}
        with patch.dict(os.environ, env):
            return checker["run_suite"](fixture.root, fixture.compiler, fixture.reports / "demo.json", fixture.reports / "demo.log", suite, "gc")

    def test_collector_deadline_preserves_output_and_stops_descendants(self):
        fixture = self.collector("print('started before hang', flush=True)\nprint('error before hang', file=sys.stderr, flush=True)\ntime.sleep(4)")
        marker = fixture.root / "orphan-wrote.txt"
        child = f"import time; from pathlib import Path; time.sleep(2.5); Path({str(marker)!r}).touch()"
        fixture.compiler.write_text(fixture.compiler.read_text().replace("print('started", f"import subprocess\nsubprocess.Popen([sys.executable, '-c', {child!r}])\nprint('started"))
        problems = self.run_collector(fixture, OSPREY_PACKAGE_TIMEOUT_SECONDS="1")
        self.assertTrue(problems, "a stuck collector must fail rather than validate coverage")
        log = (fixture.reports / "demo.log").read_text()
        for text in ("timeout", "started before hang", "error before hang"):
            self.assertIn(text, log)
        samples = [json.loads(line) for line in (fixture.reports / "demo.process.jsonl").read_text().splitlines()]
        self.assertTrue(any(len(row.get("processes", [])) == 2 for row in samples))
        self.assertEqual(samples[-1]["event"], "cleanup")
        time.sleep(3)
        self.assertFalse(marker.exists(), "a killed collector must not orphan its runtime")

    def test_collector_refuses_memory_and_output_growth_and_preserves_clean_control(self):
        cases = [("memory", "hold = bytearray(32 * 1024**2)\ntime.sleep(1.5)", {"OSPREY_PACKAGE_MAX_RSS_MB": "24"}),
                 ("output", "print('x' * 4096, flush=True)", {"OSPREY_PACKAGE_MAX_OUTPUT_MB": "0.001"})]
        for reason, program, limits in cases:
            with self.subTest(reason=reason):
                fixture = self.collector(program)
                self.assertTrue(self.run_collector(fixture, **limits))
                log = (fixture.reports / "demo.log").read_text()
                self.assertIn(f"{reason} limit exceeded", log)
        fixture = self.collector("")
        self.assertEqual(self.run_collector(fixture), [])
        self.assertIn("ok demo", (fixture.reports / "demo.stdout.log").read_text())
        self.assertTrue((fixture.reports / "demo.process.jsonl").is_file())

    def test_termination_interrupts_the_collector_and_keeps_diagnostics(self):
        fixture = self.collector("print('started before hang', flush=True)\ntime.sleep(4)")
        command = f"import sys; sys.path.insert(0, {str(ROOT / 'scripts/packages')!r}); from package_process import interrupts; import runpy; from pathlib import Path; c=runpy.run_path({str(ROOT / 'scripts/packages/package_coverage.py')!r})\nwith interrupts():\n p=c['run_suite'](Path({str(fixture.root)!r}),Path({str(fixture.compiler)!r}),Path({str(fixture.reports / 'demo.json')!r}),Path({str(fixture.reports / 'demo.log')!r}),Path({str(fixture.root / 'src/osprey/packages/demo/tests')!r}),'gc')\n raise SystemExit(1 if p else 0)"
        env = {**os.environ, "FAKE_PACKAGE_SPEC": str(fixture.root / "spec.json")}
        with subprocess.Popen([sys.executable, "-c", command], env=env) as process:
            try:
                deadline = time.monotonic() + 5
                output = fixture.reports / "demo.stdout.log"
                while not (output.exists() and "started" in output.read_text()) and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(output.exists() and "started" in output.read_text())
                process.terminate()
                self.assertEqual(process.wait(timeout=3), 1)
            finally:
                if process.poll() is None:
                    process.kill()
        self.assertIn("collector interrupted", (fixture.reports / "demo.log").read_text())



if __name__ == "__main__":
    unittest.main()
