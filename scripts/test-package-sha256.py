"""Assert package hashing uses allocation-free machine-word rotation."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def emit_ir(directory):
    sources = [str(ROOT / "packages/manager/core/src" / name)
               for name in ("bytes.osp", "sha256.osp", "policy.osp")]
    roots = ", ".join('"' + path + '"' for path in ["main.osp", *sources])
    (directory / "osprey.toml").write_text(
        '[project]\nname="sha-proof"\nsource_roots=[' + roots +
        ']\ndefault_namespace="sha_proof"\nentry="main.osp"\n')
    (directory / "main.osp").write_text(
        'namespace sha_proof;\nimport registry::Sha256\nimport registry::Policy\n'
        'fn run() = { print(Sha256::ofString("abc"))\n0 }\n'
        'fn main() = Policy::guarded(run)\n')
    return subprocess.run([os.environ.get("OSPREY_COMPILER", str(ROOT / "target/ci/osprey")),
                           str(directory), "--llvm", "--memory=gc"],
                          text=True, capture_output=True, check=True).stdout


class PackageSha256Tests(unittest.TestCase):
    def test_rotation_is_allocation_free_integer_bitwise_code(self):
        with tempfile.TemporaryDirectory(prefix="osprey-sha-proof-") as directory:
            ir = emit_ir(Path(directory))
        rotation = re.search(r"^define[^\n]*_4x726f7472\([^\n]*\{\n(.*?)^}",
                             ir, re.MULTILINE | re.DOTALL)
        self.assertIsNotNone(rotation, "Actual registry::Sha256::rotr must be compiled")
        body = rotation.group(1)
        self.assertIn("lshr i64", body)
        self.assertIn("shl i64", body)
        self.assertNotRegex(body, r"\b(?:sdiv|udiv|srem|urem) i64")
        self.assertNotRegex(body, r"\bcall\b", "Rotation must not allocate or call table helpers")


if __name__ == "__main__":
    unittest.main()
