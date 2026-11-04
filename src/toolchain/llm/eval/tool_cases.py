"""200 deterministic tool intents in ten 20-turn histories [LLM-BASELINE]."""
import json


def obj(properties):
    return {"type": "object", "properties": properties, "required": list(properties), "additionalProperties": False}


TEXT = {"type": "string"}
INTEGER = {"type": "integer"}
BOOLEAN = {"type": "boolean"}
MODE = {"type": "string", "enum": ["check", "apply", "preview"]}
FILE = obj({"path": TEXT, "content": TEXT})
TOOLS = [
    {"type": "function", "function": {"name": "write_file", "description": "Write exactly the supplied UTF-8 content to a path.", "parameters": FILE}},
    {"type": "function", "function": {"name": "run_tests", "description": "Run the named suites with an explicit policy.", "parameters": obj({"suites": {"type": "array", "items": TEXT}, "policy": obj({"mode": MODE, "retries": INTEGER, "fail_fast": BOOLEAN, "threshold": {"type": "number"}})})}},
    {"type": "function", "function": {"name": "apply_changes", "description": "Apply a batch of file writes with review metadata.", "parameters": obj({"files": {"type": "array", "items": FILE}, "review": obj({"approved": BOOLEAN, "mode": MODE, "note": TEXT})})}},
    {"type": "function", "function": {"name": "find_symbol", "description": "Find a symbol with case and result limit options.", "parameters": obj({"query": TEXT, "options": obj({"case_sensitive": BOOLEAN, "limit": INTEGER})})}},
    {"type": "function", "function": {"name": "report_problem", "description": "Record an error including structured recovery details.", "parameters": obj({"message": TEXT, "recovery": obj({"retry": BOOLEAN, "attempt": INTEGER, "mode": MODE, "context": FILE})})}},
]


def arguments(index):
    path = f"src/épreuve-{index}.py"
    content = f'def answer():\n    return "λ → {index}\\n"\n# 日本語 {{quoted}}\n'
    mode = ["check", "apply", "preview"][index % 3]
    choices = [
        ("write_file", {"path": path, "content": content}),
        ("run_tests", {"suites": ["gc", "arc"] if index % 2 else [], "policy": {"mode": mode, "retries": index % 4, "fail_fast": bool(index % 2), "threshold": (index % 10) / 10}}),
        ("apply_changes", {"files": [{"path": path, "content": content}, {"path": "empty.txt", "content": ""}], "review": {"approved": bool(index % 2), "mode": mode, "note": 'Keep "quotes" and\nnewlines.'}}),
        ("find_symbol", {"query": "Δelta_" + str(index), "options": {"case_sensitive": bool(index % 2), "limit": index % 7}}),
        ("report_problem", {"message": f"Attempt {index}: parser rejected <tool_call>", "recovery": {"retry": bool(index % 2), "attempt": index, "mode": mode, "context": {"path": path, "content": content}}}),
    ]
    return choices[index % len(choices)]


def cases():
    return [make_case(index) for index in range(200)]


def make_case(index):
    name, value = arguments(index - 1 if index % 20 == 8 else index)
    intent = f"Use {name} for this request. Preserve all types, Unicode and newlines; supply exactly these arguments: "
    if index % 20 == 8:
        intent = "The previous invocation failed. Retry the same request with its original values. " + intent
    return {"id": f"tool-{index:03d}", "session": index // 20, "name": name, "arguments": value, "prompt": intent + json.dumps(value, ensure_ascii=False), "inject_error": index % 20 == 7}
