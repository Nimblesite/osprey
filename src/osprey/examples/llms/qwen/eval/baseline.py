#!/usr/bin/env python3
"""Plan 0049 Phase 0 baseline driver [LLM-BASELINE].

Drives an unmodified Ollama over loopback HTTP: the 200 tool-call cases from
tool_cases.py and the 30-task coding corpus (coding_tasks.json) judged by
judge.py. Pass counts are deterministic harness controls, never model-quality
evidence. No baseline is accepted without genuine weight, rate and RSS proof;
weight, transport or corpus failure refuses evaluation before any result write.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

from jsonschema import Draft202012Validator

HERE = Path(__file__).resolve().parent
LOOPBACK_HOSTS = {"127.0.0.1", "localhost", "::1"}
TEMPERATURES = (0.6, 1.0)
MAX_TOOL_RETRIES = 2
MAX_NEW_TOKENS = 8192
REQUEST_TIMEOUT_SECONDS = 1800
CODING_TASK_COUNT = 30
TOOL_CASE_COUNT = 200
GGUF_MAGIC = b"GGUF"
MB = 1024.0
TOOL_SYSTEM = (
    "You are a tool-calling harness agent. Reply with exactly one tool call per request. "
    "When a tool result reports a recoverable error, retry the same intent with a corrected call."
)
CODING_SYSTEM = (
    "You write small pure Python functions. Reply with only the source of solve(value); "
    "no imports, classes, globals, file or network access, and no prose."
)

_SIBLINGS = {}


def sibling(name):
    """Load a sibling module (tool_cases, judge) without import-time side effects."""
    if name not in _SIBLINGS:
        spec = importlib.util.spec_from_file_location(f"baseline.{name}", HERE / f"{name}.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        _SIBLINGS[name] = module
    return _SIBLINGS[name]


def assert_loopback(base_url):
    parsed = urllib.parse.urlparse(base_url)
    if parsed.scheme != "http" or (parsed.hostname or "").lower() not in LOOPBACK_HOSTS:
        raise RuntimeError(f"only loopback http endpoints are allowed, got {base_url!r}")


def http_json(method, url, payload, timeout):
    body = json.dumps(payload).encode() if payload is not None else None
    request = urllib.request.Request(url, data=body, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        with error:
            raise RuntimeError(f"{method} {url} failed with HTTP status {error.code} {error.reason}") from error
    except (urllib.error.URLError, OSError, ValueError) as error:
        raise RuntimeError(f"{method} {url} transport failure: {error}") from error


def nanosecond_rate(count, duration_ns):
    if not count or not duration_ns or duration_ns <= 0:
        return None
    return count * 1e9 / duration_ns


def measurement(response, elapsed):
    return {
        "tokens_per_second": nanosecond_rate(response.get("eval_count"), response.get("eval_duration")),
        "prompt_tokens_per_second": nanosecond_rate(response.get("prompt_eval_count"), response.get("prompt_eval_duration")),
        "eval_count": response.get("eval_count"),
        "prompt_eval_count": response.get("prompt_eval_count"),
        "elapsed_seconds": round(elapsed, 3),
    }


def read_journal(path):
    """Completed generations by request hash, so a failed long run resumes instead of restarting."""
    if not path.exists():
        return {}
    try:
        entries = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    except ValueError as error:
        raise RuntimeError(f"unreadable journal {path}: {error}") from error
    return {entry["key"]: entry for entry in entries}


class Client:
    """Loopback Ollama /api/chat client (stream=false); failures raise RuntimeError naming the status."""

    def __init__(self, base_url, model, temperature, timeout=REQUEST_TIMEOUT_SECONDS, journal=None):
        assert_loopback(base_url)
        self.base_url = base_url.rstrip("/")
        self.model, self.temperature, self.timeout = model, temperature, timeout
        self.last_messages, self.measurements = [], []
        self.journal = Path(journal) if journal else None
        self.completed = read_journal(self.journal) if self.journal else {}

    def chat(self, messages, tools=None):
        payload = {"model": self.model, "messages": messages, "stream": False,
                   "options": {"temperature": self.temperature, "num_predict": MAX_NEW_TOKENS}}
        if tools:
            payload["tools"] = tools
        self.last_messages = messages
        key = hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()
        if key in self.completed:
            entry = self.completed[key]
        else:
            started = time.monotonic()
            response = http_json("POST", f"{self.base_url}/api/chat", payload, self.timeout)
            entry = {"key": key, "response": response, "elapsed": time.monotonic() - started}
            if self.journal:
                self.completed[key] = entry
                with self.journal.open("a", encoding="utf-8") as journal:
                    journal.write(json.dumps(entry, ensure_ascii=False) + "\n")
        self.measurements.append(measurement(entry["response"], entry["elapsed"]))
        return entry["response"]

    def version(self):
        return http_json("GET", f"{self.base_url}/api/version", None, self.timeout)

    def show(self):
        return http_json("POST", f"{self.base_url}/api/show", {"model": self.model}, self.timeout)


def parse_arguments(raw):
    if isinstance(raw, dict):
        return raw
    if isinstance(raw, str):
        try:
            value = json.loads(raw)
        except ValueError:
            return None
        return value if isinstance(value, dict) else None
    return None


def validate_call(call, schemas):
    """Return (invocation, error) for one tool call [LLM-BASELINE]: a named, schema-valid function or an error."""
    name = (call or {}).get("function", {}).get("name")
    if name not in schemas:
        return None, f"unknown or undeclared function {name!r}"
    arguments = parse_arguments(call["function"].get("arguments"))
    if arguments is None:
        return None, f"arguments for {name!r} are not a JSON object"
    errors = sorted(Draft202012Validator(schemas[name]).iter_errors(arguments), key=lambda e: e.json_path)
    if errors:
        detail = "; ".join(f"{e.json_path}: {e.message}" for e in errors)
        return None, f"{name} arguments fail their JSON Schema: {detail}"
    return (name, arguments), None


def tool_message(content):
    return {"role": "tool", "content": content}


def accept_call(history, case, invocation, report, injection):
    """Apply the invocation that matched the requested intent; tool results are simulated, never real effects.

    An injected execution failure counts and replies as recoverable, returns False to force a same-turn
    retry, and only the later successful call scores passed; the injection fires at most once per turn.
    """
    if case.get("inject_error") and not injection["done"]:
        injection["done"] = True
        report["injected_errors"] += 1
        history.append(tool_message(f"error: injected execution failure for {invocation[0]} (recoverable); retry this call"))
        return False
    report["passed"] += 1
    history.append(tool_message(f"ok: {invocation[0]} completed (simulated; the baseline runs no real tools)"))
    return True


def accept_message(history, assistant, case, report, schemas, injection):
    """Validate and count every call; succeed only when the first valid call matches the requested intent."""
    calls = assistant.get("tool_calls") or []
    outcomes = [validate_call(call, schemas) for call in calls] or [(None, "no tool call in the response")]
    for invocation, _error in outcomes:
        report["schema_valid_calls" if invocation else "invalid_calls"] += 1
    first = next((invocation for invocation, _error in outcomes if invocation), None)
    if first is None:
        history.append(tool_message(f"error: {outcomes[0][1]} (recoverable); retry with a corrected tool call"))
        return False
    if first != (case["name"], case["arguments"]):
        report["intent_mismatches"] += 1
        history.append(tool_message("error: schema-valid call does not match the requested intent (recoverable); retry with a corrected tool call"))
        return False
    return accept_call(history, case, first, report, injection)


def run_turn(client, history, case, tools, report, schemas):
    """One goal: up to 1 + MAX_TOOL_RETRIES attempts, preserving history; stop only on intent success."""
    history.append({"role": "user", "content": f"{case['id']}\n{case['prompt']}"})
    injection = {"done": False}
    for _ in range(1 + MAX_TOOL_RETRIES):
        report["attempts"] += 1
        assistant = client.chat(history, tools)["message"]
        history.append(assistant)
        if accept_message(history, assistant, case, report, schemas, injection):
            return
    report["unresolved"].append(case["id"])


def group_sessions(cases):
    grouped = []
    for case in cases:
        key = case.get("session")
        if not grouped or grouped[-1][0] != key:
            grouped.append((key, []))
        grouped[-1][1].append(case)
    return [members for _key, members in grouped]


def run_session(client, cases, tools, report, schemas):
    history = [{"role": "system", "content": TOOL_SYSTEM}]
    for case in cases:
        run_turn(client, history, case, tools, report, schemas)


def evaluate_tools(client, cases):
    """Drive every case over its sessions, offering the declared TOOLS [LLM-BASELINE]."""
    corpus = sibling("tool_cases")
    tools, schemas = corpus.TOOLS, {tool["function"]["name"]: tool["function"]["parameters"] for tool in corpus.TOOLS}
    if not cases:
        raise RuntimeError("no tool cases to evaluate")
    report = {"goals": len(cases), "sessions": 0, "attempts": 0, "schema_valid_calls": 0,
              "invalid_calls": 0, "passed": 0, "intent_mismatches": 0, "injected_errors": 0, "unresolved": []}
    for session_cases in group_sessions(cases):
        report["sessions"] += 1
        run_session(client, session_cases, tools, report, schemas)
    return report


def coding_tasks(path=HERE / "coding_tasks.json"):
    tasks_file = Path(path)
    if not tasks_file.is_file():
        raise RuntimeError(f"coding corpus not found: {tasks_file}")
    tasks = json.loads(tasks_file.read_text(encoding="utf-8"))
    if len(tasks) != CODING_TASK_COUNT or len({task.get("id") for task in tasks}) != CODING_TASK_COUNT:
        raise RuntimeError(f"coding corpus must hold exactly {CODING_TASK_COUNT} tasks with unique ids, found {len(tasks)}")
    return tasks


def coding_prompt(task):
    description = task.get("task") or task.get("prompt") or task.get("description")
    return f"Task {task['id']} (repo file {task['source']}):\n{description}\n\nImplement solve(value) only."


def strip_fences(text):
    blocks = re.findall(r"```(?:[a-zA-Z0-9_+-]*)\n?(.*?)```", text, re.DOTALL)
    return "\n".join(blocks).strip() if blocks else text.strip()


def evaluate_task(client, judge, task):
    if not judge.check(task["reference"], task["cases"])["passed"]:
        raise RuntimeError(f"reference solution for {task['id']} fails the judge; corpus is not authoritative")
    reply = client.chat([{"role": "system", "content": CODING_SYSTEM},
                         {"role": "user", "content": coding_prompt(task)}])["message"].get("content") or ""
    outcome = judge.check(strip_fences(reply), task["cases"])
    return {"id": task["id"], "passed": bool(outcome["passed"]), "error": outcome.get("error")}


def evaluate_coding(client):
    """Exactly 30 corpus tasks, each judged independently by judge.check [LLM-BASELINE]."""
    judge = sibling("judge")
    results = [evaluate_task(client, judge, task) for task in coding_tasks()]
    return {"total": len(results), "passed": sum(result["passed"] for result in results), "tasks": results}


def verify_gguf(path):
    """Check the GGUF magic and return the file's SHA-256 digest."""
    gguf = Path(path)
    with gguf.open("rb") as handle:
        if handle.read(len(GGUF_MAGIC)) != GGUF_MAGIC:
            raise RuntimeError(f"{gguf} is not a GGUF file: bad magic")
    digest = hashlib.sha256()
    with gguf.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def modelfile_from_lines(show):
    modelfile = show.get("modelfile") or ""
    return [line.split(None, 1)[1].strip() for line in modelfile.splitlines() if line.strip().upper().startswith("FROM")]


def verify_weights(show, digest):
    """The /api/show Modelfile FROM must prove the same weights before any evaluation."""
    lines = modelfile_from_lines(show)
    if not any(digest.lower() in line.lower() for line in lines):
        raise RuntimeError(f"weights mismatch: /api/show Modelfile FROM {lines or ['<missing>']} does not prove sha256-{digest}")
    return lines


def ps_tree():
    run = subprocess.run(["ps", "-axo", "pid=,ppid=,rss="], capture_output=True, text=True, timeout=15, check=False)
    tree = {}
    for row in run.stdout.splitlines():
        parts = row.split()
        if len(parts) == 3 and parts[0].isdigit():
            tree[int(parts[0])] = (int(parts[1]), float(parts[2]))
    return tree


def tree_rss_mb(tree, root):
    if root not in tree:
        return None
    children = {}
    for pid, (parent, _rss) in tree.items():
        children.setdefault(parent, []).append(pid)
    total, stack = 0.0, [root]
    while stack:
        pid = stack.pop()
        total += tree[pid][1]
        stack.extend(children.get(pid, []))
    return round(total / MB, 1)


class RssSampler:
    """Samples the real server process tree's RSS with ps; absent evidence stays None, never 0."""

    def __init__(self, pid, interval=0.5):
        self.pid, self.interval = pid, interval
        self.peak, self.samples, self.failures = None, 0, 0
        self._stop, self._thread = threading.Event(), None

    def start(self):
        if self.pid:
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._thread.start()

    def _run(self):
        while not self._stop.wait(self.interval):
            try:
                value = tree_rss_mb(ps_tree(), self.pid)
            except (OSError, subprocess.SubprocessError, ValueError):
                value = None
            if value is None:
                self.failures += 1
            else:
                self.samples += 1
                self.peak = value if self.peak is None else max(self.peak, value)

    def stop(self):
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=15)


def median(values):
    ordered = sorted(value for value in values if value is not None)
    if not ordered:
        return None
    middle = len(ordered) // 2
    return ordered[middle] if len(ordered) % 2 else (ordered[middle - 1] + ordered[middle]) / 2


def rate_summary(client):
    return {
        "responses": len(client.measurements),
        "median_tokens_per_second": median([m["tokens_per_second"] for m in client.measurements]),
        "median_prompt_tokens_per_second": median([m["prompt_tokens_per_second"] for m in client.measurements]),
        "total_elapsed_seconds": round(sum(m["elapsed_seconds"] for m in client.measurements), 3),
    }


def ollama_info(version, show, from_lines):
    return {"version": version, "parameters": show.get("parameters"),
            "capabilities": show.get("capabilities"), "modelfile_from": from_lines}


def run_identity(args, digest):
    return {"model": args.model, "quant": args.quant, "temperature": args.temperature,
            "base_url": args.base_url, "gguf": {"path": str(args.gguf), "sha256": digest}}


def is_full_baseline(tools, coding, rates, sampler):
    """Full acceptance requires genuine rate and RSS evidence; fixtures and gaps refuse it [LLM-BASELINE]."""
    return (sampler.peak is not None and sampler.samples > 0
            and rates["median_tokens_per_second"] is not None
            and rates["median_prompt_tokens_per_second"] is not None
            and coding["total"] == CODING_TASK_COUNT and tools["goals"] == TOOL_CASE_COUNT)


def limitations(rates, sampler):
    notes = ["pass counts are deterministic harness controls, not model-quality evidence"]
    if sampler.peak is None:
        notes.append("peak_rss_mb is null: no --server-pid RSS evidence; full baseline not accepted")
    if rates["median_tokens_per_second"] is None:
        notes.append("no genuine token-rate evidence was recorded")
    return notes


def build_result(args, client, digest, version, show, from_lines, tools, coding, sampler):
    rates = rate_summary(client)
    return {"schema": "src/osprey/examples/llms/qwen/eval/baseline/1", "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            **run_identity(args, digest), "max_new_tokens": MAX_NEW_TOKENS, "ollama": ollama_info(version, show, from_lines),
            "tools": tools, "coding": coding, "rates": rates,
            "tests_run": {"tool_goals": tools["goals"], "tool_sessions": tools["sessions"],
                          "coding_tasks": coding["total"], "chat_requests": rates["responses"]},
            "peak_rss_mb": sampler.peak, "rss_samples": sampler.samples, "rss_failures": sampler.failures,
            "full_baseline": is_full_baseline(tools, coding, rates, sampler),
            "limitations": limitations(rates, sampler)}


def write_atomic(path, payload):
    """Write the result only after the complete suite, atomically via os.replace."""
    target = Path(path)
    temporary = target.with_name(f"{target.name}.tmp{os.getpid()}")
    temporary.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    os.replace(temporary, target)


def parse_args(argv):
    parser = argparse.ArgumentParser(description="Plan 0049 Phase 0 baseline [LLM-BASELINE]: Ollama tool + coding controls.")
    parser.add_argument("--model", required=True, help="Ollama model name serving the GGUF")
    parser.add_argument("--gguf", required=True, help="path to the GGUF file to verify (magic + SHA-256)")
    parser.add_argument("--quant", required=True, help="quantization label, e.g. UD-Q4_K_XL")
    parser.add_argument("--temperature", type=float, choices=TEMPERATURES, required=True)
    parser.add_argument("--base-url", default="http://127.0.0.1:11434", help="loopback Ollama endpoint")
    parser.add_argument("--output", required=True, help="atomic result JSON path")
    parser.add_argument("--journal", default=None, help="JSONL of completed generations; rerunning resumes from it")
    parser.add_argument("--server-pid", type=int, default=None, help="optional Ollama server pid for RSS sampling")
    return parser.parse_args(argv)


def run_baseline(args):
    digest = verify_gguf(args.gguf)
    client = Client(args.base_url, args.model, args.temperature, journal=args.journal)
    version = client.version().get("version")
    show = client.show()
    from_lines = verify_weights(show, digest)
    sampler = RssSampler(args.server_pid)
    sampler.start()
    try:
        tools = evaluate_tools(client, sibling("tool_cases").cases())
        coding = evaluate_coding(client)
    finally:
        sampler.stop()
    result = build_result(args, client, digest, version, show, from_lines, tools, coding, sampler)
    write_atomic(args.output, result)
    return result


def summary_lines(result):
    tools, coding = result["tools"], result["coding"]
    return [
        f"tests run: {tools['goals']} tool goals in {tools['sessions']} sessions, "
        f"{coding['total']} coding tasks, {result['tests_run']['chat_requests']} chat requests",
        f"tools: passed {tools['passed']}/{tools['goals']}, schema_valid_calls {tools['schema_valid_calls']}, "
        f"invalid_calls {tools['invalid_calls']}, attempts {tools['attempts']}",
        f"coding: passed {coding['passed']}/{coding['total']}",
        f"rates: {result['rates']['median_tokens_per_second']} tokens/s median, "
        f"prompt {result['rates']['median_prompt_tokens_per_second']}",
        f"peak_rss_mb: {result['peak_rss_mb']}",
        f"full_baseline: {result['full_baseline']}",
        *(f"limitation: {note}" for note in result["limitations"]),
    ]


def main(argv=None):
    args = parse_args(argv)
    try:
        result = run_baseline(args)
    except Exception as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("\n".join(summary_lines(result)))
    print(f"output: {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
