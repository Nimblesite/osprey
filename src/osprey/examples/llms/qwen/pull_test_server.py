"""A local stand-in for huggingface.co for the offline pull test: the model
tree endpoint served from <dir>/tree.json and a Range-capable resolve endpoint
serving <dir>/<file>. Each request's method, path and Range header go to
stdout, which the test captures as its log."""
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

DIR, PORT_FILE = sys.argv[1], sys.argv[2]


class Hub(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):  # the default stderr log would duplicate ours
        pass

    def do_GET(self):
        print(f"{self.command} {self.path} range={self.headers.get('Range')}", flush=True)
        if self.path.startswith("/api/models/"):
            with open(os.path.join(DIR, "tree.json"), "rb") as tree:
                self._send(200, {"Content-Type": "application/json"}, tree.read())
        elif "/resolve/" in self.path:
            self._serve_file(os.path.basename(self.path))
        else:
            self._send(404, {"Content-Type": "text/plain"}, b"not found")

    def _serve_file(self, name):
        path = os.path.join(DIR, name)
        size = os.path.getsize(path)
        start = 0
        head = {"Content-Type": "application/octet-stream", "Accept-Ranges": "bytes"}
        rng = self.headers.get("Range")
        if rng:
            start = int(rng.removeprefix("bytes=").split("-")[0])
            if start >= size:
                self._send(416, {"Content-Range": f"bytes */{size}"}, b"")
                return
            head["Content-Range"] = f"bytes {start}-{size - 1}/{size}"
        with open(path, "rb") as f:
            f.seek(start)
            self._send(206 if rng else 200, head, f.read())

    def _send(self, code, head, body):
        self.send_response(code)
        for key, value in head.items():
            self.send_header(key, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = HTTPServer(("127.0.0.1", 0), Hub)
with open(PORT_FILE, "w") as port_file:
    port_file.write(str(server.server_address[1]))
print(f"listening on {server.server_address[1]}", flush=True)
server.serve_forever()
