"""Isolated E2E controls for the baseline driver, never model-quality evidence."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import unittest

HERE = Path(__file__).resolve().parent


def module(name):
    spec = importlib.util.spec_from_file_location(name, HERE / f"{name}.py")
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


class EvaluationUseCases(unittest.TestCase):
    def test_coding_judge_checks_outputs_rejects_unsafe_code_and_times_out(self):
        judge = module("judge")
        cases = [{"input": [1, 2], "expected": 3}, {"input": [], "expected": 0}]
        self.assertTrue(judge.check("def solve(value):\n    return sum(value)", cases)["passed"])
        for source in ("def solve(value):\n    return 3", "import os\ndef solve(value):\n    return os.environ", "def solve(value):\n    return open('/etc/passwd').read()", "def solve(value):\n    return value.__class__"):
            self.assertFalse(judge.check(source, cases)["passed"], source)
        timed = judge.check("def solve(value):\n    while True: pass", cases, timeout=0.2)
        self.assertFalse(timed["passed"])
        self.assertIn("timeout", timed["error"])

    def test_real_http_multiturn_recovery_and_fail_closed_transport(self):
        sys.path.insert(0, str(HERE))
        corpus = module("tool_cases")
        driver = module("baseline")
        cases = corpus.cases()
        self.assertEqual(len(cases), 200)
        self.assertEqual(len({case["id"] for case in cases}), 200)
        seen, histories = [], []
        failure = {"invalid": True, "intent": True, "http": False}
        by_id = {case["id"]: case for case in cases}

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if failure["http"]:
                    self.send_error(503)
                    return
                latest = next(m for m in reversed(body["messages"]) if m["role"] == "user")
                case = by_id[latest["content"].split("\n", 1)[0]]
                histories.append(len(body["messages"]))
                call = {"function": {"name": case["name"], "arguments": case["arguments"]}}
                if failure["invalid"]:
                    call = {"function": {"name": "undeclared", "arguments": {}}}
                    failure["invalid"] = False
                elif failure["intent"]:
                    call = {"function": {"name": case["name"], "arguments": {**case["arguments"], "content": "wrong content"}}}
                    failure["intent"] = False
                seen.append(call)
                payload = {"message": {"role": "assistant", "content": "", "tool_calls": [call]}, "done": True, "eval_count": 4, "eval_duration": 1000000000, "prompt_eval_count": 8, "prompt_eval_duration": 2000000000}
                raw = json.dumps(payload).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            client = driver.Client(f"http://127.0.0.1:{server.server_port}", "fixture-only", 0.6)
            report = driver.evaluate_tools(client, cases)
            self.assertEqual(report["passed"], 200)
            self.assertEqual(report["invalid_calls"], 1)
            self.assertEqual(report["attempts"], 212)
            self.assertEqual(report["schema_valid_calls"], 211)
            self.assertEqual(report["intent_mismatches"], 1)
            self.assertEqual(report["injected_errors"], 10)
            self.assertGreaterEqual(max(histories), 40)
            self.assertTrue(any(m["role"] == "tool" for m in client.last_messages))
            self.assertEqual(client.measurements[0]["tokens_per_second"], 4.0)
            self.assertEqual(client.measurements[0]["prompt_tokens_per_second"], 4.0)
            failure["http"] = True
            with self.assertRaisesRegex(RuntimeError, "503"):
                driver.evaluate_tools(client, cases[:1])
        finally:
            server.shutdown()
            server.server_close()
            worker.join()
            sys.path.remove(str(HERE))

    def test_corpus_and_reference_solution_acceptance(self):
        corpus_file = HERE / "coding_tasks.json"
        corpus = json.loads(corpus_file.read_text())
        self.assertEqual(len(corpus), 30)
        self.assertEqual(len({case["id"] for case in corpus}), 30)
        judge = module("judge")
        for case in corpus:
            with self.subTest(case=case["id"]):
                self.assertTrue((HERE.parents[2] / case["source"]).is_file())
                self.assertGreaterEqual(len(case["cases"]), 4)
                result = judge.check(case["reference"], case["cases"])
                self.assertTrue(result["passed"], result)
                self.assertFalse(judge.check("def solve(value):\n    return None", case["cases"])["passed"])

    def test_cli_never_passes_a_dead_endpoint_or_unpinned_weights(self):
        with tempfile.TemporaryDirectory() as directory:
            weights = Path(directory) / "fixture.gguf"
            weights.write_bytes(b"GGUF" + b"fixture-only")
            output = Path(directory) / "result.json"
            command = [sys.executable, str(HERE / "baseline.py"), "--model", "fixture-only", "--gguf", str(weights), "--quant", "UD-Q4_K_XL", "--output", str(output), "--base-url", "http://127.0.0.1:1", "--temperature", "0.6"]
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("error", result.stderr.lower())
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
