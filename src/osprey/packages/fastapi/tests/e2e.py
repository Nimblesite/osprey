"""Drive both managed backends in parallel over real HTTP, never a mocked transport."""
import concurrent.futures
from contextlib import closing
import http.client
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[5]


def request(port, method, path, body=None, headers=None):
    with closing(http.client.HTTPConnection("127.0.0.1", port, timeout=5)) as connection:
        connection.request(method, path, body=body, headers=headers or {})
        reply = connection.getresponse()
        return reply.status, dict(reply.getheaders()), reply.read()


def assert_redirect(port, path, target):
    status, headers, body = request(port, "GET", path)
    assert (status, body) == (307, b"")
    assert headers["Location"] == f"http://127.0.0.1:{port}{target}"


def health_cases(port):
    status, headers, body = request(port, "GET", "/health")
    assert (status, body) == (200, b'{"status":"ok"}'), (status, body)
    assert headers["Content-Type"] == "application/json", headers
    assert request(port, "GET", "/health?probe=true")[0] == 200
    assert_redirect(port, "/health/?probe=1", "/health?probe=1")
    assert request(port, "GET", "/missing")[::2] == (404, b'{"detail":"Not Found"}')
    status, headers, body = request(port, "GET", "/readyz")
    assert (status, body) == (503, b'{"detail":"database unavailable"}')
    assert headers["Cache-Control"] == "no-store"


def method_cases(port):
    for method in ("POST", "PUT", "DELETE", "HEAD", "OPTIONS"):
        status, headers, body = request(port, method, "/health")
        assert status == 405, (method, status)
        assert headers["Allow"] == "GET", headers
        if method != "HEAD":
            assert body == b'{"detail":"Method Not Allowed"}'


def body_cases(port):
    for payload in (b"", b"hello", "héllo ☕".encode(), b'{"x":[1,true,null]}'):
        status, headers, body = request(port, "POST", "/echo", payload)
        assert (status, body) == (201, payload), (status, body, payload)
        assert headers["Content-Type"] == "application/octet-stream"
        assert headers["X-Echo"] == "yes"


def parameter_cases(port):
    path = Path(__file__).with_name("parameters.expected.json")
    for case in json.loads(path.read_text()):
        status, headers, body = request(port, case["method"], case["path"], headers=case["headers"])
        actual = json.loads(body) if body and headers.get("Content-Type", "").startswith("application/json") else body.decode() if body else None
        assert body.decode() == case["body_text"], (case, status, body)
        assert (status, actual) == (case["status"], case["body"]), (case, status, actual)


def failure_cases(port):
    status, _, body = request(port, "GET", "/bundle/%00")
    assert (status, json.loads(body)) == (400, {"detail": "NUL is not representable in the native HTTP string boundary"})
    assert_redirect(port, "/bundle?tags=x", "/bundle/?tags=x")


def dependency_smoke(port):
    status, _, body = request(port, "GET", "/deps", headers={"Authorization": "Bearer good", "X-Role": "writer", "X-Credit": "1"})
    assert status == 200, (status, body)


def cases(port):
    for _ in range(3):
        health_cases(port)
        method_cases(port)
        body_cases(port)
        parameter_cases(port)
        failure_cases(port)
        dependency_smoke(port)
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        results = pool.map(lambda _: request(port, "GET", "/health"), range(32))
        assert all(result[::2] == (200, b'{"status":"ok"}') for result in results)


def build(memory):
    binary = ROOT / "target" / "fastapi" / f"e2e-{memory}"
    binary.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(ROOT / "target/release/osprey"), str(Path(__file__).parent / "service"),
                    "--compile", f"--memory={memory}", "--no-warnings", "-o", str(binary)], check=True, timeout=60)
    return binary


def wait_ready(process, port):
    for _ in range(100):
        if process.poll() is not None:
            raise RuntimeError(f"server exited {process.returncode}")
        try:
            assert request(port, "GET", "/health")[0] == 200
            return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError("server did not listen within 5 seconds")


def run(built):
    memory, binary = built
    port = {"gc": 18786, "arc": 18788}[memory]
    with binary.with_suffix(".log").open("w") as log:
        process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=log, stderr=log)
        try:
            process.stdin.write(f"{port}\n".encode())
            process.stdin.close()
            wait_ready(process, port)
            cases(port)
            print(f"FastAPI real HTTP E2E ({memory}): PASS", flush=True)
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as workers:
        list(workers.map(run, [(memory, build(memory)) for memory in ("gc", "arc")]))
