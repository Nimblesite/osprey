#!/usr/bin/env python3
"""CPython oracle for the `@osprey/transformers` tool-call layer ([LLM-TOOL-PARSE]).

Writes packages/transformers/tests/expected_tools.py through the shared
`scripts/oracles/expected_module.py` writer. Everything here comes from the real
Qwen artifacts and the real `jsonschema` package:

* the wire text of every tool call is rendered by the *unmodified* chat template
  `scripts/oracles/qwen3_5_chat_template.jinja`, the file published at
  Qwen/Qwen3.8-27B revision 1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0 (its
  sha256 is pinned below). The `tokenizer.chat_template` metadata inside the
  pinned weights file unsloth/Qwen3.8-27B-GGUF revision
  4ca720788d1e01f1bff70c033e0d0028fd02e502 renders identical `<think>`,
  `<tool_call>`, `<function=` and `<parameter=` bytes; only the system-message
  merging differs, which no parse case exercises;
* every validity verdict is `jsonschema` 4.26.0's
  (`Draft202012Validator.iter_errors`), recorded with the failing keyword;
* every coercion expectation is the argument the caller passed, the value the
  rendered wire text carries for it, and the canonical JSON of that value.

The 200 cases are `src/osprey/examples/llms/qwen/eval/tool_cases.py`, the Phase 0
corpus.

Run from the repository root with the oracle virtual environment:

    python -I scripts/oracles/oracle.py tool_calls
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import re
from pathlib import Path

import jsonschema
from jinja2 import Environment

from expected_module import write

REPO = Path(__file__).resolve().parents[2]
TEMPLATE = Path(__file__).resolve().parent / "qwen3_5_chat_template.jinja"
TEMPLATE_SHA256 = "c3cf9e34abf4f9e36c2d72165aa9c132d3e2a725b6c2586aaa3a8af9d7a81041"
TEMPLATE_REVISION = "Qwen/Qwen3.8-27B@1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0"

# The `<tool_call>` block tokens plus the format's own keywords. Longest first so a
# greedy tokenizer prefers the widest fragment.
FORMAT_TOKENS = [
    "\n</parameter>\n",
    "</parameter>\n",
    "</function>\n",
    "<parameter=",
    "<function=",
    "<tool_call>",
    "</tool_call>",
    "</parameter>",
    "</function>",
    "</tool_call>",
    "</think>\n\n",
    "<think>\n",
    "</think>",
    "true",
    "false",
    "null",
]
# The stages whose next byte set is closed: a tag, a name, a parameter name or an enum
# member. A token that is a proper prefix of one of these would strand the grammar
# (there is no token left to finish it), so the fixture omits it.
CONFINED = FORMAT_TOKENS + [
    "write_file",
    "run_tests",
    "apply_changes",
    "find_symbol",
    "report_problem",
    "check",
    "apply",
    "preview",
    "true",
    "false",
]
# Tokens a well-formed call never contains. The constrained sampler's adversarial
# scores prefer them, so the mask has to forbid them.
ADVERSARIAL = [
    "delete_file",
    "write_filesz",
    "write",
    "run_test",
    "apply_changez",
    "find_symbols",
    "report",
    "<function=delete_file>\n",
    "<parameter=paths>\n",
    "<parameter=name>\n",
    "<parameter=force>\n",
    "<parameter=path>\n<parameter=path>\n",
    "<tool_call>",
    "<think>",
    "<tool_call",
    "<parameter=",
    "</function>",
    "</tool_call>",
    "</parameter>",
    "true_",
    "maybe",
    "nul",
    "TRUE",
    "delete",
    '"apply"',
    "007",
    "1.2.3",
    "-",
    "e3",
    "{}",
    "[]",
    '"',
    "\\",
    "é",
    "\t",
    "  ",
    "\n",
    " ",
    "{",
    "}",
    "[",
    "]",
    ",",
    ":",
]

CONTENT = ["", "Let me check.", 'I will read "x" then write.', "\n\nReady."]


def template():
    """The real chat template, hash-checked against the pinned revision."""
    source = TEMPLATE.read_text()
    digest = hashlib.sha256(source.encode()).hexdigest()
    if digest != TEMPLATE_SHA256:
        raise SystemExit(f"{TEMPLATE} is not the pinned template: {digest}")
    return source


def environment():
    """Jinja2 with the two hooks the template needs: transformers' own `tojson`
    (`json.dumps(value, ensure_ascii=False)`, no key sorting) and
    `raise_exception`, as `transformers.utils.chat_template_utils` installs them."""
    env = Environment(keep_trailing_newline=True)
    env.filters["items"] = lambda value: list(value.items()) if isinstance(value, dict) else []
    env.filters["tojson"] = lambda value, **kw: json.dumps(value, ensure_ascii=False, **kw)
    env.globals["raise_exception"] = lambda message: (_ for _ in ()).throw(ValueError(message))
    return env


def cases_module():
    path = REPO / "src/osprey/examples/llms/qwen/eval/tool_cases.py"
    spec = importlib.util.spec_from_file_location("tool_cases_oracle", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def assistant_turn(env, tools, case, content):
    """The model output for the case's turn: the real template's assistant block."""
    reasoning = f"Plan {case['id']}: call {case['name']}."
    message = {
        "role": "assistant",
        "content": content,
        "reasoning_content": reasoning,
        "tool_calls": [{"type": "function", "function": {"name": case["name"], "arguments": case["arguments"]}}],
    }
    rendered = env.from_string(template_globals["source"]).render(
        messages=[{"role": "user", "content": case["prompt"]}, message], tools=tools, add_generation_prompt=False
    )
    turn = rendered.split("<|im_start|>assistant\n", 1)[1]
    if not turn.endswith("<|im_end|>\n"):
        raise SystemExit("assistant turn does not end with the end-of-turn token")
    return turn[: -len("<|im_end|>\n")], reasoning


def split_turn(turn):
    """The `<think>` content, the free text and the tool-call block of one turn.

    Both spans are the turn's own bytes: `think` is everything between `<think>` and
    `</think>`, and `text` is everything between `</think>` and `<tool_call>`, so the
    parser's contract is a byte-faithful split of what the template wrote and the
    harness trims what it wants to show."""
    head, _, rest = turn.partition("<think>")
    think, _, body = rest.partition("</think>")
    if head or not rest or not body:
        raise SystemExit(f"unexpected turn shape: {turn!r}")
    text, _, block = body.partition("<tool_call>")
    return think, text, block


def tokenizer(text, table):
    """Greedy longest-match over `table`, then single characters."""
    wide = sorted({entry for entry in table if entry}, key=len, reverse=True)
    out, index = [], 0
    while index < len(text):
        for entry in wide:
            if text.startswith(entry, index):
                out.append(entry)
                index += len(entry)
                break
        else:
            out.append(text[index])
            index += 1
    return out


def unstranded(candidates):
    """`candidates`, closed under the remainders of the closed-set literals.

    A tag, name, parameter name or enum member accepts only continuations of itself, so a
    token that starts one must leave the rest spellable; the fixture adds the missing
    remainder as its own token instead of dropping the prefix, because the prefix is a
    legal choice that the mask is right to allow."""
    keep = list(dict.fromkeys(candidates))
    for _ in range(4):
        added = [rest for rest in remainders(keep) if rest not in keep]
        if not added:
            break
        keep += added
    return keep


def remainders(keep):
    """Every unspellable remainder of a closed-set literal that `keep` already starts."""
    return [
        literal[len(token):]
        for literal in CONFINED
        for token in keep
        if token and literal != token and literal.startswith(token) and not spellable(literal[len(token):], keep)
    ]


def spellable(text, keep):
    """Whether `keep` spells `text` exactly, by the same greedy rule the fixture uses."""
    wide = sorted({entry for entry in keep if entry}, key=len, reverse=True)
    index = 0
    while index < len(text):
        for entry in wide:
            if text.startswith(entry, index):
                index += len(entry)
                break
        else:
            return False
    return True


def vocabulary(case, wire, tools):
    """The per-case token table: the corpus's own tokens plus adversarial ones.

    A call is spellable only if every character it needs is in the table, so the
    table carries the case's literals (tool and parameter names, enum values, the
    words of its string arguments) and then single characters."""
    names = [tool["function"]["name"] for tool in tools]
    params = sorted({name for tool in tools for name in tool["function"]["parameters"]["properties"]})
    words = [word for word in re.split(r"[\s\"{}]+", wire) if 1 <= len(word) <= 24]
    literals = ["check", "apply", "preview", "true", "false", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    spelled = names + params + literals
    table = FORMAT_TOKENS + spelled + words
    # Digits, punctuation and whitespace as single tokens; letters only inside whole words,
    # names and members, so the seeded source cannot strand the grammar inside a name it
    # has no continuation for.
    table += sorted({char for text in [wire] + spelled for char in text if char.isdigit() or not char.isalnum()})
    tokens = unstranded(list(dict.fromkeys(table + ADVERSARIAL)))
    stray = [token for token in tokenizer(wire, table) if token not in tokens]
    if stray:
        raise SystemExit(f"the vocabulary cannot spell the wire: {stray}")
    return tokens


def prefix_and_want(wire, table, tokens):
    """The turn split where the constraint starts, as token-id lists."""
    opening = wire.index("<tool_call>") + len("<tool_call>")
    prefix_ids = [tokens.index(token) for token in tokenizer(wire[:opening], table)]
    want_ids = [tokens.index(token) for token in tokenizer(wire[opening:], table)]
    spelled = "".join(tokens[index] for index in prefix_ids + want_ids)
    if spelled != wire:
        raise SystemExit(f"fixture tokenization does not reproduce the wire text: {spelled!r}")
    return prefix_ids, want_ids


def schema_of(tools, name):
    for tool in tools:
        if tool["function"]["name"] == name:
            return tool["function"]["parameters"]
    raise SystemExit(f"no tool named {name}")


def first_error(schema, value):
    """jsonschema's first error in its own traversal order: keyword and instance path.

    Draft 2020-12 visits a schema's keywords in the order the schema spells them, and
    descends into `properties` in property order, so the port checks `type`, then
    `enum`, then `items`, then `properties` (in order), then `required`, then
    `additionalProperties`, and the two agree."""
    for error in jsonschema.Draft202012Validator(schema).iter_errors(value):
        return error.validator, "/".join(str(part) for part in error.absolute_path)
    return None


def parameter_texts(wire, block):
    """Each `<parameter=NAME>` value exactly as the template wrote it."""
    pattern = re.compile(r"<parameter=(.*?)>\n(.*?)\n</parameter>\n", re.DOTALL)
    return pattern.findall(block)


def canonical(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def case_records(case_module, env, tools):
    """One fixture record per Phase 0 case."""
    records = []
    for index, case in enumerate(case_module.cases()):
        wire, reasoning = assistant_turn(env, tools, case, CONTENT[index % len(CONTENT)])
        think, text, block = split_turn(wire)
        name = case["name"]
        if think.strip() != reasoning:
            raise SystemExit(f"case {case['id']}: think {think!r} is not the reasoning")
        schema = schema_of(tools, name)
        verdict = first_error(schema, case["arguments"])
        if verdict is not None:
            raise SystemExit(f"case {case['id']}: corpus arguments are not schema-valid: {verdict}")
        table = vocabulary(case, wire, tools)
        prefix, want = prefix_and_want(wire, table, table)
        paired = parameter_texts(wire, block)
        if [pair[0] for pair in paired] != list(case["arguments"]):
            raise SystemExit(f"case {case['id']}: parameter order differs from the arguments")
        coercions = [
            {"tool": name, "param": param, "schema": canonical(schema["properties"][param]),
             "text": value, "want": canonical(case["arguments"][param]), "ok": True,
             "keyword": "", "path": ""}
            for param, value in paired
        ]
        records.append(
            {
                "id": case["id"],
                "wire": wire,
                "think": think,
                "text": text,
                "name": name,
                "args": canonical(case["arguments"]),
                "tokens": table,
                "prefix": prefix,
                "want": want,
                "coercions": coercions,
            }
        )
    return records


def literal(text):
    """`text` as one JSON literal, or None when it is not one."""
    try:
        return json.loads(text), True
    except (ValueError, TypeError):
        return None, False


def coerced(schema, text):
    """Coerce one parameter text by its declared type, then ask jsonschema.

    A string parameter keeps the template's own text verbatim (the template writes a
    string argument unquoted); every other type reads a JSON literal, as the
    template's `tojson` wrote it. `integer` also takes an integral JSON number, the
    `integer` takes an integer literal only. The verdict for the value itself is
    `jsonschema`'s: the coerced value must satisfy the parameter's schema, so an enum
    outside the declared members or an undeclared nested property is a rejection and
    not a string."""
    kind = schema.get("type")
    if kind != "string":
        parsed, ok = literal(text)
        if not ok:
            return rejected("type", "")
        value = parsed
        if not typed(kind, value):
            return rejected("type", "")
        if first_error(schema, value) is not None:
            keyword, path = first_error(schema, value)
            return rejected(keyword, path)
    else:
        value = text
        if first_error(schema, value) is not None:
            keyword, path = first_error(schema, value)
            return rejected(keyword, path)
    return {"ok": True, "want": canonical(value), "keyword": "", "path": ""}


def typed(kind, value):
    """Whether a parsed literal is an instance of `kind` under the wire's literal rule.

    A JSON literal has one form per type: `integer` takes an integer literal and not
    the integral float `1e3` (jsonschema's integer type would accept it, but the
    template writes an integer argument as `3`, never `3.0`), and `boolean` takes
    `true`/`false` and not `0`."""
    if isinstance(value, bool):
        return kind == "boolean"
    if kind == "integer":
        return isinstance(value, int)
    if kind == "number":
        return isinstance(value, (int, float))
    if kind == "array":
        return isinstance(value, list)
    if kind == "object":
        return isinstance(value, dict)
    return False


def rejected(keyword, path):
    return {"ok": False, "want": "", "keyword": keyword, "path": path}


def coercion_records(records, tools):
    """The template's own parameter texts, plus the boundary texts a model emits."""
    out = []
    for record in records[:BUILTIN_CASE_COUNT]:
        out.extend(record["coercions"])
    write_file = schema_of(tools, "write_file")["properties"]
    run_tests = schema_of(tools, "run_tests")["properties"]
    policy = run_tests["policy"]
    edges = [
        ("run_tests", "suites", "[]", run_tests["suites"]),
        ("run_tests", "suites", '["gc"]', run_tests["suites"]),
        ("run_tests", "suites", '["a b", "c\\nd"]', run_tests["suites"]),
        ("run_tests", "suites", '"gc"', run_tests["suites"]),
        ("run_tests", "suites", "[]]", run_tests["suites"]),
        ("run_tests", "policy", policy_text(edgy_policy()), policy),
        ("run_tests", "policy", "{}", policy),
        ("run_tests", "policy", "[]", policy),
        ("run_tests", "policy", "not json", policy),
        ("run_tests", "policy", '{"mode": "delete", "retries": 1, "fail_fast": true, "threshold": 0.5}', policy),
        ("run_tests", "policy", '{"mode": "check", "retries": 1.5, "fail_fast": true, "threshold": 0.5}', policy),
        ("write_file", "path", "", write_file["path"]),
        ("write_file", "content", "line one\nline two\n", write_file["content"]),
        ("write_file", "content", "  padded  ", write_file["content"]),
        ("write_file", "content", "état λ 日本語", write_file["content"]),
        ("write_file", "content", 'quote " and \\ backslash', write_file["content"]),
        ("write_file", "content", "007", write_file["content"]),
        ("write_file", "content", "true", write_file["content"]),
        ("write_file", "content", "<tool_call>", write_file["content"]),
    ]
    out.extend(coercion_edge(*edge) for edge in edges)
    typed = [
        ("edge-integer", "integer", "0"),
        ("edge-integer", "integer", "-3"),
        ("edge-integer", "integer", "9007199254740993"),
        ("edge-integer", "integer", "1e3"),
        ("edge-integer", "integer", "3.0"),
        ("edge-integer", "integer", "3.5"),
        ("edge-integer", "integer", ""),
        ("edge-integer", "integer", "0x10"),
        ("edge-integer", "integer", "007"),
        ("edge-number", "number", "0.5"),
        ("edge-number", "number", "-2"),
        ("edge-number", "number", "1e-3"),
        ("edge-number", "number", "2.0"),
        ("edge-number", "number", "true"),
        ("edge-number", "number", "1.2.3"),
        ("edge-boolean", "boolean", "true"),
        ("edge-boolean", "boolean", "false"),
        ("edge-boolean", "boolean", "True"),
        ("edge-boolean", "boolean", "yes"),
        ("edge-boolean", "boolean", "0"),
        ("edge-boolean", "boolean", ""),
        ("edge-array", "array", "[]"),
        ("edge-array", "array", "[1, 2]"),
        ("edge-array", "array", "[\"a\", [\"b\"]]"),
        ("edge-array", "array", "[1,]"),
        ("edge-array", "array", "{}"),
        ("edge-object", "object", "{}"),
        ("edge-object", "object", '{"a": 1}'),
        ("edge-object", "object", '{"a": [true, null]}'),
        ("edge-object", "object", "{'a': 1}"),
        ("edge-object", "object", "[1]"),
    ]
    out.extend(coercion_edge(tool, "value", text, {"type": kind}) for tool, kind, text in typed)
    return out


def edgy_policy():
    return {"mode": "preview", "retries": 0, "fail_fast": False, "threshold": 1.5, "note": "x"}


def policy_text(value):
    return json.dumps(value, ensure_ascii=False)


def coercion_edge(tool, param, text, schema):
    """One coercion case: the tool and parameter whose schema decides, and the verdict."""
    verdict = coerced(schema, text)
    return {"tool": tool, "param": param, "schema": canonical(schema), "text": text,
            "want": verdict["want"], "ok": verdict["ok"],
            "keyword": verdict["keyword"], "path": verdict["path"]}


def validation_records(tools):
    """Mutations of real arguments, each judged by jsonschema alone."""
    out = []
    for index, case in enumerate(cases_module().cases()[:VALIDATION_CASE_COUNT]):
        schema = schema_of(tools, case["name"])
        for value in mutations(case["arguments"]):
            verdict = first_error(schema, value)
            out.append(
                {
                    "tool": case["name"],
                    "value": canonical(value),
                    "ok": verdict is None,
                    "keyword": "" if verdict is None else verdict[0],
                    "path": "" if verdict is None else verdict[1],
                }
            )
    return out


def mutations(arguments):
    """The argument object, then the same object with one part broken."""
    out = [arguments]
    for key, value in arguments.items():
        out.append({**arguments, key: broken(value)})
        out.append({**arguments, key: [] if not isinstance(value, list) else "not a list"})
    out.append({**arguments, "unexpected": 1})
    out.append({key: value for key, value in list(arguments.items())[1:]})
    return out


def broken(value):
    """A value of the wrong JSON type for `value`'s slot."""
    if isinstance(value, bool):
        return "true"
    if isinstance(value, int):
        return "7"
    if isinstance(value, float):
        return "0.5"
    if isinstance(value, str):
        return 7
    if isinstance(value, list):
        return {}
    if isinstance(value, dict):
        return [1]
    return None


def repair_records(case_module, env, tools):
    """Calls the template would never write, each rejected by jsonschema."""
    base = case_module.cases()[0]
    write_file = schema_of(tools, "write_file")
    run_tests = schema_of(tools, "run_tests")
    known = "write_file"
    candidates = [
        ("deliver_file", [("path", "src/a.py"), ("content", "x")]),
        ("write_file", [("path", "src/a.py"), ("content", "x"), ("mode", "apply")]),
        ("write_file", [("path", "src/a.py")]),
        ("write_file", [("content", "x")]),
        ("run_tests", [("suites", '["gc"]'), ("policy", '{"mode": "delete", "retries": 1, "fail_fast": true, "threshold": 0.5}')]),
        ("run_tests", [("suites", '["gc"]'), ("policy", '{"mode": "check", "retries": "3", "fail_fast": true, "threshold": 0.5}')]),
        ("run_tests", [("suites", '["gc", "arc"'), ("policy", '{"mode": "check", "retries": 1, "fail_fast": true, "threshold": 0.5}')]),
        ("run_tests", [("suites", '"gc"'), ("policy", '{"mode": "check", "retries": 1, "fail_fast": true, "threshold": 0.5}')]),
        ("run_tests", [("suites", '[]'), ("policy", '{"mode": "check", "retries": 1, "fail_fast": true}')]),
        ("run_tests", [("suites", '[]'), ("policy", '{"mode": "check", "retries": 1, "fail_fast": "yes", "threshold": 0.5}')]),
        ("apply_changes", [("files", '[{"path": "a"}'), ("review", '{"approved": true, "mode": "apply", "note": ""}')]),
        ("find_symbol", [("query", "x"), ("options", '{"case_sensitive": true, "limit": 2.5}')]),
        ("report_problem", [("message", "boom"), ("recovery", '{"retry": true, "attempt": "many", "mode": "check", "context": {"path": "a", "content": "b"}}')]),
    ]
    out = []
    for name, pairs in candidates:
        wire = "<tool_call>\n<function=" + name + ">\n"
        wire += "".join(f"<parameter={param}>\n{text}\n</parameter>\n" for param, text in pairs)
        wire += "</function>\n</tool_call>"
        verdict = repair_verdict(tools, name, pairs)
        if verdict is None:
            raise SystemExit(f"repair case {name} resolved to a schema-valid call")
        out.append({"wire": wire, "name": name, "keyword": verdict, "id": f"repair-{len(out):02d}"})
    del base, write_file, run_tests, known
    return out


def repair_verdict(tools, name, pairs):
    """jsonschema's keyword for the repaired call's arguments, or None when valid."""
    try:
        schema = schema_of(tools, name)
    except SystemExit:
        return "unknown_function"
    parameters = schema["properties"]
    unknown = [param for param, _ in pairs if param not in parameters]
    if unknown:
        return "additionalProperties"
    arguments = {}
    for param, text in pairs:
        verdict = coerced(parameters[param], text)
        if not verdict["ok"]:
            return verdict["keyword"]
        arguments[param] = json.loads(verdict["want"])
    verdict = first_error(schema, arguments)
    return None if verdict is None else verdict[0]


BUILTIN_CASE_COUNT = 40
VALIDATION_CASE_COUNT = 40
template_globals = {}


def main():
    template_globals["source"] = template()
    env = environment()
    case_module = cases_module()
    tools = case_module.TOOLS
    records = case_records(case_module, env, tools)
    coercions = coercion_records(records, tools)
    validations = validation_records(tools)
    repairs = repair_records(case_module, env, tools)
    constants = {
        "TOOLS": json.dumps(tools, ensure_ascii=False),
        "CASES": [json.dumps(record, ensure_ascii=False) for record in records],
        "COERCE": [json.dumps(record, ensure_ascii=False) for record in coercions],
        "VALIDATE": [json.dumps(record, ensure_ascii=False) for record in validations],
        "REPAIR": [json.dumps(record, ensure_ascii=False) for record in repairs],
    }
    docs = {
        "TOOLS": "The five tool definitions of the Phase 0 corpus, as the request carries them.",
        "CASES": "One JSON record per Phase 0 case: the template's wire text, its `<think>`/text "
        "split, the call it encodes, and the constrained-sampling fixture for it.",
        "COERCE": "One JSON record per parameter text: the tool, the parameter, the text the "
        "template wrote, the canonical JSON of the coerced value, and whether coercion succeeds.",
        "VALIDATE": "One JSON record per argument mutation: the tool, the JSON value, jsonschema's "
        "verdict and the failing keyword with its instance path.",
        "REPAIR": "One JSON record per call the parser must reject: the wire text, the function "
        "name and jsonschema's keyword for the reason.",
    }
    versions = {"jsonschema": jsonschema_version(), "jinja2": jinja_version()}
    path = write(
        "transformers",
        f"Tool-call parsing values for @osprey/transformers, rendered by the chat template of {TEMPLATE_REVISION}",
        versions,
        constants,
        docs,
        module="ExpectedTools",
        namespace="transformers_tools_expected",
        file="expected_tools.py",
    )
    print(f"wrote {path}: {len(records)} cases, {len(coercions)} coercions, "
          f"{len(validations)} validations, {len(repairs)} repairs")


def jsonschema_version():
    import importlib.metadata

    return importlib.metadata.version("jsonschema")


def jinja_version():
    import importlib.metadata

    return importlib.metadata.version("jinja2")


if __name__ == "__main__":
    main()
