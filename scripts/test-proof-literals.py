#!/usr/bin/env python3
"""Actual recorded Unicode proofs must check without crashing [PROOF-EXPORT]."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("proof_openai", ROOT / "scripts/proof-openai.py")
PROOF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROOF)
GROUP = json.loads((ROOT / "examples/proof/kernel/exports/string_cases.json").read_text())


def commands():
    selected = os.environ.get("OSPREY_PROOF_CHECKER")
    prefix = os.environ.get("OSPREY_PROOF_CHECKER_PREFIX", str(PROOF.OUTPUT / "check-batch"))
    binaries = [Path(selected)] if selected else [Path(prefix + "." + suffix) for suffix in ("gc", "arc", "wasm")]
    return [(str(binary), PROOF.binary_command(binary)) for binary in binaries]

class LiteralTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows, _, cls.theorems = PROOF.check_files(GROUP)
        PROOF.verify_lean(GROUP)

    def test_recorded_empty_and_supplementary_unicode_constructor_proofs(self):
        for backend, command in commands():
            for name in GROUP["theorems"]:
                with self.subTest(backend=backend, name=name):
                    arguments, source = PROOF.checker_request(command, PROOF.export_path(GROUP), name)
                    result = subprocess.run(arguments, input=source, cwd=ROOT,
                                            text=True, capture_output=True, timeout=120)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(result.stdout, f"PASS {name} | axioms: none\n")
                    PROOF.rejected(command, self.rows, self.theorems[name], name, True)
                    PROOF.rejected(command, self.rows, self.theorems[name], name, False)
                    print(f"{backend}: PASS {name}; both mutations rejected", flush=True)


if __name__ == "__main__":
    unittest.main()
