"""Real-client regression: generated tool caches never enter a release."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

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


def publish(project, registry):
    binary = str(Path(os.environ.get("OSPREY_PACKAGE_CLIENT", ROOT / "target/osprey-pkg")).resolve())
    result = subprocess.run([binary], input=f"--json publish --folder {registry} --project {project}\n",
                            text=True, capture_output=True, check=True, timeout=30)
    return json.loads(result.stdout.splitlines()[0])


class PackagePublicationTests(unittest.TestCase):
    def test_generated_caches_are_excluded_before_reading(self):
        with tempfile.TemporaryDirectory(prefix="osprey-publish-proof-") as path:
            project, registry = Path(path) / "project", Path(path) / "registry"
            prepare(project)
            registry.mkdir()
            report = publish(project, registry)
            self.assertEqual(report["status"], 0, report)
            release = registry / "@demo/proof/1.1"
            self.assertEqual((release / "src/main.osp").read_text(), 'fn main() = print("ok")\n')
            for name in CACHES:
                self.assertFalse((release / name).exists(), f"Generated cache published: {name}")


if __name__ == "__main__":
    unittest.main()
