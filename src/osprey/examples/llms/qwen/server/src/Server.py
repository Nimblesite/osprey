# osprey: flavor=python
"""The Qwen server: one loopback listener on 11435 speaking Ollama and OpenAI.

Run it with a port on stdin, or with stdin closed for the default:

    printf '11435\\n' | target/llms/qwen-server/server-gc

Every harness that speaks either protocol points at it instead of at a real
Ollama, which cannot host this port: `ollama create` runs a GGUF on Ollama's own
engines, so no Osprey code would execute ([LLM-SERVING]). Inference sits behind
the `Generator` effect; the handler installed here is the fixture model in
`TableModel.py`, and plan 0049 Phase 3 replaces it with the real weights without
touching a route.
"""
from qwen_server import Numeric
from qwen_server import Routes
from qwen_server import TableModel

DEFAULT_PORT = 11435
LOOPBACK = "127.0.0.1"
LIFETIME = 86400000


def main():
    """Serve until the lifetime ends, then stop the listener and exit cleanly."""
    port = chosen_port()
    print(f"qwen server on {LOOPBACK}:{port}")
    match httpCreateServer(port, LOOPBACK):
        case Success(value=server):
            return serve(server)
        case Error(message=message):
            print(f"create error: {message}")
            return 1


def chosen_port():
    """The port on the first stdin line, or the default when there is none."""
    match parseInt(input()):
        case Success(value=port):
            return port if port > 0 else DEFAULT_PORT
        case Error():
            return DEFAULT_PORT


def serve(server):
    """Serve with the model handler and the numeric policy installed around the loop."""
    with Numeric.halting:
        with Numeric.arithmetic:
            with TableModel.Table:
                return listen(server)


def listen(server):
    """Start the listener and hold the fiber the runtime's loop runs on."""
    match httpListen(server, handler):
        case Success():
            sleep(LIFETIME)
            return stopped(server)
        case Error(message=message):
            print(f"listen error: {message}")
            return 1


def handler(method, url, headers, body):
    """Answer one request by routing it under the installed model handler."""
    return Routes.dispatch(method, url, body)


def stopped(server):
    """Stop the listener and report how it ended."""
    match httpStopServer(server):
        case Success():
            print("stopped")
            return 0
        case Error(message=message):
            print(f"stop error: {message}")
            return 1
