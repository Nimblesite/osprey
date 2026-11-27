"""Serve the local build and its read-only registry catalog on one loopback origin."""
from functools import partial
from http.client import HTTPConnection, HTTPException
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class Preview(SimpleHTTPRequestHandler):
    def end_headers(self):
        for line in (Path(self.directory) / "_headers").read_text().splitlines()[1:]:
            name, value = line.strip().split(":", 1)
            self.send_header(name, value.strip())
        super().end_headers()

    def do_GET(self):
        if self.path != "/v1/packages":
            return super().do_GET()
        connection = HTTPConnection("127.0.0.1", 8787, timeout=10)
        try:
            connection.request("GET", "/v1/packages")
            response = connection.getresponse()
            data = response.read(4 * 1024 * 1024 + 1)
            if len(data) > 4 * 1024 * 1024:
                self.send_error(502, "Catalog exceeds preview limit")
                return
        except (OSError, HTTPException):
            self.send_error(502, "Registry unavailable")
            return
        finally:
            connection.close()
        self.send_response(response.status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


if __name__ == "__main__":
    handler = partial(Preview, directory="target/registry-site-preview")
    ThreadingHTTPServer(("127.0.0.1", 4187), handler).serve_forever()
