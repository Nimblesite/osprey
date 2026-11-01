# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Media
"""Serving the file a device will fetch.

A Cast device plays a URL, so the sender serves the local file over HTTP on
the interface that routes to the device ([FFI-FILE-SERVER]) through the
`Wire` effect, and names the media after the file.
"""

from chromecast import Wire

__all__ = ["Served", "title", "media_url", "unserve"]


class Served:
    """The file's URL and the port of the server behind it."""
    url: str
    port: int


def base_name(path):
    parts = unwrap_or(split(path, "/"), [path])
    return unwrap_or(listGet(parts, len(parts) - 1), path)


def leading_dots(s):
    """How many `.` characters a name starts with."""
    return fold(range(0, length(s)), 0, lambda n, i: n + 1 if n == i and take(drop(s, i), 1) == "." else n)


def title(path):
    """The media title: the file's base name without its extension, as
    `os.path.splitext` cuts it (leading dots are not an extension)."""
    name = base_name(path)
    dots = leading_dots(name)
    parts = unwrap_or(split(drop(name, dots), "."), [])
    if len(parts) <= 1:
        return name
    else:
        kept = toList(map(range(0, len(parts) - 1), lambda i: unwrap_or(listGet(parts, i), "")))
        return take(name, dots) + join(kept, ".")


def media_url(path, host) -> Result[Served, str]:
    """Serves `path` on the interface that routes to `host` and returns its URL."""
    address = perform(Wire.Wire.route(host))
    port = perform(Wire.Wire.serve(path, "audio/mpeg", address)) if address != "" else -1
    if address == "":
        return Error(message=f"{host}: no route to the device")
    elif port < 1:
        return Error(message=f"{path}: cannot serve the file ({port})")
    else:
        return Success(value=Served(url=f"http://{address}:{port}/media", port=port))


def unserve(port):
    """Stops the server `media_url` started on `port`; 0, or -1 if none runs there."""
    return perform(Wire.Wire.unserve(port))
