#!/usr/bin/env python3
"""Checks the doubled provider requests of self-test/settings.json: each
request's URL and the exact body members its provider's API takes, per the
catalogue in src/osprey/examples/agent/src/engine.py. A request that went to the wrong
endpoint, or whose body carries a member the provider rejects, fails the run.
"""
import json
import sys

GLM_URL = "https://api.z.ai/api/paas/v4/chat/completions"
KIMI_URL = "https://api.kimi.com/coding/v1/chat/completions"

# The Kimi endpoint takes no thinking object and no strict tool definitions.
KIMI_EXPECTATION = {
    "url": KIMI_URL,
    "model": "k3",
    "reasoning_effort": "max",
    "tool_choice": "auto",
    "stream": False,
    "strict_tools": False,
    "no_thinking": True,
}

EXPECTATIONS = [
    {
        "url": GLM_URL,
        "model": "glm-5.3",
        "reasoning_effort": "high",
        "thinking": {"type": "enabled", "clear_thinking": False},
        "tool_choice": "auto",
        "stream": False,
        "strict_tools": True,
    },
    KIMI_EXPECTATION,
    KIMI_EXPECTATION,
]


def read_requests(path):
    requests = []
    for number, line in enumerate(open(path, encoding="utf-8"), start=1):
        if line.strip():
            record = json.loads(line)
            requests.append((number, record["url"], json.loads(record["body"])))
    return requests


def check(number, url, body, wanted):
    problems = []
    if url != wanted["url"]:
        problems.append(f"request {number} went to {url}, want {wanted['url']}")
    for member in ("model", "reasoning_effort", "tool_choice", "stream", "thinking"):
        if member in wanted and body.get(member) != wanted[member]:
            problems.append(f"request {number} member {member} is {body.get(member)!r}, want {wanted[member]!r}")
    raw = json.dumps(body, separators=(",", ":"))
    if ('"strict":true' in raw) != wanted["strict_tools"]:
        problems.append(f"request {number} strict tool definitions: {wanted['strict_tools']}")
    if wanted.get("no_thinking") and "thinking" in body:
        problems.append(f"request {number} carries a thinking object, which the Kimi endpoint rejects")
    return problems


def main():
    requests = read_requests(sys.argv[1])
    if len(requests) != len(EXPECTATIONS):
        print(f"FAILED: {len(requests)} provider requests recorded, want {len(EXPECTATIONS)}")
        return 1
    problems = [problem for (number, url, body), wanted in zip(requests, EXPECTATIONS) for problem in check(number, url, body, wanted)]
    for problem in problems:
        print(f"FAILED: {problem}")
    if problems:
        return 1
    print(f"provider requests verified: {len(requests)} requests, URLs and bodies as the catalogue says")
    return 0


if __name__ == "__main__":
    sys.exit(main())
