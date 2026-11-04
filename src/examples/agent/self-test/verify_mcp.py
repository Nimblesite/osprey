"""Asserts what the doubled model was sent in the MCP self-test: each server's real answers, and tools that come and go."""
import json
import sys


def main():
    lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
    bodies = [json.loads(json.loads(line)["body"]) for line in lines]
    if len(bodies) != 14:
        raise SystemExit(f"FAILED: expected 14 model requests, got {len(bodies)}")

    def offered(index):
        return {tool["function"]["name"] for tool in bodies[index].get("tools", [])}

    def last_tool_text(index):
        return [message["content"] for message in bodies[index]["messages"] if message.get("role") == "tool"][-1]

    def check(index, wanted, reason):
        if wanted not in last_tool_text(index):
            raise SystemExit(f"FAILED: request {index + 1}, {reason}: {last_tool_text(index)!r}")

    def offers(index, name, expected):
        if (name in offered(index)) != expected:
            raise SystemExit(f"FAILED: request {index + 1} {'lacks' if expected else 'offers'} {name}: {sorted(offered(index))}")

    offers(0, "demo_secret_word", True)
    offers(0, "demo_add", True)
    offers(0, "demo_die", True)
    offers(0, "local_secret_word", False)
    check(1, "PROCESS-ABSENT", "a project server ran before it was turned on")
    check(3, "zephyr-word", "the stdio server's answer did not reach the model")
    check(4, "5", "add did not answer")
    check(5, "this tool always fails", "isError did not reach the model as the tool's text")
    check(7, "exited before answering", "a dead server's call did not fail in words")
    offers(8, "web_secret_word", True)
    offers(8, "sse_secret_word", True)
    offers(8, "local_secret_word", True)
    check(9, "web-word", "the HTTP server's answer did not reach the model")
    check(10, "sse-word", "the SSE server's answer did not reach the model")
    check(11, "PROCESS-RUNNING", "the project server did not run once turned on")
    offers(12, "demo_secret_word", False)
    offers(12, "web_secret_word", True)
    check(13, "PROCESS-ABSENT", "the removed server's process is still running")
    print("verify_mcp: 14 requests carry the servers' real answers")


main()
