"""Real-client publication and installed-source integrity regressions."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ('[project]\nname="publication-proof"\nsource_roots=["src"]\n'
            '[package]\nname="@demo/proof"\nlicense="MIT"\nsummary="proof"\nkeywords=["proof"]\n')
CACHES = ["src/.deslop/cache/fingerprint.bin", "src/.osprey/cache/entry.bin",
          "src/__pycache__/module.pyc", "src/nested/.git/index"]


def prepare(project):
    project.mkdir()
    (project / "osprey.toml").write_text(MANIFEST)
    (project / "src").mkdir()
    (project / "src/main.osp").write_text('fn main() = print("ok")\n')
    for name in CACHES:
        path = project / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"\x80binary-cache")


def command(*words, directory=None):
    binary = str(Path(os.environ.get("OSPREY_PACKAGE_CLIENT", ROOT / "target/osprey-pkg")).resolve())
    result = subprocess.run([binary], input="--json " + " ".join(map(str, words)) + "\n",
                            cwd=directory, text=True, capture_output=True, check=True, timeout=30)
    return json.loads(result.stdout.splitlines()[0])


def wait_ready(child, origin):
    for _ in range(100):
        if child.poll() is not None:
            raise AssertionError("Registry exited before readiness")
        try:
            with urlopen(origin + "/v1/health", timeout=.2) as response:
                if response.status == 200:
                    return
        except OSError:
            time.sleep(.05)
    raise AssertionError("Registry did not become ready")


def stop(child):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


@contextmanager
def native_registry(work, registry):
    server = work / "http"
    server.mkdir()
    shutil.copy2(os.environ.get("OSPREY_REGISTRY_SERVER", ROOT / ".osprey/e2e/registry-server"), server / "server")
    with socket.socket() as available:
        available.bind(("127.0.0.1", 0))
        port = available.getsockname()[1]
    (server / "registry-server.json").write_text(json.dumps({"address": "127.0.0.1", "port": port, "root": str(registry)}))
    with (server / "server.log").open("wb") as log:
        child = subprocess.Popen([str(server / "server")], cwd=server, stdout=log, stderr=log)
        try:
            origin = f"http://127.0.0.1:{port}"
            wait_ready(child, origin)
            yield origin, child
        finally:
            stop(child)


@contextmanager
def installed_package():
    with tempfile.TemporaryDirectory(prefix="osprey-fetch-integrity-") as path:
        work = Path(path)
        project, registry, consumer = (work / name for name in ("project", "registry", "consumer"))
        prepare(project)
        registry.mkdir()
        assert command("publish", "--folder", registry, "--project", project)["status"] == 0
        consumer.mkdir()
        (consumer / "src").mkdir()
        (consumer / "osprey.toml").write_text('[project]\nname="consumer"\nsource_roots=["src"]\n[dependencies]\n"@demo/proof"={epoch=1,min_release=1}\n')
        with native_registry(work, registry) as (origin, child):
            assert command("lock", "--registry", origin, "--project", consumer)["status"] == 0
            assert command("verify", "--offline", "--project", consumer)["status"] == 0
            tree = next((consumer / ".osprey/packages/@demo/proof").iterdir())
            yield consumer, tree, origin, child


class PackagePublicationTests(unittest.TestCase):
    def test_generated_caches_are_excluded_before_reading(self):
        with tempfile.TemporaryDirectory(prefix="osprey-publish-proof-") as path:
            project, registry = Path(path) / "project", Path(path) / "registry"
            prepare(project)
            registry.mkdir()
            report = command("publish", "--folder", registry, "--project", project)
            self.assertEqual(report["status"], 0, report)
            release = registry / "@demo/proof/1.1"
            self.assertEqual((release / "src/main.osp").read_text(), 'fn main() = print("ok")\n')
            for name in CACHES:
                self.assertFalse((release / name).exists(), f"Generated cache published: {name}")


class PackageFetchIntegrityTests(unittest.TestCase):
    def test_refetch_cannot_succeed_with_a_corrupt_tree(self):
        for kind in ("extra", "direct-extra", "modified", "symlink", "offline"):
            with self.subTest(kind=kind), installed_package() as (project, tree, origin, child):
                corrupt(kind, project, tree, child)
                report = (command("fetch", "@demo/proof", "--registry", origin, directory=project)
                          if kind == "direct-extra" else command("fetch", "--registry", origin, "--project", project))
                verified = command("verify", "--offline", "--project", project)
                self.assertTrue(report["status"] != 0 or verified["status"] == 0,
                                f"Successful fetch left a corrupt tree: {report}; {verified}")
                if kind == "modified":
                    self.assertEqual((report["status"], verified["status"]), (0, 0))
                    self.assertEqual((tree / "src/main.osp").read_text(), 'fn main() = print("ok")\n')
                if kind in ("symlink", "offline"):
                    self.assertNotEqual(report["status"], 0, report)
                if kind == "symlink":
                    self.assertEqual((project / "outside.txt").read_text(), "preserve\n")
                if kind == "offline":
                    self.assertNotEqual(verified["status"], 0, verified)


def corrupt(kind, project, tree, child):
    target = tree / ("src/unexpected.osp" if kind in ("extra", "direct-extra") else "src/main.osp")
    if kind == "symlink":
        outside = project / "outside.txt"
        outside.write_text("preserve\n")
        target.unlink()
        target.symlink_to(outside)
    else:
        target.write_text("fn wrong() = 0\n")
    if kind == "offline":
        stop(child)


if __name__ == "__main__":
    unittest.main()
