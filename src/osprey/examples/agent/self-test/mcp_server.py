"""A real MCP server for the agent's proofs, on stdio or streamable HTTP. Python standard library only.

    mcp_server.py [--pid-file PATH]                                  stdio, one JSON message per line
    mcp_server.py --http [--port N] [--sse] [--token T] [--pid-file PATH]    prints `listening PORT`

Tools come back over two `tools/list` pages. `secret_word` answers MCP_TEST_WORD from the environment, so a
proof shows both that `env` arrived and that the answer reached the model. Over stdio the server sends one
`ping` of its own after the first `tools/list`, which a client has to answer; over HTTP `--token` turns away
a request without the bearer token, which shows that `headers` arrived. `--sse` answers as an event stream.
"""
import argparse
import http.server
import json
import os
import sys
import time

PROTOCOL_VERSION = "2025-06-18"
WORD_NAME = "MCP_TEST_WORD"
SESSION = "osprey-test-session"
PING_ID = "server-ping-1"
SLOW_SECONDS = 30


def text_result(text, is_error=False):
    return {"content": [{"type": "text", "text": text}], "isError": is_error}


def secret_word(_arguments):
    return text_result(os.environ.get(WORD_NAME, f"{WORD_NAME} is not set"))


def add(arguments):
    return text_result(str(arguments["a"] + arguments["b"]))


def fails(_arguments):
    return text_result("this tool always fails", is_error=True)


def slow(_arguments):
    time.sleep(SLOW_SECONDS)
    return text_result("finally")


def die(_arguments):
    os._exit(3)


def schema(**properties):
    return {"type": "object", "properties": {name: {"type": kind} for name, kind in properties.items()}, "required": list(properties)}


TOOLS = {
    "secret_word": (secret_word, "The word only this server knows.", schema()),
    "add": (add, "Adds a and b.", schema(a="number", b="number")),
    "fails": (fails, "Always answers with a tool error.", schema()),
    "slow": (slow, f"Answers after {SLOW_SECONDS} seconds.", schema()),
    "die": (die, "Ends the server without answering.", schema()),
}
PAGES = [["secret_word", "add"], ["fails", "slow", "die"]]


def listed(name):
    _, description, input_schema = TOOLS[name]
    return {"name": name, "description": description, "inputSchema": input_schema}


def tools_page(params):
    index = int(params.get("cursor", "0"))
    page = {"tools": [listed(name) for name in PAGES[index]]}
    return {**page, "nextCursor": str(index + 1)} if index + 1 < len(PAGES) else page


def call_tool(params):
    name = params.get("name")
    if name not in TOOLS:
        raise LookupError(f"unknown tool {name}")
    return TOOLS[name][0](params.get("arguments", {}))


def initialize(_params):
    return {
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {"tools": {"listChanged": False}},
        "serverInfo": {"name": "osprey-test-server", "version": "1"},
    }


METHODS = {"initialize": initialize, "ping": lambda _params: {}, "tools/list": tools_page, "tools/call": call_tool}


def answer(message):
    """The response to one message, or None for a notification or a response to our own ping."""
    if "method" not in message or "id" not in message:
        return None
    frame = {"jsonrpc": "2.0", "id": message["id"]}
    method = METHODS.get(message["method"])
    if method is None:
        return {**frame, "error": {"code": -32601, "message": f"unknown method {message['method']}"}}
    try:
        return {**frame, "result": method(message.get("params", {}))}
    except (LookupError, TypeError, ValueError) as problem:
        return {**frame, "error": {"code": -32602, "message": str(problem)}}


def write_line(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def serve_stdio():
    pinged = False
    for line in sys.stdin:
        if not line.strip():
            continue
        message = json.loads(line)
        response = answer(message)
        if response is not None:
            write_line(response)
        if message.get("method") == "tools/list" and not pinged:
            pinged = True
            write_line({"jsonrpc": "2.0", "id": PING_ID, "method": "ping"})


def http_handler(sse, token):
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def reply(self, status, content_type, body):
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Mcp-Session-Id", SESSION)
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            self.reply(405, "text/plain", b"no server stream")

        def do_DELETE(self):
            self.reply(200, "text/plain", b"")

        def do_POST(self):
            if token and self.headers.get("Authorization") != f"Bearer {token}":
                self.reply(401, "text/plain", b"missing or wrong token")
                return
            message = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
            known = message.get("method") == "initialize" or self.headers.get("Mcp-Session-Id") == SESSION
            if not known:
                self.reply(400, "text/plain", b"missing session id")
                return
            response = answer(message)
            if response is None:
                self.reply(202, "text/plain", b"")
            elif sse:
                self.reply(200, "text/event-stream", f"event: message\ndata: {json.dumps(response)}\n\n".encode())
            else:
                self.reply(200, "application/json", json.dumps(response).encode())

    return Handler


def serve_http(port, sse, token):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), http_handler(sse, token))
    print(f"listening {server.server_port}", flush=True)
    server.serve_forever()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--http", action="store_true")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--sse", action="store_true")
    parser.add_argument("--token", default="")
    parser.add_argument("--pid-file", default="")
    options = parser.parse_args()
    if options.pid_file:
        with open(options.pid_file, "w") as file:
            file.write(str(os.getpid()))
    if options.http:
        serve_http(options.port, options.sse, options.token)
    else:
        serve_stdio()


main()
