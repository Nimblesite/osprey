"""Real compiler transport acceptance for [EDITOR-ZED], without a Zed GUI.

OSPREY_TEST_COMPILER selects the already-built compiler; no build is started.
Framing is shared with the existing LSP benchmark rather than copied.
"""

import importlib.util
import os
from pathlib import Path
import queue
import re
import subprocess
import tempfile
import threading
import time
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "lsp_transport", ROOT / "scripts/tests/benchmark-lsp.py")
TRANSPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRANSPORT)
COMPILER = Path(os.environ.get("OSPREY_TEST_COMPILER", ROOT / "target/release/osprey")).resolve()
EXTENSION = Path(__file__).resolve().parent


class ZedManifest(unittest.TestCase):
    def test_registration_marker_isolation_and_canonical_grammar(self):
        manifest = tomllib.loads((EXTENSION / "extension.toml").read_text())
        workspace = tomllib.loads((ROOT / "src/toolchain/Cargo.toml").read_text())
        self.assertEqual(manifest["version"], workspace["workspace"]["package"]["version"])
        server = manifest["language_servers"]["osprey"]
        self.assertEqual(set(server["languages"]), set(server["language_ids"]))
        configs = [tomllib.loads(path.read_text())
                   for path in (EXTENSION / "languages").glob("*/config.toml")]
        self.assertEqual({config["name"] for config in configs}, set(server["languages"]))
        for config in configs:
            self.assertNotIn("py", config["path_suffixes"], "must not take over CPython")
        python = next(config for config in configs if config["name"] == "Osprey Python")
        marker = re.compile(python["first_line_pattern"])
        for line in ["# osprey: flavor=python", "#osprey: flavor=python", "  # osprey: flavor=python  "]:
            self.assertIsNotNone(marker.fullmatch(line), line)
        for line in ["import sys", "#!/usr/bin/python3", "# osprey: flavor=ml", "# osprey:flavor=python"]:
            self.assertIsNone(marker.fullmatch(line), line)
        grammar = manifest["grammars"]["osprey"]
        for name in ["grammar.js", "src/scanner.c", "src/parser.c"]:
            pinned = subprocess.check_output(
                ["git", "show", f'{grammar["rev"]}:{grammar["path"]}/{name}'], cwd=ROOT)
            self.assertEqual(pinned, (ROOT / grammar["path"] / name).read_bytes(),
                             "update the pinned grammar revision when canonical syntax changes")
        self.assertEqual((EXTENSION / "languages/osprey/highlights.scm").read_bytes(),
                         (ROOT / grammar["path"] / "queries/highlights.scm").read_bytes())


class ZedStdio(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="osprey zed ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.process = subprocess.Popen(
            [str(COMPILER), "lsp"], cwd=self.root,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(self.stop)
        self.incoming = queue.Queue()
        self.reports = {}
        threading.Thread(target=TRANSPORT.receive,
                         args=(self.process.stdout, self.incoming), daemon=True).start()
        self.next_id = 0
        capabilities = self.request("initialize", {
            "processId": None, "rootUri": self.root.as_uri(),
            "capabilities": {"general": {"positionEncodings": ["utf-16"]},
                             "workspace": {"workspaceEdit": {"documentChanges": True}}},
            "initializationOptions": {"diagnostics": {"scope": "openFiles"}},
        })["capabilities"]
        self.assertTrue(capabilities["hoverProvider"])
        self.assertTrue(capabilities["definitionProvider"])
        self.assertTrue(capabilities["documentFormattingProvider"])
        self.notify("initialized", {})

    def stop(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)
        for stream in [self.process.stdin, self.process.stdout, self.process.stderr]:
            stream.close()

    def notify(self, method, params):
        TRANSPORT.send(self.process, method, params)

    def pump(self, done):
        """Read messages, recording published diagnostics, until `done(message)` holds."""
        deadline = time.monotonic() + 20
        while True:
            try:
                message = self.incoming.get(timeout=max(0, deadline - time.monotonic()))
            except queue.Empty:
                return None
            if isinstance(message, Exception):
                raise message
            if message.get("method") == "textDocument/publishDiagnostics":
                report = message["params"]
                self.reports[report["uri"]] = report["diagnostics"]
            if done(message):
                return message

    def request(self, method, params):
        self.next_id += 1
        TRANSPORT.send(self.process, method, params, self.next_id)
        message = self.pump(lambda item: item.get("id") == self.next_id)
        self.assertIsNotNone(message, f"{method} must answer within 20 s")
        self.assertNotIn("error", message, message)
        return message.get("result")

    def published(self, uri, why):
        """Diagnostics run on a background worker [LSP-DIAGNOSTICS-SCOPE], so the
        publish for `uri` can trail the reply to a later request; wait for it."""
        if uri not in self.reports:
            self.pump(lambda item: item.get("method") == "textDocument/publishDiagnostics"
                      and item["params"]["uri"] == uri)
        self.assertIn(uri, self.reports, why)

    def document(self, name, language, source):
        path = self.root / name
        path.write_text(source)
        uri = path.as_uri()
        self.reports.pop(uri, None)
        self.notify("textDocument/didOpen", {"textDocument": {
            "uri": uri, "languageId": language, "version": 1, "text": source}})
        self.request("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
        self.published(uri, "opening must publish diagnostics")
        return uri

    def change(self, uri, version, source):
        """Send an edit and wait for the diagnostics it republishes."""
        self.reports.pop(uri, None)
        self.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": version},
                    "contentChanges": [{"text": source}]})
        self.request("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
        self.published(uri, "an edit must republish diagnostics")

    def errors(self, uri):
        """Error-severity diagnostics currently published for `uri`."""
        return [d for d in self.reports[uri] if d.get("severity") == 1]

    @staticmethod
    def at(uri, line, character):
        return {"textDocument": {"uri": uri},
                "position": {"line": line, "character": character}}

    def test_flavors_requests_edits_and_shutdown(self):
        cases = [
            ("a.osp", "osprey", "fn double(x) = wrapMul(x, 2)\nlet total = double(3)\n", 1, 13),
            ("b.ospml", "osprey-ml", "double x = wrapMul x 2\nlet total = double 3\n", 1, 13),
            ("c.py", "osprey-python", "# osprey: flavor=python\ndef double(x):\n    return wrapMul(x, 2)\ntotal = double(3)\n", 3, 9),
        ]
        for name, language, source, line, column in cases:
            with self.subTest(language=language):
                uri = self.document(name, language, source)
                self.assertFalse(self.errors(uri))
                hover = self.request("textDocument/hover", self.at(uri, line, column))
                self.assertIn("double", str(hover))
                definition = self.request("textDocument/definition", self.at(uri, line, column))
                self.assertTrue(definition, "definition must resolve")
                self.assertIn(uri, str(definition))
                self.assertEqual(definition[0]["range"]["start"]["line"], 1 if language == "osprey-python" else 0)
                references = self.request("textDocument/references", {
                    **self.at(uri, line, column), "context": {"includeDeclaration": True}})
                self.assertEqual(len(references), 2, references)
                self.assertEqual({item["uri"] for item in references}, {uri})
                symbols = self.request("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
                self.assertIn("double", str(symbols))
                # [LSP-WORKSPACE] explicitly supports invocation on the callee,
                # including ML calls without parentheses.
                signature = self.request("textDocument/signatureHelp", self.at(uri, line, column))
                self.assertIn("double", str(signature))
                self.assertEqual(signature["activeParameter"], 0)
                completion = self.request("textDocument/completion", self.at(uri, line, column))
                self.assertIn("double", str(completion))
                formatting = self.request("textDocument/formatting", {
                    "textDocument": {"uri": uri},
                    "options": {"tabSize": 4, "insertSpaces": True}})
                self.assertIsInstance(formatting, list)
                self.change(uri, 2, source.replace("double(3)", "missing(3)").replace("double 3", "missing 3"))
                self.assertTrue(self.errors(uri))
                self.change(uri, 3, source)
                self.assertFalse(self.errors(uri))
                self.notify("textDocument/didClose", {"textDocument": {"uri": uri}})
        self.assertIsNone(self.request("shutdown", None))
        self.notify("exit", None)
        self.assertEqual(self.process.wait(timeout=10), 0)

    def test_utf16_navigation_after_astral_text(self):
        source = 'fn double(x) = wrapMul(x, 2)\nlet total = { let face = "😀"; double(3) }\n'
        uri = self.document("unicode.osp", "osprey", source)
        line = source.splitlines()[1]
        column = len(line[:line.index("double") + 1].encode("utf-16-le")) // 2
        hover = self.request("textDocument/hover", self.at(uri, 1, column))
        self.assertIn("double", str(hover))
        definition = self.request("textDocument/definition", self.at(uri, 1, column))
        self.assertTrue(definition)
        self.assertIn(uri, str(definition))

    def test_mixed_flavor_project_navigation_and_live_helper_edits(self):
        (self.root / "src").mkdir()
        (self.root / "osprey.toml").write_text(
            '[project]\nname = "zed-mixed"\nsource_roots = ["src"]\n'
            'default_namespace = "zed"\nentry = "src/main.osp"\n')
        helper = "increment value = wrapAdd value 1\n"
        helper_path = self.root / "src/helper.ospml"
        helper_path.write_text(helper)
        uri = self.document("src/main.osp", "osprey", "let total = increment(3)\n")
        self.assertFalse(self.errors(uri))
        definition = self.request("textDocument/definition", self.at(uri, 0, 15))
        self.assertIn(helper_path.as_uri(), str(definition))
        hover = self.request("textDocument/hover", self.at(uri, 0, 15))
        self.assertIn("increment", str(hover))
        helper_uri = self.document("src/helper.ospml", "osprey-ml", helper)
        self.change(helper_uri, 2, "increment value = missingName\n")
        self.assertTrue(self.errors(helper_uri), self.reports)
        self.change(helper_uri, 3, helper)
        self.assertFalse(self.errors(helper_uri))
        self.assertIn(helper_path.as_uri(), str(self.request(
            "textDocument/definition", self.at(uri, 0, 15))))

    def test_quick_fix_edits_the_live_document_and_removes_its_warning(self):
        source = "fn add(a: int, b: int) -> int = satAdd(a, b)\nlet total = add(1, 2)\n"
        uri = self.document("fix.osp", "osprey", source)
        warnings = [item for item in self.reports[uri] if item.get("code") == "redundant-annotation"]
        self.assertEqual(len(warnings), 2, self.reports[uri])
        actions = self.request("textDocument/codeAction", {
            "textDocument": {"uri": uri}, "range": warnings[0]["range"],
            "context": {"diagnostics": warnings}})
        self.assertTrue(actions, "the annotation warning must offer an actual edit")
        # [LSP-WARNING-ACTIONS] returns versioned TextDocumentEdit, which Zed
        # supports; plain WorkspaceEdit.changes would discard that safeguard.
        document = actions[0]["edit"]["documentChanges"][0]
        self.assertEqual(document["textDocument"], {"uri": uri, "version": 1})
        edits = document["edits"]
        lines = source.splitlines(keepends=True)
        for edit in sorted(edits, key=lambda item: (item["range"]["start"]["line"],
                                                    item["range"]["start"]["character"]), reverse=True):
            start, end = edit["range"]["start"], edit["range"]["end"]
            self.assertEqual(start["line"], end["line"], "this fixture has only single-line annotations")
            line = lines[start["line"]]
            lines[start["line"]] = line[:start["character"]] + edit["newText"] + line[end["character"]:]
        fixed = "".join(lines)
        self.assertNotEqual(fixed, source)
        self.change(uri, 2, fixed)
        remaining = [item for item in self.reports[uri] if item.get("code") == "redundant-annotation"]
        self.assertLess(len(remaining), len(warnings), self.reports[uri])
        self.assertFalse(self.errors(uri))


if __name__ == "__main__":
    unittest.main()
