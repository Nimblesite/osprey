# osprey: flavor=python
"""The Qwen server: one loopback listener on 11435 speaking Ollama and OpenAI.

Two lines on stdin choose the port and the model: the port (empty for the
default) and a GGUF path (empty for the fixture table `TableModel.py`, the CI
stand-in):

    printf '11435\\n%s\\n' ~/.cache/osprey-llm/gguf/Qwen3.8-27B-UD-Q4_K_XL.gguf | OSPREY_GPU=metal target/llms/qwen-server/server-arc

Every harness that speaks either protocol points at it instead of at a real
Ollama, which cannot host this port: `ollama create` runs a GGUF on Ollama's own
engines, so no Osprey code would execute ([LLM-SERVING]). Inference sits behind
the `Generator` effect: a GGUF path installs `RealModel.Real` over the mapped
weights, and a file that is missing or invalid ends startup with its error
instead of serving anything; no route knows which handler answers it.
"""
from qwen_server import Numeric
from qwen_server import RealModel
from qwen_server import Routes
from qwen_server import TableModel

DEFAULT_PORT = 11435
LOOPBACK = "127.0.0.1"
LIFETIME = 86400000


def main():
    """Serve until the lifetime ends, then stop the listener and exit cleanly."""
    port = chosen_port()
    weights = input()
    print(f"qwen server on {LOOPBACK}:{port}")
    match httpCreateServer(port, LOOPBACK):
        case Success(value=server):
            return serve(server, weights)
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


def serve(server, weights):
    """Serve with the model handler and the numeric policy installed around the loop."""
    with Numeric.halting:
        with Numeric.arithmetic:
            return served(server, weights)


def served(server, weights):
    """The fixture table, or the real weights at `weights`; a model that cannot load ends startup."""
    if weights == "":
        match TableModel.installed():
            case Success(value=model):
                with TableModel.Table:
                    return listen(server, model, "fixture table")
            case Error(message=message):
                return refused(message)
    else:
        match RealModel.open(weights):
            case Success(value=session):
                with RealModel.Real:
                    return listen(server, session, weights)
            case Error(message=message):
                return refused(message)


def refused(message):
    print(f"model error: {message}")
    return 1


def listen(server, model, served_from):
    """Start the listener over `model` and hold the fiber the runtime's loop runs on."""
    print(f"serving {served_from}")
    match httpListen(server, lambda method, url, _headers, body: Routes.dispatch(method, url, body, model)):
        case Success():
            sleep(LIFETIME)
            return stopped(server)
        case Error(message=message):
            print(f"listen error: {message}")
            return 1


def stopped(server):
    """Stop the listener and report how it ended."""
    match httpStopServer(server):
        case Success():
            print("stopped")
            return 0
        case Error(message=message):
            print(f"stop error: {message}")
            return 1
